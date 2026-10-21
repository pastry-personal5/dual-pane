//! Move steps within a volume and across volumes.

use super::*;

impl OperationState {
    pub(super) fn move_item(&mut self, context: &mut StepContext<'_>, plan: &[PlannedItem], index: usize, identity: Option<Identity>, choice: Option<OperationChoice>) -> Item {
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
    pub(super) fn move_on_volume(&mut self, context: &StepContext<'_>, kind: EntryKind, source: &Path, target: &Path, choice: Option<OperationChoice>) -> Result<(), Item> {
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
    pub(super) fn remove_moved_source(&mut self, context: &StepContext<'_>, source: &Path, identity: Identity) -> Result<(), Stop> {
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
}
