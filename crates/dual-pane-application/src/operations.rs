use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dual_pane_domain::{DecisionToken, EntryKind, Location, OperationChoice, OperationId, OperationIntent, OperationIssue, OperationKind, starts_with};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedItem {
    pub source: Location,
    pub destination: Option<Location>,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationErrorKind {
    PermissionDenied,
    NotFound,
    NoSpace,
    Busy,
    ExecutorUnavailable,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationFailure {
    pub item: Location,
    pub kind: OperationErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFailure {
    pub failure: OperationFailure,
    pub entry_kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSkip {
    pub item: Location,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OperationProgress {
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
}
impl OperationProgress {
    fn total(self) -> Option<usize> {
        self.completed.checked_add(self.skipped)?.checked_add(self.failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    Succeeded,
    Partial,
    Failed,
    Cancelled,
    CleanupUncertain,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupResult {
    Clean,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationStatus {
    AwaitingConfirmation { targets: usize },
    Scanning,
    WaitingScan { item: Location, failure: OperationFailure, entry_kind: EntryKind, token: DecisionToken },
    Running { at: usize },
    Waiting { at: usize, item: Location, issue: OperationIssue, failure: Option<OperationFailure>, token: DecisionToken },
    Cancelling,
    NameCollision { item: Location },
    Finished(OperationOutcome),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationEffect {
    Scan { id: OperationId, generation: u64, intent: OperationIntent, skipped: Arc<[ScanSkip]> },
    Execute { id: OperationId, generation: u64, plan: Arc<[PlannedItem]>, skipped: Arc<[ScanSkip]>, at: usize, choice: Option<OperationChoice> },
    Cancel { id: OperationId, generation: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult {
    Advanced { next: usize, progress: OperationProgress },
    DecisionRequired { item: Location, issue: OperationIssue, destination_kind: EntryKind, progress: OperationProgress },
    RecoverableError { failure: OperationFailure, progress: OperationProgress },
    NameCollision { item: Location, progress: OperationProgress },
    Finished { progress: OperationProgress },
    Failed { failure: OperationFailure, progress: OperationProgress },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationJob {
    id: OperationId,
    intent: OperationIntent,
    status: OperationStatus,
    generation: u64,
    plan: Option<Arc<[PlannedItem]>>,
    progress: OperationProgress,
    failure: Option<OperationFailure>,
    scan_skips: Vec<ScanSkip>,
    scan_skip_positions: Arc<[usize]>,
    all_conflicts: Option<OperationChoice>,
    auto_attempted: Option<usize>,
    issued_choice: Option<OperationChoice>,
}
impl OperationJob {
    pub fn id(&self) -> OperationId {
        self.id
    }
    pub fn intent(&self) -> &OperationIntent {
        &self.intent
    }
    pub fn status(&self) -> &OperationStatus {
        &self.status
    }
    pub fn progress(&self) -> OperationProgress {
        self.progress
    }
    pub fn failure(&self) -> Option<&OperationFailure> {
        self.failure.as_ref()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn planned_count(&self) -> Option<usize> {
        self.plan.as_ref().map(|plan| plan.len())
    }
    pub fn scan_skips(&self) -> &[ScanSkip] {
        &self.scan_skips
    }
}

#[derive(Debug, Default)]
pub struct OperationCoordinator {
    next_id: u64,
    next_decision: u64,
    jobs: Vec<OperationJob>,
}
impl OperationCoordinator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn jobs(&self) -> &[OperationJob] {
        &self.jobs
    }
    pub fn job(&self, id: OperationId) -> Option<&OperationJob> {
        self.jobs.iter().find(|job| job.id == id)
    }
    fn job_mut(&mut self, id: OperationId) -> Option<&mut OperationJob> {
        self.jobs.iter_mut().find(|job| job.id == id)
    }
    pub fn start(&mut self, intent: OperationIntent) -> (OperationId, Vec<OperationEffect>) {
        self.next_id += 1;
        let id = OperationId::new(self.next_id);
        let confirmation = matches!(intent.kind(), OperationKind::DeletePermanently);
        let status = if confirmation { OperationStatus::AwaitingConfirmation { targets: intent.targets().len() } } else { OperationStatus::Scanning };
        let effects = if confirmation { vec![] } else { vec![OperationEffect::Scan { id, generation: 1, intent: intent.clone(), skipped: Arc::from([]) }] };
        self.jobs.push(OperationJob { id, intent, status, generation: if confirmation { 0 } else { 1 }, plan: None, progress: OperationProgress::default(), failure: None, scan_skips: vec![], scan_skip_positions: Arc::from([]), all_conflicts: None, auto_attempted: None, issued_choice: None });
        (id, effects)
    }
    pub fn confirm_delete(&mut self, id: OperationId, targets: usize) -> Option<Vec<OperationEffect>> {
        let job = self.job_mut(id)?;
        if job.status != (OperationStatus::AwaitingConfirmation { targets }) {
            return None;
        }
        job.status = OperationStatus::Scanning;
        job.generation += 1;
        Some(vec![OperationEffect::Scan { id, generation: job.generation, intent: job.intent.clone(), skipped: Arc::from([]) }])
    }
    pub fn scanned(&mut self, id: OperationId, generation: u64, plan: Result<Arc<[PlannedItem]>, ScanFailure>) -> Option<Vec<OperationEffect>> {
        let next_decision = self.next_decision.checked_add(1)?;
        self.next_decision = next_decision;
        let job = self.job_mut(id)?;
        if job.generation != generation || job.status != OperationStatus::Scanning {
            return None;
        }
        match plan {
            Err(scan_failure) if job.scan_skips.iter().any(|skip| starts_with(&scan_failure.failure.item, &skip.item)) => {
                job.failure = Some(scan_failure.failure);
                job.status = OperationStatus::Finished(OperationOutcome::Failed);
                Some(vec![])
            }
            Err(ScanFailure { failure, entry_kind }) if failure.kind != OperationErrorKind::ExecutorUnavailable && intent_roots(&job.intent).iter().any(|(root, kind)| starts_with(&failure.item, root) && (failure.item != *root || entry_kind == *kind) && (failure.item == *root || *kind == EntryKind::Directory && !matches!(job.intent.kind(), OperationKind::Rename { .. } | OperationKind::NewFolder { .. }))) => {
                job.failure = Some(failure.clone());
                job.status = OperationStatus::WaitingScan { item: failure.item.clone(), failure, entry_kind, token: DecisionToken::new(next_decision) };
                Some(vec![])
            }
            Ok(plan) if valid_plan(&job.intent, &job.scan_skips, &plan) => {
                if plan.is_empty() {
                    return None;
                }
                let skipped: HashSet<_> = job.scan_skips.iter().map(|skip| &skip.item).collect();
                job.scan_skip_positions = plan.iter().enumerate().filter_map(|(index, item)| skipped.contains(&item.source).then_some(index)).collect::<Vec<_>>().into();
                job.plan = Some(plan);
                Some(Self::execute(job, 0, None))
            }
            _ => None,
        }
    }
    fn execute(job: &mut OperationJob, at: usize, choice: Option<OperationChoice>) -> Vec<OperationEffect> {
        let Some(plan) = &job.plan else { return vec![] };
        job.generation += 1;
        job.status = OperationStatus::Running { at };
        job.issued_choice = choice;
        vec![OperationEffect::Execute { id: job.id, generation: job.generation, plan: Arc::clone(plan), skipped: Arc::from(job.scan_skips.clone()), at, choice }]
    }
    pub fn step_result(&mut self, id: OperationId, generation: u64, result: StepResult) -> Option<Vec<OperationEffect>> {
        let next_decision = self.next_decision.checked_add(1)?;
        self.next_decision = next_decision;
        let job = self.job_mut(id)?;
        let OperationStatus::Running { at } = job.status else { return None };
        if job.generation != generation {
            return None;
        }
        let plan = job.plan.as_ref()?;
        let len = plan.len();
        let progress = match &result {
            StepResult::Advanced { progress, .. } | StepResult::DecisionRequired { progress, .. } | StepResult::RecoverableError { progress, .. } | StepResult::NameCollision { progress, .. } | StepResult::Finished { progress } | StepResult::Failed { progress, .. } => *progress,
        };
        let count = progress.total()?;
        if job.progress.total() != Some(at) || progress.completed < job.progress.completed || progress.skipped < job.progress.skipped || progress.failed < job.progress.failed || count > len {
            return None;
        }
        if job.scan_skip_positions.binary_search(&at).is_ok() && !matches!(&result, StepResult::Advanced { .. } | StepResult::Finished { .. }) {
            return None;
        }
        if matches!(&result, StepResult::Advanced { .. } | StepResult::Finished { .. }) {
            let subtree_end = if job.issued_choice == Some(OperationChoice::Skip) { plan[at + 1..].iter().position(|item| !starts_with(&item.source, &plan[at].source)).map_or(len, |offset| at + 1 + offset) } else { at };
            if count < subtree_end || progress.failed != job.progress.failed {
                return None;
            }
            let scan_skips_after = job.scan_skip_positions.partition_point(|index| *index < count) - job.scan_skip_positions.partition_point(|index| *index < subtree_end);
            let expected_skips = subtree_end - at + scan_skips_after;
            if progress.skipped - job.progress.skipped != expected_skips {
                return None;
            }
        }
        let expected = &plan[at].source;
        match result {
            StepResult::Advanced { next, .. } if next > at && next < len && count == next => {
                job.progress = progress;
                job.auto_attempted = None;
                Some(Self::execute(job, next, None))
            }
            StepResult::DecisionRequired { item, issue, destination_kind, .. }
                if &item == expected
                    && count == at
                    && matches!(job.intent.kind(), OperationKind::Copy | OperationKind::Move)
                    && plan[at].destination.is_some()
                    && match issue {
                        OperationIssue::FileConflict => plan[at].kind == EntryKind::File && destination_kind == EntryKind::File,
                        OperationIssue::LinkCollision => matches!(plan[at].kind, EntryKind::Symlink { .. }) || matches!(destination_kind, EntryKind::Symlink { .. }),
                        OperationIssue::KindMismatch => matches!((plan[at].kind, destination_kind), (EntryKind::File, EntryKind::Directory) | (EntryKind::Directory, EntryKind::File)),
                        OperationIssue::RecoverableError => false,
                    } =>
            {
                job.progress = progress;
                if issue == OperationIssue::FileConflict
                    && job.auto_attempted != Some(at)
                    && let Some(choice) = job.all_conflicts
                {
                    job.auto_attempted = Some(at);
                    return Some(Self::execute(job, at, Some(choice)));
                }
                job.status = OperationStatus::Waiting { at, item, issue, failure: None, token: DecisionToken::new(next_decision) };
                Some(vec![])
            }
            StepResult::RecoverableError { failure, .. } if failure.item == *expected && failure.kind != OperationErrorKind::ExecutorUnavailable && count == at => {
                job.progress = progress;
                job.status = OperationStatus::Waiting { at, item: failure.item.clone(), issue: OperationIssue::RecoverableError, failure: Some(failure), token: DecisionToken::new(next_decision) };
                Some(vec![])
            }
            StepResult::NameCollision { item, .. } if &item == expected && count == at && matches!(job.intent.kind(), OperationKind::Rename { .. } | OperationKind::NewFolder { .. }) => {
                job.progress = progress;
                job.status = OperationStatus::NameCollision { item };
                Some(vec![])
            }
            StepResult::Finished { .. } if count == len => {
                job.progress = progress;
                job.status = OperationStatus::Finished(if progress.failed > 0 || progress.skipped > 0 { OperationOutcome::Partial } else { OperationOutcome::Succeeded });
                Some(vec![])
            }
            StepResult::Failed { failure, .. } if failure.item == *expected && progress.failed == job.progress.failed + 1 && progress.completed == job.progress.completed && progress.skipped == job.progress.skipped && count == at + 1 => {
                job.progress = progress;
                job.failure = Some(failure);
                job.status = OperationStatus::Finished(if progress.completed > 0 || progress.skipped > 0 { OperationOutcome::Partial } else { OperationOutcome::Failed });
                Some(vec![])
            }
            _ => None,
        }
    }
    pub fn decide(&mut self, id: OperationId, token: DecisionToken, item: &Location, choice: OperationChoice, apply_to_all: bool) -> Option<Vec<OperationEffect>> {
        if let OperationStatus::WaitingScan { item: pending, entry_kind, token: expected, .. } = self.job(id)?.status().clone() {
            if pending != *item || expected != token || !OperationIssue::RecoverableError.permits(choice, apply_to_all) {
                return None;
            }
            if choice == OperationChoice::Cancel {
                return self.cancel(id);
            }
            let job = self.job_mut(id)?;
            if choice == OperationChoice::Skip {
                job.scan_skips.retain(|old| !starts_with(&old.item, item));
                job.scan_skips.push(ScanSkip { item: item.clone(), kind: entry_kind });
            } else if job.scan_skips.is_empty() {
                job.failure = None;
            }
            job.generation += 1;
            job.status = OperationStatus::Scanning;
            return Some(vec![OperationEffect::Scan { id, generation: job.generation, intent: job.intent.clone(), skipped: Arc::from(job.scan_skips.clone()) }]);
        }
        let job = self.job_mut(id)?;
        let OperationStatus::Waiting { at, item: ref pending, issue, token: expected, .. } = job.status else { return None };
        if pending != item || token != expected || !issue.permits(choice, apply_to_all) {
            return None;
        }
        if choice == OperationChoice::Cancel {
            return self.cancel(id);
        }
        if apply_to_all {
            job.all_conflicts = Some(choice);
        }
        job.auto_attempted = Some(at);
        Some(Self::execute(job, at, Some(choice)))
    }
    pub fn cancel(&mut self, id: OperationId) -> Option<Vec<OperationEffect>> {
        let job = self.job_mut(id)?;
        if matches!(job.status, OperationStatus::Finished(_) | OperationStatus::NameCollision { .. } | OperationStatus::Cancelling) {
            return None;
        }
        if matches!(job.status, OperationStatus::AwaitingConfirmation { .. }) {
            job.status = OperationStatus::Finished(OperationOutcome::Cancelled);
            return Some(vec![]);
        }
        job.generation += 1;
        job.status = OperationStatus::Cancelling;
        Some(vec![OperationEffect::Cancel { id, generation: job.generation }])
    }
    pub fn cleaned(&mut self, id: OperationId, generation: u64, result: CleanupResult) -> Option<Vec<OperationEffect>> {
        let job = self.job_mut(id)?;
        if job.generation != generation || job.status != OperationStatus::Cancelling {
            return None;
        }
        job.status = OperationStatus::Finished(match result {
            CleanupResult::Uncertain => OperationOutcome::CleanupUncertain,
            CleanupResult::Clean if job.progress.completed > 0 => OperationOutcome::Partial,
            CleanupResult::Clean => OperationOutcome::Cancelled,
        });
        Some(vec![])
    }
    pub fn unavailable(&mut self, id: OperationId, generation: u64) -> Option<Vec<OperationEffect>> {
        let job = self.job_mut(id)?;
        if job.generation != generation {
            return None;
        }
        match job.status {
            OperationStatus::Scanning | OperationStatus::Running { .. } => {
                if let Some((item, _)) = intent_roots(&job.intent).into_iter().next() {
                    job.failure = Some(OperationFailure { item, kind: OperationErrorKind::ExecutorUnavailable });
                }
                job.status = OperationStatus::Finished(if job.progress.completed > 0 || job.progress.skipped > 0 { OperationOutcome::Partial } else { OperationOutcome::Failed });
                Some(vec![])
            }
            OperationStatus::Cancelling => self.cleaned(id, generation, CleanupResult::Uncertain),
            _ => None,
        }
    }
}

fn valid_plan(intent: &OperationIntent, skips: &[ScanSkip], plan: &[PlannedItem]) -> bool {
    let roots = intent_roots(intent);
    let source_depth = intent.source().components().len();
    let root_indices: HashMap<_, _> = roots.iter().enumerate().map(|(index, (root, _))| (root.components()[source_depth].clone(), index)).collect();
    let skip_kinds: HashMap<_, _> = skips.iter().map(|skip| (&skip.item, skip.kind)).collect();
    if skip_kinds.len() != skips.len() {
        return false;
    }
    let mut seen = HashMap::new();
    let mut closed = HashSet::new();
    let mut previous: Option<&Location> = None;
    let mut next_root = 0;
    for item in plan {
        let mut departed = previous.cloned();
        while let Some(location) = departed {
            if !starts_with(&item.source, &location) {
                closed.insert(location.clone());
            }
            departed = location.parent();
        }
        let mut ancestor = Some(item.source.clone());
        while let Some(location) = ancestor {
            if closed.contains(&location) || location != item.source && skip_kinds.contains_key(&location) {
                return false;
            }
            ancestor = location.parent();
        }
        if seen.insert(item.source.clone(), item.kind).is_some() {
            return false;
        }
        let Some(root_index) = item.source.components().get(source_depth).and_then(|name| root_indices.get(name)).copied() else { return false };
        let (root, kind) = &roots[root_index];
        if !starts_with(&item.source, root) {
            return false;
        }
        if &item.source == root {
            if item.kind != *kind || root_index != next_root {
                return false;
            }
            next_root += 1;
        } else {
            if root_index + 1 != next_root || matches!(intent.kind(), OperationKind::Rename { .. } | OperationKind::NewFolder { .. }) {
                return false;
            }
            let Some(parent) = item.source.parent() else { return false };
            if seen.get(&parent) != Some(&EntryKind::Directory) {
                return false;
            }
        }
        let destination = match intent.kind() {
            OperationKind::Copy | OperationKind::Move => intent.destination().map(|dest| Location::from_components(dest.components().iter().cloned().chain(item.source.components()[intent.source().components().len()..].iter().cloned()))),
            OperationKind::Rename { to } => Some(intent.source().join(to)),
            _ => None,
        };
        if item.destination != destination {
            return false;
        }
        previous = Some(&item.source);
    }
    next_root == roots.len() && skips.iter().all(|skip| seen.get(&skip.item) == Some(&skip.kind))
}

fn intent_roots(intent: &OperationIntent) -> Vec<(Location, EntryKind)> {
    match intent.kind() {
        OperationKind::NewFolder { name } => vec![(intent.source().join(name), EntryKind::Directory)],
        _ => intent.targets().iter().map(|target| (intent.source().join(&target.name), target.kind)).collect(),
    }
}
