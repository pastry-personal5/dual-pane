//! Copy steps: folders, files, and their temporaries.

use super::*;

impl OperationState {
    pub(super) fn copy(&mut self, context: &mut StepContext<'_>, plan: &[PlannedItem], index: usize, identity: Option<Identity>, choice: Option<OperationChoice>) -> Item {
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
    /// Creates `target` for a folder, or merges into an existing folder.
    pub(super) fn make_folder(&mut self, context: &StepContext<'_>, source: &Path, target: &Path, index: usize) -> Item {
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
    pub(super) fn existing_folder(&self, target: &Path, index: usize) -> Item {
        match fs::symlink_metadata(target) {
            Ok(metadata) if metadata.is_dir() => Item::Done { next: index + 1, completed: 1 },
            Ok(metadata) => conflict(EntryKind::Directory, &metadata, target).into(),
            Err(error) => error_stop(self.id, "read destination", &error).into(),
        }
    }
    /// Copies a file or link to `target` through a temporary in its folder.
    /// A new item appears only by exclusive rename; a replacement only after
    /// its data is complete, by one rename over the unchanged destination.
    pub(super) fn copy_entry(&mut self, context: &mut StepContext<'_>, kind: EntryKind, source: &Path, identity: Identity, target: &Path, choice: Option<OperationChoice>) -> Result<(), Item> {
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
    pub(super) fn fill(&mut self, context: &mut StepContext<'_>, source: &Path, identity: Identity, folder: &Path, temporary: &Path) -> Result<(), Item> {
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
    pub(super) fn temporary(&mut self, context: &StepContext<'_>, folder: &Path) -> Result<PathBuf, Item> {
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
    pub(super) fn discard(&mut self, fs: &dyn FileSystem, temporary: &Path) {
        match fs.remove_file(temporary) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => log(self.id, "remove temporary", &error),
            _ => self.temporaries.retain(|path| path != temporary),
        }
    }
    /// Rejects a folder root whose destination folder lies inside it through
    /// a link or another spelling of its path, by device and inode.
    pub(super) fn outside_source(&self, entry: &PlannedItem, identity: Identity) -> Result<(), Stop> {
        if !self.intent.roots().iter().any(|root| root.source == entry.source) {
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
}
