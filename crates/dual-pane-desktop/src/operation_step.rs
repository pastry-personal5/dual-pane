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

    fn new_folder(&mut self, context: &StepContext<'_>, entry: &PlannedItem) -> Result<(), Item> {
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        context.fs.create_dir(&path_from_location(&entry.source)).map_err(|error| name_error(self.id, "create folder", &error).into())
    }

    /// Renames with exclusive rename. A collision that turns out to be the
    /// same file under a case or normalization variant of its name is
    /// renamed in place; identity decides it, never a name comparison.
    fn rename(&mut self, context: &StepContext<'_>, entry: &PlannedItem, identity: Option<Identity>) -> Result<(), Item> {
        let (Some(identity), Some(destination)) = (identity, entry.destination.as_ref()) else { return Err(Stop::Error(OperationErrorKind::Other).into()) };
        let source = path_from_location(&entry.source);
        revalidate(&source, identity)?;
        let target = path_from_location(destination);
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        match context.fs.rename_exclusive(&source, &target) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => {
                if same_entry_variant(&source, &target, identity) {
                    context.fs.rename(&source, &target).map_err(|error| name_error(self.id, "rename", &error).into())
                } else {
                    Err(Stop::Name(NameProblem::Collision).into())
                }
            }
            Err(error) => Err(name_error(self.id, "rename", &error).into()),
        }
    }

    fn trash(&mut self, context: &StepContext<'_>, entry: &PlannedItem, identity: Option<Identity>) -> Result<(), Item> {
        let Some(identity) = identity else { return Err(Stop::Error(OperationErrorKind::Other).into()) };
        let source = path_from_location(&entry.source);
        revalidate(&source, identity)?;
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        // Qt names files by text, so a name that is not valid UTF-8 cannot be
        // trashed safely. It is never deleted instead.
        if std::str::from_utf8(source.as_os_str().as_bytes()).is_err() {
            return Err(Stop::Error(OperationErrorKind::TrashUnavailable).into());
        }
        match context.fs.move_to_trash(&source) {
            Ok(trashed) => {
                if let Some(folder) = trashed.parent().and_then(location_from_path)
                    && !self.trash_folders.contains(&folder)
                {
                    self.trash_folders.push(folder);
                }
                Ok(())
            }
            Err(error) => {
                log(self.id, "move to Trash", &error);
                Err(Stop::Error(OperationErrorKind::TrashUnavailable).into())
            }
        }
    }

    /// Unlinks files and links; folders wait for finalization.
    fn delete(&mut self, context: &StepContext<'_>, entry: &PlannedItem, identity: Option<Identity>) -> Result<(), Item> {
        let Some(identity) = identity else { return Err(Stop::Error(OperationErrorKind::Other).into()) };
        let source = path_from_location(&entry.source);
        revalidate(&source, identity)?;
        if entry.kind == EntryKind::Directory {
            return Ok(());
        }
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        match context.fs.remove_file(&source) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error_stop(self.id, "delete", &error).into()),
            _ => Ok(()),
        }
    }

    fn copy(&mut self, context: &mut StepContext<'_>, plan: &[PlannedItem], index: usize, identity: Option<Identity>, choice: Option<OperationChoice>) -> Item {
        let entry = &plan[index];
        let (Some(identity), Some(destination)) = (identity, entry.destination.as_ref()) else { return Stop::Error(OperationErrorKind::Other).into() };
        let source = path_from_location(&entry.source);
        let target = path_from_location(destination);
        if let Err(stop) = revalidate(&source, identity) {
            return stop.into();
        }
        match entry.kind {
            EntryKind::Directory => {
                if let Err(stop) = self.outside_source(entry, identity) {
                    return stop.into();
                }
                self.make_folder(context, &source, &target, index)
            }
            EntryKind::File | EntryKind::Symlink { .. } => self.copy_entry(context, entry.kind, &source, identity, &target, choice).map_or_else(Item::from, |()| Item::Done { next: index + 1, completed: 1 }),
            EntryKind::Other => Stop::Error(OperationErrorKind::UnsupportedItem).into(),
        }
    }

    fn move_item(&mut self, context: &mut StepContext<'_>, plan: &[PlannedItem], index: usize, identity: Option<Identity>, choice: Option<OperationChoice>) -> Item {
        let entry = &plan[index];
        let (Some(identity), Some(destination)) = (identity, entry.destination.as_ref()) else { return Stop::Error(OperationErrorKind::Other).into() };
        let source = path_from_location(&entry.source);
        let target = path_from_location(destination);
        if self.awaiting_removal.as_ref() == Some(&entry.source) {
            return self.remove_moved_source(context, &source, identity).map_or_else(Item::from, |()| Item::Done { next: index + 1, completed: 1 });
        }
        if let Err(stop) = revalidate(&source, identity) {
            return stop.into();
        }
        let Some(parent) = target.parent() else { return Stop::Error(OperationErrorKind::Other).into() };
        let same_volume = match context.fs.device(parent) {
            Ok(device) => device == identity.device,
            Err(error) => return error_stop(self.id, "read destination volume", &error).into(),
        };
        match entry.kind {
            EntryKind::Directory => {
                if let Err(stop) = self.outside_source(entry, identity) {
                    return stop.into();
                }
                let end = subtree_end(plan, index);
                // A whole subtree moves with one rename unless it holds content
                // the scan skipped, which must stay behind.
                let placeholders = self.identities(plan).is_some_and(|identities| identities[index + 1..end].iter().any(Option::is_none));
                if !same_volume || placeholders {
                    return self.make_folder(context, &source, &target, index);
                }
                if context.cancelled() {
                    return Item::Stopped;
                }
                match context.fs.rename_exclusive(&source, &target) {
                    Ok(()) => Item::Done { next: end, completed: end - index },
                    Err(error) if error.raw_os_error() == Some(libc::EEXIST) => self.existing_folder(&target, index),
                    Err(error) if error.raw_os_error() == Some(libc::EINVAL) => Stop::Error(OperationErrorKind::DestinationWithinSource).into(),
                    Err(error) => error_stop(self.id, "move folder", &error).into(),
                }
            }
            EntryKind::File | EntryKind::Symlink { .. } | EntryKind::Other if same_volume => self.move_on_volume(context, entry.kind, &source, &target, choice).map_or_else(Item::from, |()| Item::Done { next: index + 1, completed: 1 }),
            EntryKind::Other => Stop::Error(OperationErrorKind::UnsupportedItem).into(),
            EntryKind::File | EntryKind::Symlink { .. } => {
                if let Err(item) = self.copy_entry(context, entry.kind, &source, identity, &target, choice) {
                    return item;
                }
                // The copy is complete; only now may its source go.
                self.awaiting_removal = Some(entry.source.clone());
                self.remove_moved_source(context, &source, identity).map_or_else(Item::from, |()| Item::Done { next: index + 1, completed: 1 })
            }
        }
    }

    fn move_on_volume(&mut self, context: &StepContext<'_>, kind: EntryKind, source: &Path, target: &Path, choice: Option<OperationChoice>) -> Result<(), Item> {
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        if choice == Some(OperationChoice::Replace) {
            // Only a regular file replaces a regular file, in one rename.
            match fs::symlink_metadata(target) {
                Ok(metadata) if kind == EntryKind::File && metadata.is_file() => return context.fs.rename(source, target).map_err(|error| error_stop(self.id, "replace", &error).into()),
                Ok(metadata) => return Err(conflict(kind, &metadata, target).into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error_stop(self.id, "read destination", &error).into()),
            }
        }
        match context.fs.rename_exclusive(source, target) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => Err(existing_destination(kind, target).into()),
            Err(error) => Err(error_stop(self.id, "move", &error).into()),
        }
    }

    /// Removes the source of a completed cross-volume copy, after checking
    /// it is still the scanned item. A failure keeps both copies.
    fn remove_moved_source(&mut self, context: &StepContext<'_>, source: &Path, identity: Identity) -> Result<(), Stop> {
        match revalidate(source, identity) {
            Ok(_) => {}
            Err(Stop::Error(OperationErrorKind::NotFound)) => {
                self.awaiting_removal = None;
                return Ok(());
            }
            Err(stop) => return Err(stop),
        }
        match context.fs.remove_file(source) {
            Ok(()) => {
                self.awaiting_removal = None;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.awaiting_removal = None;
                Ok(())
            }
            Err(error) => {
                log(self.id, "remove moved source", &error);
                Err(Stop::Error(OperationErrorKind::SourceRemovalFailed))
            }
        }
    }

    /// Creates `target` for a folder, or merges into an existing folder.
    fn make_folder(&mut self, context: &StepContext<'_>, source: &Path, target: &Path, index: usize) -> Item {
        if context.cancelled() {
            return Item::Stopped;
        }
        match context.fs.create_dir(target) {
            Ok(()) => {
                self.created_folders.push((source.to_path_buf(), target.to_path_buf()));
                Item::Done { next: index + 1, completed: 1 }
            }
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => self.existing_folder(target, index),
            Err(error) => error_stop(self.id, "create folder", &error).into(),
        }
    }

    /// An existing folder merges; anything else conflicts with a folder.
    fn existing_folder(&self, target: &Path, index: usize) -> Item {
        match fs::symlink_metadata(target) {
            Ok(metadata) if metadata.is_dir() => Item::Done { next: index + 1, completed: 1 },
            Ok(metadata) => conflict(EntryKind::Directory, &metadata, target).into(),
            Err(error) => error_stop(self.id, "read destination", &error).into(),
        }
    }

    /// Copies a file or link to `target` through a temporary in its folder.
    /// A new item appears only by exclusive rename; a replacement only after
    /// its data is complete, by one rename over the unchanged destination.
    fn copy_entry(&mut self, context: &mut StepContext<'_>, kind: EntryKind, source: &Path, identity: Identity, target: &Path, choice: Option<OperationChoice>) -> Result<(), Item> {
        let replace = match fs::symlink_metadata(target) {
            Ok(metadata) if kind == EntryKind::File && metadata.is_file() && choice == Some(OperationChoice::Replace) => Some(Identity::of(&metadata)),
            Ok(metadata) => return Err(conflict(kind, &metadata, target).into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error_stop(self.id, "read destination", &error).into()),
        };
        let Some(folder) = target.parent() else { return Err(Stop::Error(OperationErrorKind::Other).into()) };
        let temporary = self.temporary(context, folder)?;
        if let Err(item) = self.fill(context, source, identity, folder, &temporary) {
            self.discard(context.fs, &temporary);
            return Err(item);
        }
        if context.cancelled() {
            self.discard(context.fs, &temporary);
            return Err(Item::Stopped);
        }
        let placed = match replace {
            Some(existing) => match fs::symlink_metadata(target) {
                Ok(metadata) if existing.matches(&Identity::of(&metadata)) => context.fs.rename(&temporary, target).map_err(|error| error_stop(self.id, "replace", &error).into()),
                Ok(_) => Err(Stop::Error(OperationErrorKind::ChangedSinceScan).into()),
                Err(error) => Err(error_stop(self.id, "read destination", &error).into()),
            },
            None => match context.fs.rename_exclusive(&temporary, target) {
                Ok(()) => Ok(()),
                Err(error) if error.raw_os_error() == Some(libc::EEXIST) => Err(existing_destination(kind, target).into()),
                Err(error) => Err(error_stop(self.id, "place copy", &error).into()),
            },
        };
        match placed {
            Ok(()) => {
                self.temporaries.retain(|path| path != &temporary);
                Ok(())
            }
            Err(item) => {
                self.discard(context.fs, &temporary);
                Err(item)
            }
        }
    }

    /// Writes the source's data and metadata to `temporary`, cloning on the
    /// same volume.
    fn fill(&mut self, context: &mut StepContext<'_>, source: &Path, identity: Identity, folder: &Path, temporary: &Path) -> Result<(), Item> {
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        if identity.kind == NativeKind::File && context.fs.device(folder).is_ok_and(|device| device == identity.device) {
            match context.fs.clone_file(source, temporary) {
                Ok(()) => return Ok(()),
                Err(error) if matches!(error.raw_os_error(), Some(libc::ENOTSUP | libc::EXDEV)) => {}
                Err(error) => return Err(error_stop(self.id, "clone", &error).into()),
            }
        }
        let total = identity.size;
        let cancelled = context.cancelled;
        let publish = &mut *context.progress;
        let mut progress = |done: u64| {
            publish(ByteProgress { done, total });
            !cancelled.load(Ordering::Relaxed)
        };
        match context.fs.copy_file(source, temporary, &mut progress) {
            Ok(true) => Ok(()),
            Ok(false) => Err(Item::Stopped),
            Err(_) if context.cancelled() => Err(Item::Stopped),
            Err(error) => Err(error_stop(self.id, "copy", &error).into()),
        }
    }

    /// A new temporary path in `folder`, recorded in the journal before the
    /// operation's first temporary there.
    fn temporary(&mut self, context: &StepContext<'_>, folder: &Path) -> Result<PathBuf, Item> {
        if !self.journaled.contains(folder) {
            let device = context.fs.device(folder).map_err(|error| Item::from(error_stop(self.id, "read destination volume", &error)))?;
            if context.journal.record(self.id.get(), folder, device).is_err() {
                return Err(Stop::Error(OperationErrorKind::JournalUnavailable).into());
            }
            self.journaled.insert(folder.to_path_buf());
        }
        let path = folder.join(temporary_name(context.journal.launch(), self.id.get(), self.next_temporary));
        self.next_temporary += 1;
        self.temporaries.push(path.clone());
        Ok(path)
    }

    fn discard(&mut self, fs: &dyn FileSystem, temporary: &Path) {
        match fs.remove_file(temporary) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => log(self.id, "remove temporary", &error),
            _ => self.temporaries.retain(|path| path != temporary),
        }
    }

    /// Rejects a folder root whose destination folder lies inside it through
    /// a link or another spelling of its path, by device and inode.
    fn outside_source(&self, entry: &PlannedItem, identity: Identity) -> Result<(), Stop> {
        if entry.source.parent().as_ref() != Some(self.intent.source()) {
            return Ok(());
        }
        let Some(destination) = self.intent.destination() else { return Ok(()) };
        let canonical = fs::canonicalize(path_from_location(destination)).map_err(|error| error_stop(self.id, "resolve destination", &error))?;
        for ancestor in canonical.ancestors() {
            let metadata = fs::metadata(ancestor).map_err(|error| error_stop(self.id, "read destination ancestry", &error))?;
            if metadata.dev() == identity.device && metadata.ino() == identity.inode {
                return Err(Stop::Error(OperationErrorKind::DestinationWithinSource));
            }
        }
        Ok(())
    }

    /// Gives the folders this operation created their source's metadata,
    /// deepest first, once their contents are in place.
    fn finish_folders(&mut self, fs: &dyn FileSystem) {
        for (source, target) in self.created_folders.drain(..).rev() {
            if let Err(error) = fs.copy_metadata(&source, &target) {
                log(self.id, "copy folder metadata", &error);
            }
        }
    }

    /// Removes source folders from `at` on, deepest first. A folder already
    /// gone counts as removed.
    pub fn finalize(&mut self, context: &StepContext<'_>, directories: &[Location], at: usize) -> Outcome<FinalizeResult> {
        if at >= directories.len() || self.scan.is_none() {
            return Outcome::Unavailable;
        }
        let started = Instant::now();
        for (index, directory) in directories.iter().enumerate().skip(at) {
            if context.cancelled() {
                return Outcome::Stopped;
            }
            if index > at && started.elapsed() >= context.budget {
                return Outcome::Done(FinalizeResult::Advanced { next: index });
            }
            let identity = self.scan_index.get(directory).and_then(|scanned| self.scan.as_ref().and_then(|scan| scan.identities[*scanned]));
            let path = path_from_location(directory);
            let stop = match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => Some(error_stop(self.id, "read folder", &error)),
                Ok(metadata) if identity.is_some_and(|identity| !identity.matches(&Identity::of(&metadata))) => Some(Stop::Error(OperationErrorKind::ChangedSinceScan)),
                Ok(_) => match context.fs.remove_dir(&path) {
                    Ok(()) => None,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) if matches!(error.raw_os_error(), Some(libc::ENOTEMPTY | libc::EEXIST)) => Some(Stop::Error(OperationErrorKind::FolderNotEmpty)),
                    Err(error) => Some(error_stop(self.id, "remove folder", &error)),
                },
            };
            match stop {
                None => {}
                Some(_) if index > at => return Outcome::Done(FinalizeResult::Advanced { next: index }),
                Some(Stop::Error(kind)) => return Outcome::Done(FinalizeResult::RecoverableError { failure: OperationFailure { item: directory.clone(), kind } }),
                Some(_) => return Outcome::Unavailable,
            }
        }
        Outcome::Done(FinalizeResult::Finished)
    }

    /// Removes this operation's temporaries after its in-flight request has
    /// stopped. Completed work stays. Anything that cannot be removed makes
    /// the cleanup uncertain and stays recorded for the next launch.
    pub fn clean_up(&mut self, fs: &dyn FileSystem, journal: &Journal) -> CleanupResult {
        let mut uncertain = false;
        for temporary in std::mem::take(&mut self.temporaries) {
            match fs.remove_file(&temporary) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    log(self.id, "remove temporary", &error);
                    uncertain = true;
                }
            }
        }
        let folders = self.journaled.iter().cloned().chain(journal.folders(self.id.get())).collect::<HashSet<_>>();
        uncertain |= !remove_operation_temporaries(self.id, fs, journal, &folders);
        if !uncertain && !folders.is_empty() && !journal.clear(self.id.get()) {
            uncertain = true;
        }
        if uncertain { CleanupResult::Uncertain } else { CleanupResult::Clean }
    }

    /// Forgets the journal records once a request ended with no temporary
    /// left, as every finished or failed step does.
    pub fn release_journal(&mut self, journal: &Journal) {
        if self.temporaries.is_empty() && !self.journaled.is_empty() && journal.clear(self.id.get()) {
            self.journaled.clear();
        }
    }
}

