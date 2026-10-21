//! Move to Trash and permanent deletion steps.

use super::*;

impl OperationState {
    pub(super) fn trash(&mut self, context: &StepContext<'_>, entry: &PlannedItem, identity: Option<Identity>) -> Result<(), Item> {
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
    pub(super) fn delete(&mut self, context: &StepContext<'_>, entry: &PlannedItem, identity: Option<Identity>) -> Result<(), Item> {
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
}
