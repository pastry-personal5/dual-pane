//! Presentation of file operations: one Operation Panel per job with its
//! progress and any Operation Decision Card, Permanent Delete confirmations,
//! inline-editor outcomes, and the quit prompt. The presenter owns the panel
//! reveal and hide delays, measured on an injected clock, and reports which
//! finished jobs the application should dismiss.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dual_pane_application::{OperationJob, OperationOutcome, OperationStatus, Output};
use dual_pane_domain::{DecisionToken, Location, OperationChoice, OperationId, OperationIssue, OperationKind, starts_with};

use crate::format::{format_size, location_text};
use crate::operation_presenter::{name_problem_text, operation_error_text, operation_summary, operation_title};

/// Time since an arbitrary start, for the reveal and hide delays.
pub type Ticks = Arc<dyn Fn() -> Duration + Send + Sync>;

/// A job that outlasts this shows its panel.
pub const REVEAL_DELAY: Duration = Duration::from_millis(500);
/// A finished panel that needs no review hides after this.
pub const HIDE_DELAY: Duration = Duration::from_secs(3);

/// One choice on an Operation Decision Card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceViewModel {
    pub choice: OperationChoice,
    pub label: &'static str,
}

/// A job's pending decision. It has no default choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionCardViewModel {
    pub token: DecisionToken,
    pub item: Location,
    pub message: String,
    /// The permitted choices in a fixed order.
    pub choices: Vec<ChoiceViewModel>,
    /// Whether the card offers the initially unchecked “apply to all”.
    pub offers_apply_to_all: bool,
}

/// One Operation Panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPanelViewModel {
    pub id: OperationId,
    pub title: String,
    pub detail: String,
    pub status: String,
    /// Entries handled and planned, or `None` while the plan is unknown.
    pub progress: Option<(usize, usize)>,
    /// The current file's copied bytes, when known.
    pub bytes: Option<String>,
    pub finished: bool,
    pub can_cancel: bool,
    pub decision: Option<DecisionCardViewModel>,
}

/// What an inline name editor shows for its job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorOutcome {
    /// The job is still deciding whether the name works.
    Pending,
    /// The name was refused; the editor stays open with this text.
    Rejected(&'static str),
    /// The Item was created or renamed.
    Succeeded,
    /// The editor closes; any failure shows on the job's panel.
    Closed,
}

#[derive(Debug, Clone, Default)]
pub struct OperationsViewModel {
    panels: Vec<OperationPanelViewModel>,
    panels_revision: u64,
    confirmations: Vec<(OperationId, usize)>,
    confirmations_revision: u64,
    quit_prompt: Option<usize>,
    quit_prompt_revision: u64,
    quit_accepted: bool,
}

impl OperationsViewModel {
    /// Revealed panels in the order their jobs started.
    pub fn panels(&self) -> &[OperationPanelViewModel] {
        &self.panels
    }
    pub fn panel(&self, id: OperationId) -> Option<&OperationPanelViewModel> {
        self.panels.iter().find(|panel| panel.id == id)
    }
    pub fn panels_revision(&self) -> u64 {
        self.panels_revision
    }
    /// Permanent deletions waiting for confirmation, with their frozen
    /// target counts.
    pub fn confirmations(&self) -> &[(OperationId, usize)] {
        &self.confirmations
    }
    pub fn confirmations_revision(&self) -> u64 {
        self.confirmations_revision
    }
    /// The running operation count to ask about before quitting.
    pub fn quit_prompt(&self) -> Option<usize> {
        self.quit_prompt
    }
    pub fn quit_prompt_revision(&self) -> u64 {
        self.quit_prompt_revision
    }
    pub fn quit_accepted(&self) -> bool {
        self.quit_accepted
    }
}

struct Tracked {
    job: OperationJob,
    started: Duration,
    revealed: bool,
    finished_at: Option<Duration>,
}

/// Keeps an [`OperationsViewModel`] up to date.
pub struct OperationsPresenter {
    ticks: Ticks,
    jobs: HashMap<OperationId, Tracked>,
    order: Vec<OperationId>,
    view: OperationsViewModel,
    dismissals: Vec<OperationId>,
}

impl Default for OperationsPresenter {
    fn default() -> Self {
        let start = Instant::now();
        Self::with_ticks(Arc::new(move || start.elapsed()))
    }
}