/// Removes, without state, every temporary of `id` in its recorded folders,
/// as after a worker panic. Records stay unless every folder is clean.
pub fn clean_up_after_loss(id: OperationId, fs: &dyn FileSystem, journal: &Journal) -> CleanupResult {
    let folders = journal.folders(id.get()).into_iter().collect::<HashSet<_>>();
    if remove_operation_temporaries(id, fs, journal, &folders) && (folders.is_empty() || journal.clear(id.get())) { CleanupResult::Clean } else { CleanupResult::Uncertain }
}

/// Removes the regular files and links in `folders` whose names match this
/// launch's exact pattern for `id`. Returns whether every one is gone.
fn remove_operation_temporaries(id: OperationId, fs: &dyn FileSystem, journal: &Journal, folders: &HashSet<PathBuf>) -> bool {
    let launch = journal.launch().hex();
    let mut clean = true;
    for folder in folders {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                clean = false;
                continue;
            }
        };
        for entry in entries.flatten() {
            if !is_operation_temporary(entry.file_name().as_bytes(), &launch, id.get()) {
                continue;
            }
            let path = entry.path();
            if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_dir()) && fs.remove_file(&path).is_err_and(|error| error.kind() != io::ErrorKind::NotFound) {
                clean = false;
            }
        }
    }
    clean
}

