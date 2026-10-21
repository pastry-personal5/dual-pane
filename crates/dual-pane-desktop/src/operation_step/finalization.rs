//! Finalization of source folders and cancellation cleanup.

use super::*;

impl OperationState {
    /// Gives the folders this operation created their source's metadata,
    /// deepest first, once their contents are in place.
    pub(super) fn finish_folders(&mut self, fs: &dyn FileSystem) {
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
pub(super) fn remove_operation_temporaries(id: OperationId, fs: &dyn FileSystem, journal: &Journal, folders: &HashSet<PathBuf>) -> bool {
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
pub(super) fn subtree_end(plan: &[PlannedItem], at: usize) -> usize {
    plan[at + 1..].iter().position(|item| !starts_with(&item.source, &plan[at].source)).map_or(plan.len(), |offset| at + 1 + offset)
}