impl OperationsPresenter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_ticks(ticks: Ticks) -> Self {
        Self { ticks, jobs: HashMap::new(), order: Vec::new(), view: OperationsViewModel::default(), dismissals: Vec::new() }
    }
    pub fn view(&self) -> &OperationsViewModel {
        &self.view
    }

    pub fn apply(&mut self, output: &Output) {
        match output {
            Output::OperationChanged { job } => self.changed(job),
            Output::OperationDismissed { id } => {
                self.jobs.remove(id);
                self.order.retain(|tracked| tracked != id);
                self.dismissals.retain(|dismissal| dismissal != id);
                self.rebuild();
            }
            Output::QuitConfirmationRequired { running } => {
                self.view.quit_prompt = Some(*running);
                self.view.quit_prompt_revision = self.view.quit_prompt_revision.wrapping_add(1);
            }
            Output::QuitAccepted => {
                self.view.quit_prompt = None;
                self.view.quit_accepted = true;
            }
            _ => {}
        }
    }

    fn changed(&mut self, job: &OperationJob) {
        let now = (self.ticks)();
        let id = job.id();
        let tracked = self.jobs.entry(id).or_insert_with(|| Tracked { job: job.clone(), started: now, revealed: false, finished_at: None });
        if !self.order.contains(&id) {
            self.order.push(id);
        }
        tracked.job = job.clone();
        match job.status() {
            OperationStatus::Finished(outcome) => {
                tracked.finished_at.get_or_insert(now);
                if needs_review(*outcome) {
                    tracked.revealed = true;
                } else if !tracked.revealed {
                    // It finished before its panel appeared: the Notice is the
                    // only trace, so the job is dismissed at once.
                    self.dismissals.push(id);
                }
            }
            _ if job.decision().is_some() => tracked.revealed = true,
            _ => {}
        }
        self.tick();
    }

    /// Reveals panels that outlasted the reveal delay and hides finished
    /// panels whose hide delay passed. Call it when [`Self::next_deadline`]
    /// arrives.
    pub fn tick(&mut self) {
        let now = (self.ticks)();
        for (id, tracked) in &mut self.jobs {
            if self.dismissals.contains(id) {
                continue;
            }
            let finished = matches!(tracked.job.status(), OperationStatus::Finished(_));
            let editing = tracked.job.name_problem().is_some() || matches!(tracked.job.status(), OperationStatus::AwaitingConfirmation { .. });
            if !tracked.revealed && !finished && !editing && now.saturating_sub(tracked.started) >= REVEAL_DELAY {
                tracked.revealed = true;
            }
            if tracked.revealed
                && let (OperationStatus::Finished(outcome), Some(finished_at)) = (tracked.job.status(), tracked.finished_at)
                && !needs_review(*outcome)
                && now.saturating_sub(finished_at) >= HIDE_DELAY
            {
                self.dismissals.push(*id);
            }
        }
        self.rebuild();
    }

    /// When [`Self::tick`] next has something to do.
    pub fn next_deadline(&self) -> Option<Duration> {
        let now = (self.ticks)();
        self.jobs
            .iter()
            .filter(|(id, _)| !self.dismissals.contains(id))
            .filter_map(|(_, tracked)| match (tracked.job.status(), tracked.finished_at) {
                (OperationStatus::Finished(outcome), Some(finished_at)) if tracked.revealed && !needs_review(*outcome) => Some(finished_at + HIDE_DELAY),
                (OperationStatus::Finished(_), _) => None,
                (OperationStatus::AwaitingConfirmation { .. } | OperationStatus::NameCollision { .. } | OperationStatus::NameRejected { .. }, _) => None,
                _ if !tracked.revealed => Some(tracked.started + REVEAL_DELAY),
                _ => None,
            })
            .min()
            .map(|deadline| deadline.saturating_sub(now))
    }

    /// The person closed a panel. A finished one is dismissed; a running or
    /// waiting one stays.
    pub fn close(&mut self, id: OperationId) -> bool {
        let closable = self.jobs.get(&id).is_some_and(|tracked| matches!(tracked.job.status(), OperationStatus::Finished(_)));
        if closable && !self.dismissals.contains(&id) {
            self.dismissals.push(id);
            self.rebuild();
        }
        closable
    }

    /// Finished jobs to dismiss in the application, each reported once.
    pub fn take_dismissals(&mut self) -> Vec<OperationId> {
        std::mem::take(&mut self.dismissals).into_iter().filter(|id| self.jobs.contains_key(id)).collect()
    }

    /// What an inline editor shows for the job its commit started.
    pub fn editor_outcome(&self, id: OperationId) -> EditorOutcome {
        let Some(tracked) = self.jobs.get(&id) else { return EditorOutcome::Closed };
        match tracked.job.status() {
            _ if tracked.job.name_problem().is_some() => tracked.job.name_problem().map_or(EditorOutcome::Closed, |problem| EditorOutcome::Rejected(name_problem_text(problem))),
            OperationStatus::Scanning | OperationStatus::Running { .. } => EditorOutcome::Pending,
            OperationStatus::Finished(OperationOutcome::Succeeded) => EditorOutcome::Succeeded,
            _ => EditorOutcome::Closed,
        }
    }

    fn rebuild(&mut self) {
        let panels = self.order.iter().filter(|id| !self.dismissals.contains(id)).filter_map(|id| self.jobs.get(id)).filter(|tracked| tracked.revealed).map(|tracked| panel(&tracked.job)).collect::<Vec<_>>();
        if panels != self.view.panels {
            self.view.panels = panels;
            self.view.panels_revision = self.view.panels_revision.wrapping_add(1);
        }
        let confirmations = self
            .order
            .iter()
            .filter_map(|id| self.jobs.get(id))
            .filter_map(|tracked| match tracked.job.status() {
                OperationStatus::AwaitingConfirmation { targets } => Some((tracked.job.id(), *targets)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if confirmations != self.view.confirmations {
            self.view.confirmations = confirmations;
            self.view.confirmations_revision = self.view.confirmations_revision.wrapping_add(1);
        }
    }
}

/// Partial, failed, and uncertain results stay for review.
fn needs_review(outcome: OperationOutcome) -> bool {
    matches!(outcome, OperationOutcome::Partial | OperationOutcome::Failed | OperationOutcome::CleanupUncertain)
}

fn panel(job: &OperationJob) -> OperationPanelViewModel {
    let intent = job.intent();
    let subject = match (intent.kind(), intent.targets()) {
        (OperationKind::NewFolder { name }, _) => format!("“{}”", name.to_text_lossy()),
        (_, [only]) => format!("“{}”", only.name.to_text_lossy()),
        (_, targets) => format!("{} items", targets.len()),
    };
    let detail = match (intent.kind(), intent.destination()) {
        (OperationKind::Rename { to }, _) => format!("{subject} to “{}”", to.to_text_lossy()),
        (_, Some(destination)) => format!("{subject} to “{}”", location_text(destination)),
        _ => format!("{subject} in “{}”", location_text(intent.source())),
    };
    let progress = job.planned_count().map(|planned| {
        let done = job.progress();
        (done.completed + done.skipped + done.failed, planned)
    });
    let finished = matches!(job.status(), OperationStatus::Finished(_));
    let status = match job.status() {
        OperationStatus::Finished(outcome) => operation_summary(intent, *outcome, job.progress(), job.failure()),
        OperationStatus::AwaitingConfirmation { .. } => "Waiting for confirmation".to_owned(),
        OperationStatus::Scanning => "Preparing…".to_owned(),
        OperationStatus::WaitingScan { .. } | OperationStatus::Waiting { .. } | OperationStatus::WaitingFinalize { .. } => "Waiting for your decision".to_owned(),
        OperationStatus::Finalizing { .. } => "Removing emptied folders…".to_owned(),
        OperationStatus::Cancelling => "Cancelling…".to_owned(),
        OperationStatus::NameCollision { .. } | OperationStatus::NameRejected { .. } => "Waiting for a new name".to_owned(),
        OperationStatus::Running { .. } => match intent.kind() {
            OperationKind::Copy => "Copying…",
            OperationKind::Move => "Moving…",
            OperationKind::Rename { .. } => "Renaming…",
            OperationKind::NewFolder { .. } => "Creating folder…",
            OperationKind::MoveToTrash => "Moving to the Trash…",
            OperationKind::DeletePermanently => "Deleting…",
        }
        .to_owned(),
    };
    OperationPanelViewModel { id: job.id(), title: operation_title(intent.kind()).to_owned(), detail, status, progress, bytes: job.bytes().filter(|bytes| bytes.total > 0).map(|bytes| format!("{} of {}", format_size(bytes.done), format_size(bytes.total))), finished, can_cancel: !finished && !matches!(job.status(), OperationStatus::Cancelling), decision: decision_card(job) }
}

fn decision_card(job: &OperationJob) -> Option<DecisionCardViewModel> {
    let decision = job.decision()?;
    let intent = job.intent();
    let name = decision.item.components().last().map(|name| name.to_text_lossy().into_owned()).unwrap_or_default();
    // Where the item would go, for a conflict in Copy or Move.
    let destination = intent.destination().map(|destination| Location::from_components(destination.components().iter().cloned().chain(decision.item.components().iter().skip(intent.source().components().len()).cloned())));
    let folder = destination.as_ref().and_then(Location::parent).map(|folder| location_text(&folder)).unwrap_or_default();
    let message = match decision.issue {
        OperationIssue::FileConflict => format!("An item named “{name}” already exists in “{folder}”."),
        OperationIssue::LinkCollision => format!("“{name}” can’t replace, or be replaced by, a link in “{folder}”."),
        OperationIssue::KindMismatch => format!("“{name}” and the item with its name in “{folder}” are a file and a folder, so neither replaces the other."),
        OperationIssue::RecoverableError => {
            let reason = decision.failure.map_or("an unexpected error occurred", |failure| operation_error_text(failure.kind));
            let removing = starts_with(decision.item, intent.source()) && matches!(job.status(), OperationStatus::WaitingFinalize { .. });
            let scanning = matches!(job.status(), OperationStatus::WaitingScan { .. });
            let verb = if removing {
                "remove the folder"
            } else if scanning {
                "read"
            } else {
                match intent.kind() {
                    OperationKind::Copy => "copy",
                    OperationKind::Move => "move",
                    OperationKind::Rename { .. } => "rename",
                    OperationKind::NewFolder { .. } => "create",
                    OperationKind::MoveToTrash => "move to the Trash",
                    OperationKind::DeletePermanently => "delete",
                }
            };
            format!("Couldn’t {verb} “{}” because {reason}.", location_text(decision.item))
        }
    };
    let order = [(OperationChoice::Replace, "Replace"), (OperationChoice::TryAgain, "Try Again"), (OperationChoice::Skip, "Skip"), (OperationChoice::Cancel, "Cancel")];
    let choices = order.into_iter().filter(|(choice, _)| decision.issue.permits(*choice, false)).map(|(choice, label)| ChoiceViewModel { choice, label }).collect();
    Some(DecisionCardViewModel { token: decision.token, item: decision.item.clone(), message, choices, offers_apply_to_all: decision.issue == OperationIssue::FileConflict })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use dual_pane_application::{CleanupResult, Command, Event, OperationEffect, OperationProgress, PlannedItem, StepResult, WorkRequest, Workspace};
    use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName};

    use super::*;

    fn name(text: &str) -> EntryName {
        EntryName::new(text).unwrap()
    }
    fn path(parts: &[&str]) -> Location {
        parts.iter().fold(Location::root(), |location, part| location.join(&name(part)))
    }

    /// A workspace with `from` on the left and `to` on the right, a presenter
    /// on a hand-moved clock, and the effects the workspace asked for.
    struct Fixture {
        ws: Workspace,
        presenter: OperationsPresenter,
        clock: Arc<Mutex<Duration>>,
        work: Vec<WorkRequest>,
    }

    impl Fixture {
        fn new(entries: &[&str]) -> Self {
            let clock = Arc::new(Mutex::new(Duration::ZERO));
            let ticks = Arc::clone(&clock);
            let mut fixture = Self { ws: Workspace::new(), presenter: OperationsPresenter::with_ticks(Arc::new(move || *ticks.lock().unwrap())), clock, work: Vec::new() };
            for (browser, location, rows) in [(BrowserSide::Left, path(&["from"]), entries), (BrowserSide::Right, path(&["to"]), &[][..])] {
                let read = fixture.ws.handle(Command::Navigate { browser, location }.into());
                let Some(WorkRequest::ReadDirectory { tab, token, .. }) = read.work.into_iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) else { panic!() };
                fixture.ws.handle(Event::FolderItemsLoaded { browser, tab, token, entries: rows.iter().map(|text| Entry::new(name(text), EntryKind::File)).collect(), changes: None }.into());
            }
            fixture
        }
        fn handle(&mut self, input: impl Into<dual_pane_application::Input>) {
            let transition = self.ws.handle(input.into());
            for output in &transition.outputs {
                self.presenter.apply(output);
            }
            self.work.extend(transition.work);
        }
        fn advance(&mut self, by: Duration) {
            *self.clock.lock().unwrap() += by;
            self.presenter.tick();
        }
        fn start(&mut self, names: &[&str], kind: OperationKind) -> OperationId {
            let tab = self.ws.active_tab(BrowserSide::Left);
            let entries = self.ws.entries(BrowserSide::Left).to_vec();
            for (index, text) in names.iter().enumerate() {
                let row = entries.iter().position(|entry| entry.name() == &name(text)).unwrap();
                self.handle(if index == 0 { Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: name(text) } } else { Command::ToggleEntry { browser: BrowserSide::Left, tab, row, name: name(text) } });
            }
            self.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind });
            self.ws.operation_jobs().last().unwrap().id()
        }
        /// The latest effect for `id` and its generation.
        fn effect(&self, id: OperationId) -> (u64, OperationEffect) {
            self.work
                .iter()
                .rev()
                .find_map(|work| match work {
                    WorkRequest::Operation(effect @ (OperationEffect::Scan { id: owner, generation, .. } | OperationEffect::Execute { id: owner, generation, .. } | OperationEffect::Cancel { id: owner, generation })) if *owner == id => Some((*generation, effect.clone())),
                    _ => None,
                })
                .unwrap()
        }
        fn scan(&mut self, id: OperationId, names: &[&str]) {
            let (generation, _) = self.effect(id);
            let plan = names.iter().map(|text| PlannedItem { source: path(&["from", text]), destination: Some(path(&["to", text])), kind: EntryKind::File }).collect::<Vec<_>>();
            self.handle(Event::OperationScanned { id, generation, plan: Ok(plan.into()) });
        }
        fn step(&mut self, id: OperationId, result: StepResult) {
            let (generation, _) = self.effect(id);
            self.handle(Event::OperationStepped { id, generation, result });
        }
    }

    fn progress(completed: usize, skipped: usize) -> OperationProgress {
        OperationProgress { completed, skipped, failed: 0 }
    }

    #[test]
    fn a_panel_appears_after_the_reveal_delay_and_hides_after_success() {
        let mut fixture = Fixture::new(&["a", "b"]);
        let id = fixture.start(&["a", "b"], OperationKind::Copy);
        assert!(fixture.presenter.view().panels().is_empty(), "a quick job shows nothing at first");
        assert_eq!(fixture.presenter.next_deadline(), Some(REVEAL_DELAY));
        fixture.advance(Duration::from_millis(499));
        assert!(fixture.presenter.view().panels().is_empty());
        fixture.advance(Duration::from_millis(1));
        let panel = fixture.presenter.view().panel(id).cloned().expect("revealed");
        assert_eq!((panel.title.as_str(), panel.detail.as_str(), panel.status.as_str(), panel.progress), ("Copy", "2 items to “/to”", "Preparing…", None));
        fixture.scan(id, &["a", "b"]);
        assert_eq!(fixture.presenter.view().panel(id).unwrap().progress, Some((0, 2)));
        fixture.handle(Event::OperationProgress { id, generation: fixture.effect(id).0, bytes: dual_pane_application::ByteProgress { done: 1_500, total: 3_000 } });
        assert_eq!(fixture.presenter.view().panel(id).unwrap().bytes.as_deref(), Some("1.5 KB of 3.0 KB"));
        fixture.step(id, StepResult::Finished { progress: progress(2, 0) });
        assert_eq!(fixture.presenter.view().panel(id).unwrap().status, "Copied 2 items to “/to”.");
        assert!(fixture.presenter.take_dismissals().is_empty());
        assert_eq!(fixture.presenter.next_deadline(), Some(HIDE_DELAY));
        fixture.advance(HIDE_DELAY);
        assert!(fixture.presenter.view().panels().is_empty());
        assert_eq!(fixture.presenter.take_dismissals(), vec![id]);
        assert!(fixture.presenter.take_dismissals().is_empty(), "each dismissal is reported once");
    }

    #[test]
    fn a_quick_job_is_dismissed_once_its_notice_is_recorded_and_never_shows_a_panel() {
        let mut fixture = Fixture::new(&["a"]);
        let id = fixture.start(&["a"], OperationKind::Copy);
        fixture.scan(id, &["a"]);
        fixture.step(id, StepResult::Finished { progress: progress(1, 0) });
        assert!(fixture.presenter.view().panels().is_empty());
        assert_eq!(fixture.presenter.take_dismissals(), vec![id]);
        fixture.handle(Command::DismissOperation { id });
        assert!(fixture.ws.operation_jobs().is_empty());
        assert_eq!(fixture.presenter.next_deadline(), None);
    }

    #[test]
    fn a_decision_shows_at_once_with_exactly_the_permitted_choices() {
        let mut fixture = Fixture::new(&["a"]);
        let id = fixture.start(&["a"], OperationKind::Copy);
        fixture.scan(id, &["a"]);
        fixture.step(id, StepResult::DecisionRequired { item: path(&["from", "a"]), issue: OperationIssue::FileConflict, destination_kind: EntryKind::File, progress: progress(0, 0) });
        let card = fixture.presenter.view().panel(id).and_then(|panel| panel.decision.clone()).expect("a card without waiting for the delay");
        assert_eq!(card.message, "An item named “a” already exists in “/to”.");
        assert_eq!(card.choices.iter().map(|choice| choice.label).collect::<Vec<_>>(), ["Replace", "Skip", "Cancel"]);
        assert!(card.offers_apply_to_all);
        fixture.handle(Command::DecideOperation { id, token: card.token, item: card.item, choice: OperationChoice::Skip, apply_to_all: false });
        assert!(fixture.presenter.view().panel(id).unwrap().decision.is_none());
        fixture.step(id, StepResult::RecoverableError { failure: dual_pane_application::OperationFailure { item: path(&["from", "a"]), kind: dual_pane_application::OperationErrorKind::NoSpace }, progress: progress(0, 0) });
        let card = fixture.presenter.view().panel(id).and_then(|panel| panel.decision.clone()).unwrap();
        assert_eq!(card.message, "Couldn’t copy “/from/a” because there isn’t enough space.");
        assert_eq!(card.choices.iter().map(|choice| choice.label).collect::<Vec<_>>(), ["Try Again", "Skip", "Cancel"]);
        assert!(!card.offers_apply_to_all, "only a file conflict offers apply to all");
    }

    #[test]
    fn concurrent_jobs_keep_independent_panels_and_cards() {
        let mut fixture = Fixture::new(&["a", "b"]);
        let first = fixture.start(&["a"], OperationKind::Copy);
        let second = fixture.start(&["b"], OperationKind::Copy);
        fixture.scan(first, &["a"]);
        fixture.scan(second, &["b"]);
        fixture.step(first, StepResult::DecisionRequired { item: path(&["from", "a"]), issue: OperationIssue::FileConflict, destination_kind: EntryKind::File, progress: progress(0, 0) });
        fixture.step(second, StepResult::DecisionRequired { item: path(&["from", "b"]), issue: OperationIssue::LinkCollision, destination_kind: EntryKind::Symlink { points_to_directory: false }, progress: progress(0, 0) });
        let panels = fixture.presenter.view().panels();
        assert_eq!(panels.iter().map(|panel| panel.id).collect::<Vec<_>>(), [first, second]);
        assert_ne!(panels[0].decision.as_ref().unwrap().token, panels[1].decision.as_ref().unwrap().token);
        assert_eq!(panels[1].decision.as_ref().unwrap().choices.iter().map(|choice| choice.label).collect::<Vec<_>>(), ["Try Again", "Skip", "Cancel"]);
    }

    #[test]
    fn partial_and_failed_results_stay_until_closed() {
        let mut fixture = Fixture::new(&["a", "b"]);
        let id = fixture.start(&["a", "b"], OperationKind::Copy);
        fixture.scan(id, &["a", "b"]);
        fixture.step(id, StepResult::DecisionRequired { item: path(&["from", "a"]), issue: OperationIssue::FileConflict, destination_kind: EntryKind::File, progress: progress(0, 0) });
        let card = fixture.presenter.view().panel(id).and_then(|panel| panel.decision.clone()).unwrap();
        fixture.handle(Command::DecideOperation { id, token: card.token, item: card.item, choice: OperationChoice::Skip, apply_to_all: false });
        fixture.step(id, StepResult::Finished { progress: progress(1, 1) });
        assert!(fixture.presenter.view().panel(id).unwrap().finished);
        fixture.advance(Duration::from_secs(60));
        assert!(fixture.presenter.view().panel(id).is_some(), "a partial result stays for review");
        assert!(fixture.presenter.take_dismissals().is_empty());
        assert!(fixture.presenter.close(id));
        assert!(fixture.presenter.view().panels().is_empty());
        assert_eq!(fixture.presenter.take_dismissals(), vec![id]);
    }

    #[test]
    fn a_running_or_waiting_panel_cannot_be_closed() {
        let mut fixture = Fixture::new(&["a"]);
        let id = fixture.start(&["a"], OperationKind::Copy);
        fixture.advance(REVEAL_DELAY);
        assert!(!fixture.presenter.close(id));
        assert!(fixture.presenter.view().panel(id).unwrap().can_cancel);
        fixture.handle(Command::CancelOperation { id });
        let panel = fixture.presenter.view().panel(id).unwrap();
        assert_eq!((panel.status.as_str(), panel.can_cancel), ("Cancelling…", false));
        assert!(!fixture.presenter.close(id));
        let (generation, _) = fixture.effect(id);
        fixture.handle(Event::OperationCleaned { id, generation, result: CleanupResult::Clean });
        assert_eq!(fixture.presenter.view().panel(id).unwrap().status, "Copy of “a” was cancelled.");
        fixture.advance(HIDE_DELAY);
        assert_eq!(fixture.presenter.take_dismissals(), vec![id], "a clean cancellation hides after the delay");
    }

    #[test]
    fn a_permanent_deletion_waits_for_confirmation_without_a_panel() {
        let mut fixture = Fixture::new(&["a", "b"]);
        let id = fixture.start(&["a", "b"], OperationKind::DeletePermanently);
        assert_eq!(fixture.presenter.view().confirmations(), &[(id, 2)]);
        fixture.advance(Duration::from_secs(10));
        assert!(fixture.presenter.view().panels().is_empty(), "a confirmation is not a running job");
        fixture.handle(Command::CancelOperation { id });
        assert!(fixture.presenter.view().confirmations().is_empty());
        assert_eq!(fixture.presenter.take_dismissals(), vec![id], "the closed confirmation leaves nothing behind");
    }

    #[test]
    fn editor_outcomes_follow_the_job() {
        let mut fixture = Fixture::new(&["a"]);
        let id = fixture.start(&["a"], OperationKind::Rename { to: name("b") });
        assert_eq!(fixture.presenter.editor_outcome(id), EditorOutcome::Pending);
        let (generation, _) = fixture.effect(id);
        fixture.handle(Event::OperationScanned { id, generation, plan: Ok(vec![PlannedItem { source: path(&["from", "a"]), destination: Some(path(&["from", "b"])), kind: EntryKind::File }].into()) });
        fixture.step(id, StepResult::NameCollision { item: path(&["from", "a"]), progress: progress(0, 0) });
        assert_eq!(fixture.presenter.editor_outcome(id), EditorOutcome::Rejected("An item with this name already exists."));
        fixture.advance(Duration::from_secs(5));
        assert!(fixture.presenter.view().panels().is_empty(), "a refused name shows in the editor, not a panel");
        fixture.handle(Command::CancelOperation { id });
        assert_eq!(fixture.presenter.take_dismissals(), vec![id]);
        let retry = fixture.start(&["a"], OperationKind::Rename { to: name("c") });
        let (generation, _) = fixture.effect(retry);
        fixture.handle(Event::OperationScanned { id: retry, generation, plan: Ok(vec![PlannedItem { source: path(&["from", "a"]), destination: Some(path(&["from", "c"])), kind: EntryKind::File }].into()) });
        fixture.step(retry, StepResult::Finished { progress: progress(1, 0) });
        assert_eq!(fixture.presenter.editor_outcome(retry), EditorOutcome::Succeeded);
        assert_eq!(fixture.presenter.editor_outcome(OperationId::new(99)), EditorOutcome::Closed);
    }

    #[test]
    fn the_quit_prompt_names_the_running_operations() {
        let mut fixture = Fixture::new(&["a"]);
        fixture.start(&["a"], OperationKind::Copy);
        fixture.handle(Command::Quit { confirmed: false });
        assert_eq!(fixture.presenter.view().quit_prompt(), Some(1));
        let revision = fixture.presenter.view().quit_prompt_revision();
        fixture.handle(Command::Quit { confirmed: true });
        assert_eq!((fixture.presenter.view().quit_prompt(), fixture.presenter.view().quit_accepted()), (None, true));
        assert_eq!(fixture.presenter.view().quit_prompt_revision(), revision);
    }
}