/// The index just past `plan[at]` and every entry below it.
fn subtree_end(plan: &[PlannedItem], at: usize) -> usize {
    plan[at + 1..].iter().position(|item| !starts_with(&item.source, &plan[at].source)).map_or(plan.len(), |offset| at + 1 + offset)
}

/// Checks that `path` is still the item the scan saw.
fn revalidate(path: &Path, identity: Identity) -> Result<Metadata, Stop> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if identity.matches(&Identity::of(&metadata)) => Ok(metadata),
        Ok(_) => Err(Stop::Error(OperationErrorKind::ChangedSinceScan)),
        Err(error) => Err(Stop::Error(error_kind(&error))),
    }
}

/// The conflict an existing `target` raises for an arriving `kind`.
fn conflict(kind: EntryKind, existing: &Metadata, target: &Path) -> Stop {
    let existing_kind = match native_kind(existing.file_type()) {
        NativeKind::Directory => EntryKind::Directory,
        NativeKind::File => EntryKind::File,
        NativeKind::Symlink => EntryKind::Symlink { points_to_directory: fs::metadata(target).is_ok_and(|metadata| metadata.is_dir()) },
        NativeKind::Other => return Stop::Error(OperationErrorKind::UnsupportedItem),
    };
    match (kind, existing_kind) {
        (EntryKind::Symlink { .. }, _) | (_, EntryKind::Symlink { .. }) => Stop::Decision(OperationIssue::LinkCollision, existing_kind),
        (EntryKind::File, EntryKind::File) => Stop::Decision(OperationIssue::FileConflict, existing_kind),
        (EntryKind::File, EntryKind::Directory) | (EntryKind::Directory, EntryKind::File) => Stop::Decision(OperationIssue::KindMismatch, existing_kind),
        _ => Stop::Error(OperationErrorKind::UnsupportedItem),
    }
}

