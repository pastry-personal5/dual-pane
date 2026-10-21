//! Carries out one operation request on a worker: an execution step over the
//! plan, a finalization step over source folders, or cancellation cleanup.
//! Every write is decided by its write primitive, never by an earlier check:
//! `mkdir`, exclusive rename, and `EEXIST`. A step stops at the first item
//! that needs the person and reports it only when it is the step's first
//! item; otherwise it reports the progress before it, so the next step
//! rediscovers the item. It never chooses a resolution itself.

use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dual_pane_application::{ByteProgress, CleanupResult, FinalizeResult, NameProblem, OperationErrorKind, OperationFailure, OperationProgress, PlannedItem, StepResult};
use dual_pane_domain::{EntryKind, Location, OperationChoice, OperationId, OperationIntent, OperationIssue, OperationKind, starts_with};

use crate::native_calls;
use crate::native_location::{location_from_path, path_from_location};
use crate::operation_journal::{Journal, is_operation_temporary, temporary_name};
use crate::operation_scan::{Identity, NativeKind, ScannedPlan, error_kind, native_kind};

mod copying;
mod finalization;
mod moving;
mod naming;
mod removal;
mod stops;
#[cfg(test)]
mod tests;

pub use finalization::clean_up_after_loss;
use finalization::*;
use stops::*;

/// The file-system primitives a step uses. Tests replace single calls to
/// inject failures, panics, or a second volume.
pub trait FileSystem: Send + Sync {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }
    fn rename_exclusive(&self, from: &Path, to: &Path) -> io::Result<()> {
        native_calls::rename_exclusive(from, to)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    fn clone_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        native_calls::clone_file(from, to)
    }
    fn copy_file(&self, from: &Path, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<bool> {
        native_calls::copy_file(from, to, progress)
    }
    fn copy_metadata(&self, from: &Path, to: &Path) -> io::Result<()> {
        native_calls::copy_metadata(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }
    /// The volume of a folder, following links, to tell whether a move
    /// crosses volumes.
    fn device(&self, folder: &Path) -> io::Result<u64> {
        fs::metadata(folder).map(|metadata| metadata.dev())
    }
    /// Moves `path` to its volume's Trash and returns where it went.
    fn move_to_trash(&self, path: &Path) -> io::Result<PathBuf>;
}

/// What one request produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    Done(T),
    /// The operation was cancelled during the request; its cleanup reports.
    Stopped,
    /// The request could not be carried out validly, so the operation ends.
    Unavailable,
}

/// The shared, per-request resources of a step.
pub struct StepContext<'a> {
    pub fs: &'a dyn FileSystem,
    pub journal: &'a Journal,
    pub cancelled: &'a AtomicBool,
    pub budget: Duration,
    pub progress: &'a mut dyn FnMut(ByteProgress),
}

impl StepContext<'_> {
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// What handling one item led to.
enum Item {
    /// `completed` entries are done, and the step continues at `next`.
    Done {
        next: usize,
        completed: usize,
    },
    /// The item needs the person or failed; the step stops before it.
    Stop(Stop),
    Stopped,
}

enum Stop {
    Decision(OperationIssue, EntryKind),
    Error(OperationErrorKind),
    Name(NameProblem),
}

impl From<Stop> for Item {
    fn from(stop: Stop) -> Self {
        Self::Stop(stop)
    }
}

/// Everything the lane keeps for one operation between its requests.
pub struct OperationState {
    pub id: OperationId,
    pub intent: OperationIntent,
    scan: Option<ScannedPlan>,
    scan_index: HashMap<Location, usize>,
    /// Temporaries created and not yet moved into place or removed.
    temporaries: Vec<PathBuf>,
    journaled: HashSet<PathBuf>,
    next_temporary: u64,
    /// Folders this operation created, for their metadata at the end.
    created_folders: Vec<(PathBuf, PathBuf)>,
    /// A source copied to another volume whose removal is still pending.
    awaiting_removal: Option<Location>,
    /// Folders that received items moved to the Trash.
    pub trash_folders: Vec<Location>,
}

