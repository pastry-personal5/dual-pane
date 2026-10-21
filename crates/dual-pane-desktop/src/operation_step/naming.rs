//! New Folder and Rename steps.

use super::*;

impl OperationState {
    pub(super) fn new_folder(&mut self, context: &StepContext<'_>, entry: &PlannedItem) -> Result<(), Item> {
        if context.cancelled() {
            return Err(Item::Stopped);
        }
        context.fs.create_dir(&path_from_location(&entry.source)).map_err(|error| name_error(self.id, "create folder", &error).into())
    }
    /// Renames with exclusive rename. A collision that turns out to be the
    /// same file under a case or normalization variant of its name is
    /// renamed in place; identity decides it, never a name comparison.
    pub(super) fn rename(&mut self, context: &StepContext<'_>, entry: &PlannedItem, identity: Option<Identity>) -> Result<(), Item> {
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
}