/// The conflict after a write primitive reported `EEXIST` for `target`.
fn existing_destination(kind: EntryKind, target: &Path) -> Stop {
    match fs::symlink_metadata(target) {
        Ok(metadata) => conflict(kind, &metadata, target),
        // It vanished again; trying again decides.
        Err(error) => Stop::Error(error_kind(&error)),
    }
}

/// Whether an `EEXIST` for renaming `source` to `target` came from the same
/// entry under a case or normalization variant: `target` resolves to the
/// source's file, no entry has `target`'s exact name, and no other entry in
/// the folder is a hard link to the same file.
fn same_entry_variant(source: &Path, target: &Path, identity: Identity) -> bool {
    let (Some(folder), Some(source_name), Some(target_name)) = (source.parent(), source.file_name(), target.file_name()) else { return false };
    if !fs::symlink_metadata(target).is_ok_and(|metadata| metadata.dev() == identity.device && metadata.ino() == identity.inode) {
        return false;
    }
    let Ok(entries) = fs::read_dir(folder) else { return false };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        if name.as_bytes() == target_name.as_bytes() {
            return false;
        }
        if name.as_bytes() != source_name.as_bytes() && entry.metadata().is_ok_and(|metadata| metadata.ino() == identity.inode) {
            return false;
        }
    }
    true
}

/// A Rename or New Folder failure: name problems keep the editor open.
fn name_error(id: OperationId, action: &str, error: &io::Error) -> Stop {
    match error.raw_os_error() {
        Some(libc::EEXIST) => Stop::Name(NameProblem::Collision),
        Some(libc::ENAMETOOLONG) => Stop::Name(NameProblem::TooLong),
        Some(libc::EILSEQ | libc::EINVAL) => Stop::Name(NameProblem::RejectedCharacters),
        _ => error_stop(id, action, error),
    }
}

fn error_stop(id: OperationId, action: &str, error: &io::Error) -> Stop {
    log(id, action, error);
    Stop::Error(error_kind(error))
}

/// Keeps the raw native error in the log only.
fn log(id: OperationId, action: &str, error: &io::Error) {
    eprintln!("Dual Pane operation {}: {action} failed: {error}", id.get());
}