impl OperationState {
    pub fn new(id: OperationId, intent: OperationIntent) -> Self {
        Self { id, intent, scan: None, scan_index: HashMap::new(), temporaries: Vec::new(), journaled: HashSet::new(), next_temporary: 0, created_folders: Vec::new(), awaiting_removal: None, trash_folders: Vec::new() }
    }
    /// Remembers what a scan saw, for revalidating its plan.
    pub fn scanned(&mut self, scan: ScannedPlan) {
        self.scan_index = scan.plan.iter().enumerate().map(|(index, item)| (item.source.clone(), index)).collect();
        self.scan = Some(scan);
        self.awaiting_removal = None;
    }
    /// The scan's identity for `plan[index]`, or `None` for a placeholder.
    /// Fails when `plan` is not the plan this state scanned.
    fn identities(&self, plan: &[PlannedItem]) -> Option<&[Option<Identity>]> {
        let scan = self.scan.as_ref()?;
        (scan.plan.len() == plan.len() && (std::ptr::eq(scan.plan.as_ptr(), plan.as_ptr()) || scan.plan.iter().zip(plan).all(|(scanned, given)| scanned == given))).then_some(&scan.identities)
    }

    /// Runs the plan from `at`, starting from the accepted `progress`.
    pub fn execute(&mut self, context: &mut StepContext<'_>, plan: &[PlannedItem], at: usize, choice: Option<OperationChoice>, mut progress: OperationProgress) -> Outcome<StepResult> {
        let Some(identities) = self.identities(plan).map(Arc::<[Option<Identity>]>::from) else { return Outcome::Unavailable };
        if at >= plan.len() {
            return Outcome::Unavailable;
        }
        let new_folder = matches!(self.intent.kind(), OperationKind::NewFolder { .. });
        let started = Instant::now();
        let mut index = at;
        while index < plan.len() {
            // A cancel stops at this safe boundary; entries already done are
            // reported so the outcome counts them.
            if context.cancelled() {
                return if index > at { Outcome::Done(StepResult::Advanced { next: index, progress }) } else { Outcome::Stopped };
            }
            if index > at && started.elapsed() >= context.budget {
                return Outcome::Done(StepResult::Advanced { next: index, progress });
            }
            // A placeholder for content the scan skipped is counted, never read.
            if identities[index].is_none() && !new_folder {
                progress.skipped += 1;
                index += 1;
                continue;
            }
            if index == at && choice == Some(OperationChoice::Skip) {
                let end = subtree_end(plan, at);
                progress.skipped += end - at;
                self.awaiting_removal = None;
                index = end;
                continue;
            }
            let item_choice = if index == at { choice } else { None };
            match self.item(context, plan, &identities, index, item_choice) {
                Item::Done { next, completed } => {
                    progress.completed += completed;
                    index = next;
                }
                Item::Stop(_) | Item::Stopped if index > at => return Outcome::Done(StepResult::Advanced { next: index, progress }),
                Item::Stopped => return Outcome::Stopped,
                Item::Stop(stop) => {
                    let item = plan[at].source.clone();
                    return Outcome::Done(match stop {
                        Stop::Decision(issue, destination_kind) => StepResult::DecisionRequired { item, issue, destination_kind, progress },
                        Stop::Error(kind) => StepResult::RecoverableError { failure: OperationFailure { item, kind }, progress },
                        Stop::Name(NameProblem::Collision) => StepResult::NameCollision { item, progress },
                        Stop::Name(problem) => StepResult::NameRejected { item, problem, progress },
                    });
                }
            }
        }
        self.finish_folders(context.fs);
        Outcome::Done(StepResult::Finished { progress })
    }

    fn item(&mut self, context: &mut StepContext<'_>, plan: &[PlannedItem], identities: &[Option<Identity>], index: usize, choice: Option<OperationChoice>) -> Item {
        let entry = &plan[index];
        let identity = identities[index];
        let single = |handled: Result<(), Item>| handled.map_or_else(|item| item, |()| Item::Done { next: index + 1, completed: 1 });
        match self.intent.kind().clone() {
            OperationKind::NewFolder { .. } => single(self.new_folder(context, entry)),
            OperationKind::Rename { .. } => single(self.rename(context, entry, identity)),
            OperationKind::MoveToTrash => single(self.trash(context, entry, identity)),
            OperationKind::DeletePermanently => single(self.delete(context, entry, identity)),
            OperationKind::Copy => self.copy(context, plan, index, identity, choice),
            OperationKind::Move => self.move_item(context, plan, index, identity, choice),
        }
    }
}

/// The real file system, with Trash through Qt.
pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn move_to_trash(&self, path: &Path) -> io::Result<PathBuf> {
        crate::native_shell::move_to_trash(path)
    }
}