/// The real file system, with Trash through Qt.
pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn move_to_trash(&self, path: &Path) -> io::Result<PathBuf> {
        crate::native_shell::move_to_trash(path)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::Mutex;

    use dual_pane_application::{Command, Event, OperationEffect, OperationOutcome, OperationStatus, WorkRequest, Workspace};
    use dual_pane_domain::{BrowserSide, EntryName};

    use super::*;
    use crate::operation_journal::{JOURNAL_FILE_NAME, LaunchId};
    use crate::operation_lane::{SerialLane, Services};

    type CopyHook = Box<dyn Fn(&Path) + Send>;

    /// The real file system with injectable faults and a pretend second
    /// volume.
    #[derive(Default)]
    struct TestFs {
        /// Folders at or below this path report another volume.
        other_volume: Mutex<Option<PathBuf>>,
        /// Copies into these folders write a little, then fail with this error.
        failing_copies: Mutex<Vec<(PathBuf, i32)>>,
        /// Removing these files or folders fails with `EACCES` while listed.
        failing_removals: Mutex<Vec<PathBuf>>,
        /// A copy runs this first, for example to cancel.
        before_copy: Mutex<Option<CopyHook>>,
        exclusive_renames: Mutex<usize>,
        /// An exclusive rename first creates its destination, as another
        /// process racing the copy would.
        racing_destination: Mutex<Option<PathBuf>>,
        trash: Mutex<Option<PathBuf>>,
    }

    impl TestFs {
        /// Runs the copy hook, then writes a partial copy and fails when
        /// `to` is in a failing folder.
        fn before_writing(&self, to: &Path) -> io::Result<()> {
            if let Some(hook) = self.before_copy.lock().unwrap().as_ref() {
                hook(to);
            }
            if let Some((_, errno)) = self.failing_copies.lock().unwrap().iter().find(|(folder, _)| to.starts_with(folder)) {
                fs::write(to, b"part")?;
                return Err(io::Error::from_raw_os_error(*errno));
            }
            Ok(())
        }
    }

    impl FileSystem for TestFs {
        fn rename_exclusive(&self, from: &Path, to: &Path) -> io::Result<()> {
            *self.exclusive_renames.lock().unwrap() += 1;
            if let Some(racing) = self.racing_destination.lock().unwrap().take_if(|racing| racing == to) {
                fs::write(&racing, b"racer")?;
            }
            native_calls::rename_exclusive(from, to)
        }
        fn clone_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.before_writing(to)?;
            native_calls::clone_file(from, to)
        }
        fn copy_file(&self, from: &Path, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<bool> {
            self.before_writing(to)?;
            native_calls::copy_file(from, to, progress)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            if self.failing_removals.lock().unwrap().iter().any(|failing| failing == path) {
                return Err(io::Error::from_raw_os_error(libc::EACCES));
            }
            fs::remove_file(path)
        }
        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            if self.failing_removals.lock().unwrap().iter().any(|failing| failing == path) {
                return Err(io::Error::from_raw_os_error(libc::EACCES));
            }
            fs::remove_dir(path)
        }
        fn device(&self, folder: &Path) -> io::Result<u64> {
            let device = fs::metadata(folder)?.dev();
            Ok(if self.other_volume.lock().unwrap().as_ref().is_some_and(|volume| folder.starts_with(volume)) { device.wrapping_add(1) } else { device })
        }
        fn move_to_trash(&self, path: &Path) -> io::Result<PathBuf> {
            let Some(trash) = self.trash.lock().unwrap().clone() else { return Err(io::Error::other("no Trash")) };
            let target = trash.join(path.file_name().ok_or_else(|| io::Error::other("no name"))?);
            fs::rename(path, &target)?;
            Ok(target)
        }
    }

    fn name(text: &str) -> EntryName {
        EntryName::new(text).unwrap()
    }

    /// A temporary root with `from` and `to`, a workspace showing them, and a
    /// serial lane over a test file system and a journal in the root.
    struct Run {
        root: tempfile::TempDir,
        ws: Workspace,
        lane: SerialLane,
        fs: Arc<TestFs>,
        events: Vec<Event>,
    }

    impl Run {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("from")).unwrap();
            fs::create_dir(root.path().join("to")).unwrap();
            let fs = Arc::new(TestFs::default());
            let journal = Journal::new(Some(root.path().join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
            assert!(journal.reopen());
            let lane = SerialLane::new(Services { fs: Arc::clone(&fs) as Arc<dyn FileSystem>, journal, budget: Duration::from_secs(60) });
            Self { root, ws: Workspace::new(), lane, fs, events: Vec::new() }
        }
        fn path(&self, relative: &str) -> PathBuf {
            self.root.path().join(relative)
        }
        fn location(&self, relative: &str) -> Location {
            location_from_path(&self.path(relative)).unwrap()
        }
        /// Shows `from` on the left and `to` on the right, freshly listed.
        fn show(&mut self) {
            for (browser, folder) in [(BrowserSide::Left, "from"), (BrowserSide::Right, "to")] {
                let location = self.location(folder);
                let work = self.ws.handle(Command::Navigate { browser, location }.into()).work;
                self.drive(work);
            }
        }
        /// Selects `names` on the left and starts `kind`.
        fn start(&mut self, names: &[&str], kind: OperationKind) -> OperationId {
            self.show();
            let tab = self.ws.active_tab(BrowserSide::Left);
            let entries = self.ws.entries(BrowserSide::Left).to_vec();
            for (index, text) in names.iter().enumerate() {
                let row = entries.iter().position(|entry| entry.name() == &name(text)).unwrap_or_else(|| panic!("{text} is listed"));
                let command = if index == 0 { Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: name(text) } } else { Command::ToggleEntry { browser: BrowserSide::Left, tab, row, name: name(text) } };
                self.ws.handle(command.into());
            }
            let started = self.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind }.into());
            let id = self.ws.operation_jobs().last().unwrap().id();
            assert!(matches!(started.outputs.first(), Some(dual_pane_application::Output::OperationChanged { .. })), "{:?}", started.outputs);
            self.drive(started.work);
            id
        }
        /// Carries out work until none is left: listings through the real
        /// reader, operation effects through the lane.
        fn drive(&mut self, work: Vec<WorkRequest>) {
            let mut work = VecDeque::from(work);
            while let Some(request) = work.pop_front() {
                let events = match request {
                    WorkRequest::Operation(effect) => self.lane.run(effect),
                    WorkRequest::ReadDirectory { browser, tab, token, location, sort, .. } => {
                        let entries = crate::folder_items::read(&location, sort, &AtomicBool::new(false)).unwrap();
                        vec![match entries {
                            Ok(entries) => Event::FolderItemsLoaded { browser, tab, token, entries, changes: None },
                            Err(kind) => Event::FolderItemsFailed { browser, tab, token, kind },
                        }]
                    }
                    _ => vec![],
                };
                for event in events {
                    self.events.push(event.clone());
                    work.extend(self.ws.handle(event.into()).work);
                }
            }
        }
        fn status(&self, id: OperationId) -> OperationStatus {
            self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().status().clone()
        }
        fn decide(&mut self, id: OperationId, choice: OperationChoice, apply_to_all: bool) {
            let job = self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap();
            let decision = job.decision().unwrap_or_else(|| panic!("waiting: {:?}", job.status()));
            let (token, item) = (decision.token, decision.item.clone());
            let work = self.ws.handle(Command::DecideOperation { id, token, item, choice, apply_to_all }.into()).work;
            assert!(!work.is_empty(), "{choice:?} was accepted");
            self.drive(work);
        }
        fn waiting(&self, id: OperationId) -> (OperationIssue, Option<OperationErrorKind>) {
            let job = self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap();
            let decision = job.decision().unwrap_or_else(|| panic!("waiting: {:?}", job.status()));
            (decision.issue, decision.failure.map(|failure| failure.kind))
        }
        fn write(&self, relative: &str, content: &str) {
            let path = self.path(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        fn read(&self, relative: &str) -> String {
            fs::read_to_string(self.path(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
        }
        fn temporaries(&self) -> Vec<String> {
            walk(self.root.path()).into_iter().filter(|name| name.contains(".dual-pane-")).collect()
        }
    }

    fn walk(folder: &Path) -> Vec<String> {
        let mut names = Vec::new();
        for entry in fs::read_dir(folder).unwrap().flatten() {
            names.push(entry.file_name().to_string_lossy().into_owned());
            if entry.file_type().unwrap().is_dir() {
                names.extend(walk(&entry.path()));
            }
        }
        names
    }

    #[test]
    fn a_copy_merges_folders_keeps_metadata_and_never_clobbers() {
        let mut run = Run::new();
        run.write("from/dir/keep.txt", "new");
        run.write("from/dir/sub/fresh.txt", "fresh");
        run.write("to/dir/keep.txt", "old");
        fs::set_permissions(run.path("from/dir/sub"), fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(run.path("from/dir/sub/fresh.txt"), fs::Permissions::from_mode(0o600)).unwrap();
        symlink("sub", run.path("from/dir/link")).unwrap();
        let id = run.start(&["dir"], OperationKind::Copy);
        assert_eq!(run.waiting(id), (OperationIssue::FileConflict, None), "the existing folder merges; its file conflicts");
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!(run.read("to/dir/keep.txt"), "old", "a skipped conflict keeps the destination");
        assert_eq!(run.read("to/dir/sub/fresh.txt"), "fresh");
        assert_eq!(fs::metadata(run.path("to/dir/sub")).unwrap().permissions().mode() & 0o777, 0o750, "a created folder gets its source's metadata");
        assert_eq!(fs::metadata(run.path("to/dir/sub/fresh.txt")).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(fs::symlink_metadata(run.path("to/dir/link")).unwrap().file_type().is_symlink(), "a link is copied as a link");
        assert_eq!(run.read("from/dir/keep.txt"), "new", "the source is untouched");
        assert!(run.temporaries().is_empty());

        let id = run.start(&["dir"], OperationKind::Copy);
        assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
        run.decide(id, OperationChoice::Replace, true);
        assert_eq!(run.waiting(id).0, OperationIssue::LinkCollision, "a link is never replaced");
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!(run.read("to/dir/keep.txt"), "new", "Replace swaps in the complete copy");
        assert!(run.temporaries().is_empty(), "no temporary is left over");
    }

    #[test]
    fn a_kind_mismatch_replaces_neither_side() {
        let mut run = Run::new();
        run.write("from/clash/inner.txt", "inner");
        run.write("to/clash", "a file");
        run.write("from/file.txt", "file");
        fs::create_dir(run.path("to/file.txt")).unwrap();
        let id = run.start(&["clash", "file.txt"], OperationKind::Copy);
        assert_eq!(run.waiting(id).0, OperationIssue::KindMismatch);
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.waiting(id).0, OperationIssue::KindMismatch);
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!(run.read("to/clash"), "a file");
        assert!(run.path("to/file.txt").is_dir());
    }

    #[test]
    fn a_failed_replacement_leaves_the_destination_and_removes_only_its_temporary() {
        let mut run = Run::new();
        run.write("from/a.txt", "new content");
        run.write("to/a.txt", "old content");
        let foreign = run.root.path().join("to").join(crate::operation_journal::temporary_name(LaunchId::from_bytes([1; 16]), 1, 0));
        fs::write(&foreign, b"another launch").unwrap();
        let id = run.start(&["a.txt"], OperationKind::Copy);
        run.fs.failing_copies.lock().unwrap().push((run.path("to"), libc::ENOSPC));
        run.decide(id, OperationChoice::Replace, false);
        assert_eq!(run.waiting(id), (OperationIssue::RecoverableError, Some(OperationErrorKind::NoSpace)));
        assert_eq!(run.read("to/a.txt"), "old content", "the destination is untouched until the data is complete");
        assert_eq!(run.temporaries(), vec![foreign.file_name().unwrap().to_string_lossy().into_owned()], "only this operation's temporary is removed");
        run.fs.failing_copies.lock().unwrap().clear();
        run.decide(id, OperationChoice::TryAgain, false);
        assert_eq!(run.waiting(id).0, OperationIssue::FileConflict, "trying again rediscovers the conflict");
        run.decide(id, OperationChoice::Replace, false);
        assert_eq!(run.read("to/a.txt"), "new content");
    }

    #[test]
    fn the_journal_records_a_folder_before_its_first_temporary_and_forgets_it_after() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        let journal = run.lane.services.journal.clone();
        let folder = run.path("to");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&seen);
        *run.fs.before_copy.lock().unwrap() = Some(Box::new(move |_| observed.lock().unwrap().push(journal.folders(1))));
        let id = run.start(&["a.txt"], OperationKind::Copy);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert_eq!(seen.lock().unwrap().as_slice(), &[vec![folder]], "recorded before the copy wrote its temporary");
        assert!(run.lane.services.journal.folders(id.get()).is_empty(), "cleared once nothing is left");
    }

    #[test]
    fn a_same_volume_move_renames_a_whole_folder_once() {
        let mut run = Run::new();
        run.write("from/dir/a/x.txt", "x");
        run.write("from/dir/b.txt", "b");
        let id = run.start(&["dir"], OperationKind::Move);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert_eq!(*run.fs.exclusive_renames.lock().unwrap(), 1);
        assert_eq!(run.ws.operation_jobs()[0].progress().completed, 4);
        assert_eq!(run.read("to/dir/a/x.txt"), "x");
        assert!(!run.path("from/dir").exists(), "finalization finds the folder already gone");
    }

    #[test]
    fn a_merging_move_moves_entries_one_by_one_and_keeps_folders_with_skipped_content() {
        let mut run = Run::new();
        run.write("from/dir/a/x.txt", "new x");
        run.write("from/dir/b/y.txt", "y");
        run.write("to/dir/a/x.txt", "old x");
        let id = run.start(&["dir"], OperationKind::Move);
        assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!(run.read("to/dir/b/y.txt"), "y");
        assert!(!run.path("from/dir/b").exists(), "an emptied folder is removed");
        assert_eq!(run.read("from/dir/a/x.txt"), "new x", "the skipped file and the folders above it stay");
        assert_eq!(run.read("to/dir/a/x.txt"), "old x");
    }

    #[test]
    fn a_cross_volume_move_copies_first_and_removes_each_source_after() {
        let mut run = Run::new();
        run.write("from/dir/a.txt", "a");
        run.write("from/b.txt", "b");
        *run.fs.other_volume.lock().unwrap() = Some(run.path("to"));
        run.fs.failing_removals.lock().unwrap().push(run.path("from/b.txt"));
        let id = run.start(&["dir", "b.txt"], OperationKind::Move);
        assert_eq!(*run.fs.exclusive_renames.lock().unwrap(), 2, "each file is placed from a temporary, never renamed across volumes");
        assert_eq!(run.waiting(id), (OperationIssue::RecoverableError, Some(OperationErrorKind::SourceRemovalFailed)));
        assert_eq!((run.read("to/b.txt"), run.read("from/b.txt")), ("b".to_owned(), "b".to_owned()), "the copy is kept with its source");
        run.decide(id, OperationChoice::TryAgain, false);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::SourceRemovalFailed), "Try Again retries only the removal");
        run.fs.failing_removals.lock().unwrap().clear();
        run.decide(id, OperationChoice::TryAgain, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert!(!run.path("from/b.txt").exists() && !run.path("from/dir").exists());
        assert_eq!(run.read("to/dir/a.txt"), "a");
        assert!(run.temporaries().is_empty());

        run.write("from/c.txt", "c");
        run.fs.failing_removals.lock().unwrap().push(run.path("from/c.txt"));
        let id = run.start(&["c.txt"], OperationKind::Move);
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!((run.read("to/c.txt"), run.read("from/c.txt")), ("c".to_owned(), "c".to_owned()), "Skip keeps both copies");
    }

    #[test]
    fn a_cross_volume_move_without_a_journal_stops_at_the_item_until_it_reopens() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        *run.fs.other_volume.lock().unwrap() = Some(run.path("to"));
        let blocked = run.path("blocked");
        fs::write(&blocked, b"").unwrap();
        run.lane.services.journal = Journal::new(Some(blocked.join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
        let id = run.start(&["a.txt"], OperationKind::Move);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::JournalUnavailable));
        assert!(run.temporaries().is_empty() && run.path("from/a.txt").exists());
        run.lane.services.journal = Journal::new(Some(run.path(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
        run.decide(id, OperationChoice::TryAgain, false);
        assert!(run.events.contains(&Event::JournalStatus { available: true }), "Try Again reopens the journal");
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    }

    #[test]
    fn rename_refuses_collisions_and_hard_links_but_allows_a_case_only_change() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        run.write("from/b.txt", "b");
        let id = run.start(&["a.txt"], OperationKind::Rename { to: name("b.txt") });
        assert_eq!(run.status(id), OperationStatus::NameCollision { item: run.location("from/a.txt") });
        assert_eq!(run.read("from/b.txt"), "b");
        run.ws.handle(Command::CancelOperation { id }.into());

        fs::hard_link(run.path("from/a.txt"), run.path("from/link.txt")).unwrap();
        let id = run.start(&["a.txt"], OperationKind::Rename { to: name("link.txt") });
        assert_eq!(run.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().name_problem(), Some(NameProblem::Collision), "a hard link to the same file is still another item");
        run.ws.handle(Command::CancelOperation { id }.into());
        fs::remove_file(run.path("from/link.txt")).unwrap();

        let id = run.start(&["a.txt"], OperationKind::Rename { to: name("A.txt") });
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        let names = fs::read_dir(run.path("from")).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect::<HashSet<_>>();
        assert!(names.contains("A.txt") && !names.contains("a.txt"), "{names:?}");

        let long = "n".repeat(300);
        let id = run.start(&["A.txt"], OperationKind::Rename { to: name(&long) });
        assert_eq!(run.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().name_problem(), Some(NameProblem::TooLong));
    }

    #[test]
    fn new_folder_creates_or_reports_a_name_problem() {
        let mut run = Run::new();
        run.write("from/taken/inner", "x");
        let id = run.start(&[], OperationKind::NewFolder { name: name("made") });
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert!(run.path("from/made").is_dir());
        let id = run.start(&[], OperationKind::NewFolder { name: name("taken") });
        assert_eq!(run.status(id), OperationStatus::NameCollision { item: run.location("from/taken") });
        assert!(run.path("from/taken/inner").exists());
    }

    #[test]
    fn a_destination_inside_the_source_through_a_link_is_refused() {
        let mut run = Run::new();
        fs::create_dir_all(run.path("from/dir/inner")).unwrap();
        fs::remove_dir(run.path("to")).unwrap();
        symlink(run.path("from/dir/inner"), run.path("to")).unwrap();
        let id = run.start(&["dir"], OperationKind::Copy);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::DestinationWithinSource));
        assert!(fs::read_dir(run.path("from/dir/inner")).unwrap().next().is_none(), "nothing was copied into itself");
        run.decide(id, OperationChoice::Skip, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    }

    #[test]
    fn an_item_changed_since_the_scan_is_not_touched() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        run.show();
        let tab = run.ws.active_tab(BrowserSide::Left);
        run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("a.txt") }.into());
        let started = run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::DeletePermanently }.into());
        assert!(started.work.is_empty());
        let id = run.ws.operation_jobs()[0].id();
        let confirmed = run.ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work;
        let [WorkRequest::Operation(scan @ OperationEffect::Scan { .. })] = confirmed.as_slice() else { panic!() };
        let scanned = run.lane.run(scan.clone());
        let mut work = Vec::new();
        for event in scanned {
            work.extend(run.ws.handle(event.into()).work);
        }
        run.write("from/a.txt", "rewritten after the scan");
        run.drive(work);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::ChangedSinceScan));
        assert_eq!(run.read("from/a.txt"), "rewritten after the scan");
    }

    #[test]
    fn permanent_deletion_unlinks_files_then_removes_folders_deepest_first() {
        let mut run = Run::new();
        run.write("from/dir/a/x.txt", "x");
        run.write("from/dir/b.txt", "b");
        symlink(run.path("to"), run.path("from/dir/outside")).unwrap();
        run.write("to/kept.txt", "kept");
        run.fs.failing_removals.lock().unwrap().push(run.path("from/dir/a"));
        run.show();
        let tab = run.ws.active_tab(BrowserSide::Left);
        run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("dir") }.into());
        run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::DeletePermanently }.into());
        let id = run.ws.operation_jobs()[0].id();
        let work = run.ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work;
        run.drive(work);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::PermissionDenied), "a folder that cannot be removed asks");
        assert!(!run.path("from/dir/a/x.txt").exists() && fs::symlink_metadata(run.path("from/dir/outside")).is_err());
        assert_eq!(run.read("to/kept.txt"), "kept", "a link is removed, never followed");
        run.fs.failing_removals.lock().unwrap().clear();
        run.decide(id, OperationChoice::TryAgain, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert!(!run.path("from/dir").exists());
    }

    #[test]
    fn trash_moves_each_root_or_reports_trash_unavailable_without_deleting() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        let id = run.start(&["a.txt"], OperationKind::MoveToTrash);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::TrashUnavailable));
        assert_eq!(run.read("from/a.txt"), "a", "never deleted instead");
        let trash = run.path("Trash");
        fs::create_dir(&trash).unwrap();
        *run.fs.trash.lock().unwrap() = Some(trash.clone());
        run.decide(id, OperationChoice::TryAgain, false);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert!(trash.join("a.txt").exists());
        assert!(run.events.contains(&Event::LocationInvalidated { location: location_from_path(&trash).unwrap() }));
    }

    #[test]
    fn cancelling_mid_copy_keeps_completed_work_and_removes_only_its_temporaries() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        fs::write(run.path("from/large.bin"), vec![1_u8; 4 * 1024 * 1024]).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        run.lane.cancelled.insert(OperationId::new(1), Arc::clone(&cancelled));
        let flag = Arc::clone(&cancelled);
        *run.fs.before_copy.lock().unwrap() = Some(Box::new(move |to| {
            if !to.to_string_lossy().contains("-1-0.") {
                flag.store(true, Ordering::Relaxed);
            }
        }));
        let id = run.start(&["a.txt", "large.bin"], OperationKind::Copy);
        assert_eq!(id, OperationId::new(1));
        assert_eq!(run.status(id), OperationStatus::Running { at: 1 }, "the step reported the copy it completed, and the next one stopped at once");
        assert_eq!(run.read("to/a.txt"), "a");
        assert!(run.temporaries().is_empty(), "the interrupted copy discards its temporary at once");
        let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
        run.drive(work);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert!(run.temporaries().is_empty());
        assert!(!run.path("to/large.bin").exists());
        assert_eq!(run.read("to/a.txt"), "a", "completed work stays");
    }

    #[test]
    fn a_cancel_with_nothing_in_flight_still_reports_its_cleanup() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        run.write("to/a.txt", "old");
        let id = run.start(&["a.txt"], OperationKind::Copy);
        assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
        let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
        run.drive(work);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Cancelled));
        let mut lost = Run::new();
        assert_eq!(lost.lane.run(OperationEffect::Cancel { id: OperationId::new(42), generation: 3 }), vec![Event::OperationCleaned { id: OperationId::new(42), generation: 3, result: CleanupResult::Clean }], "a cancel with no state still reports");
    }

    #[test]
    fn cleanup_removes_a_temporary_the_step_could_not_and_reports_what_it_cannot() {
        let mut run = Run::new();
        run.write("from/a.txt", "a");
        run.write("to/a.txt", "old");
        let id = run.start(&["a.txt"], OperationKind::Copy);
        // A temporary that this operation recorded but could not discard.
        let leftover = run.path("to").join(crate::operation_journal::temporary_name(LaunchId::from_bytes([9; 16]), id.get(), 7));
        fs::write(&leftover, b"partial").unwrap();
        run.lane.services.journal.record(id.get(), &run.path("to"), fs::metadata(run.path("to")).unwrap().dev()).unwrap();
        run.fs.failing_removals.lock().unwrap().push(leftover.clone());
        let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
        run.drive(work);
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::CleanupUncertain), "a temporary that cannot be shown removed is reported");
        assert_eq!(run.read("to/a.txt"), "old");
    }

    #[test]
    fn the_write_primitive_decides_a_conflict_the_fast_path_missed() {
        let mut run = Run::new();
        run.write("from/a.txt", "mine");
        *run.fs.racing_destination.lock().unwrap() = Some(run.path("to/a.txt"));
        let id = run.start(&["a.txt"], OperationKind::Copy);
        assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
        assert_eq!(run.read("to/a.txt"), "racer", "the item that appeared first is kept");
        assert!(run.temporaries().is_empty());
    }

    #[test]
    fn a_moved_folder_holding_skipped_content_moves_entry_by_entry() {
        let mut run = Run::new();
        run.write("from/dir/a.txt", "a");
        run.write("from/dir/locked/secret.txt", "secret");
        fs::set_permissions(run.path("from/dir/locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let id = run.start(&["dir"], OperationKind::Move);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::PermissionDenied), "the unreadable folder asks during the scan");
        run.decide(id, OperationChoice::Skip, false);
        fs::set_permissions(run.path("from/dir/locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!(run.read("to/dir/a.txt"), "a");
        assert_eq!(run.read("from/dir/locked/secret.txt"), "secret", "the skipped content and the folder above it stay");
        assert!(!run.path("to/dir/locked").exists());
    }

    #[test]
    fn a_destination_inside_the_source_by_another_spelling_is_refused() {
        let mut run = Run::new();
        fs::create_dir_all(run.path("from/dir/inner")).unwrap();
        let variant = run.path("FROM/DIR/inner");
        if !variant.is_dir() {
            // A case-sensitive volume has no second spelling to test.
            return;
        }
        fs::remove_dir(run.path("to")).unwrap();
        run.show();
        let destination = location_from_path(&variant).unwrap();
        let read = run.ws.handle(Command::Navigate { browser: BrowserSide::Right, location: destination }.into()).work;
        run.drive(read);
        let tab = run.ws.active_tab(BrowserSide::Left);
        run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("dir") }.into());
        let started = run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Copy }.into());
        let id = run.ws.operation_jobs()[0].id();
        run.drive(started.work);
        assert_eq!(run.waiting(id).1, Some(OperationErrorKind::DestinationWithinSource), "device and inode catch what name comparison cannot");
    }
}
