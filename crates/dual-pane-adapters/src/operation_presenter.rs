//! Wording for file operations: summaries, failures, and name problems.

use dual_pane_application::{NameProblem, OperationErrorKind, OperationFailure, OperationOutcome, OperationProgress};
use dual_pane_domain::{EntryName, OperationIntent, OperationKind, OperationRejection};

use crate::format::location_text;

/// Why an item could not be handled, as a clause after its name.
pub fn operation_error_text(kind: OperationErrorKind) -> &'static str {
    match kind {
        OperationErrorKind::PermissionDenied => "you don’t have permission",
        OperationErrorKind::NotFound => "it no longer exists",
        OperationErrorKind::NoSpace => "there isn’t enough space",
        OperationErrorKind::Busy => "it’s in use",
        OperationErrorKind::ExecutorUnavailable => "the operation stopped unexpectedly",
        OperationErrorKind::Other => "an unexpected error occurred",
        OperationErrorKind::ReadOnly => "the volume is read-only",
        OperationErrorKind::PrivacyRestricted => "macOS privacy settings don’t allow it",
        OperationErrorKind::ChangedSinceScan => "it changed after it was checked",
        OperationErrorKind::UnsupportedItem => "this kind of item can’t be copied",
        OperationErrorKind::TrashUnavailable => "its volume has no Trash",
        OperationErrorKind::SourceRemovalFailed => "it was copied, but the original couldn’t be removed",
        OperationErrorKind::JournalUnavailable => "Dual Pane couldn’t record its temporary files",
        OperationErrorKind::FolderNotEmpty => "the folder isn’t empty",
        OperationErrorKind::DestinationWithinSource => "the destination is inside the folder being copied or moved",
    }
}

/// Inline editor wording for a refused Rename or New Folder name.
pub fn name_problem_text(problem: NameProblem) -> &'static str {
    match problem {
        NameProblem::Collision => "An item with this name already exists.",
        NameProblem::TooLong => "This name is too long.",
        NameProblem::RejectedCharacters => "This name contains characters the volume doesn’t allow.",
    }
}

/// Inline editor wording for a name refused before any job starts, or
/// `None` when the editor should simply close, as for an unchanged name.
pub fn name_rejection_text(reason: OperationRejection) -> Option<&'static str> {
    match reason {
        OperationRejection::UnchangedName => None,
        OperationRejection::InvalidName => Some("Enter a name."),
        _ => Some(rejection_text(reason)),
    }
}

/// Inline editor wording for text that cannot be a file name, such as one
/// containing `/`.
pub const UNREPRESENTABLE_NAME_TEXT: &str = "Names can’t contain “/”, be empty, or be “.” or “..”.";

/// Why a file command is unavailable, for the Status Bar.
pub fn rejection_text(reason: OperationRejection) -> &'static str {
    match reason {
        OperationRejection::StaleTab | OperationRejection::StaleSelection => "The selection changed. Try again.",
        OperationRejection::NoTargets => "Select one or more items first.",
        OperationRejection::DuplicateTarget => "The selection lists an item twice.",
        OperationRejection::InvalidTargetCount => "Select exactly one item to rename.",
        OperationRejection::InvalidName => "Enter a name.",
        OperationRejection::UnchangedName => "The name is unchanged.",
        OperationRejection::DestinationRequired | OperationRejection::DestinationUnavailable => "The other Browser has no folder to receive items.",
        OperationRejection::DestinationNotAllowed => "This command doesn’t use the other Browser.",
        OperationRejection::SameSourceAndDestination => "Both Browsers show the same folder.",
        OperationRejection::DestinationWithinSource => "A folder can’t be placed inside itself.",
        OperationRejection::SourceUnavailable => "This folder isn’t ready yet.",
        OperationRejection::JournalUnavailable => "Copy is unavailable because Dual Pane couldn’t open its safety journal.",
    }
}

/// The operation's name, for titles: `Copy`, `Move`, and so on.
pub fn operation_title(kind: &OperationKind) -> &'static str {
    match kind {
        OperationKind::Copy => "Copy",
        OperationKind::Move => "Move",
        OperationKind::Rename { .. } => "Rename",
        OperationKind::NewFolder { .. } => "New Folder",
        OperationKind::MoveToTrash => "Move to Trash",
        OperationKind::DeletePermanently => "Delete Permanently",
    }
}

fn quoted(name: &EntryName) -> String {
    format!("“{}”", name.to_text_lossy())
}

/// The targets as a phrase: one quoted name, or a count.
fn subject(intent: &OperationIntent) -> String {
    match (intent.kind(), intent.targets()) {
        (OperationKind::NewFolder { name }, _) => quoted(name),
        (_, [only]) => quoted(&only.name),
        (_, targets) => format!("{} items", targets.len()),
    }
}

/// A one-line summary of a finished operation for Notices.
pub fn operation_summary(intent: &OperationIntent, outcome: OperationOutcome, progress: OperationProgress, failure: Option<&OperationFailure>) -> String {
    let subject = subject(intent);
    let destination = intent.destination().map(|destination| format!(" to “{}”", location_text(destination))).unwrap_or_default();
    let done = match intent.kind() {
        OperationKind::Copy => format!("Copied {subject}{destination}"),
        OperationKind::Move => format!("Moved {subject}{destination}"),
        OperationKind::Rename { to } => format!("Renamed {subject} to {}", quoted(to)),
        OperationKind::NewFolder { .. } => format!("Created folder {subject} in “{}”", location_text(intent.source())),
        OperationKind::MoveToTrash => format!("Moved {subject} to the Trash"),
        OperationKind::DeletePermanently => format!("Permanently deleted {subject}"),
    };
    let action = match intent.kind() {
        OperationKind::Copy => "copy",
        OperationKind::Move => "move",
        OperationKind::Rename { .. } => "rename",
        OperationKind::NewFolder { .. } => "create",
        OperationKind::MoveToTrash => "move to the Trash",
        OperationKind::DeletePermanently => "delete",
    };
    let reason = failure.map(|failure| format!(" Couldn’t {action} “{}” because {}.", location_text(&failure.item), operation_error_text(failure.kind))).unwrap_or_default();
    let title = operation_title(intent.kind());
    match outcome {
        OperationOutcome::Succeeded => format!("{done}."),
        OperationOutcome::Partial => {
            let total = progress.completed + progress.skipped + progress.failed;
            let skipped = if progress.skipped > 0 { format!(" {} skipped.", progress.skipped) } else { String::new() };
            format!("{title} finished partly: {} of {total} items done.{skipped}{reason}", progress.completed)
        }
        OperationOutcome::Failed if failure.is_some() => format!("{title} of {subject} failed.{reason}"),
        OperationOutcome::Failed => format!("{title} of {subject} failed."),
        OperationOutcome::Cancelled => format!("{title} of {subject} was cancelled."),
        OperationOutcome::CleanupUncertain => format!("{title} of {subject} was cancelled, but some temporary items may remain."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::{EntryKind, Location, OperationTarget};

    fn name(text: &str) -> EntryName {
        EntryName::new(text).unwrap()
    }
    fn path(text: &str) -> Location {
        Location::root().join(&name(text))
    }
    fn copy(names: &[&str]) -> OperationIntent {
        OperationIntent::new(OperationKind::Copy, path("from"), names.iter().map(|text| OperationTarget { name: name(text), kind: EntryKind::File }).collect(), Some(path("to"))).unwrap()
    }

    #[test]
    fn summaries_name_the_targets_and_the_outcome() {
        let done = OperationProgress { completed: 1, skipped: 0, failed: 0 };
        assert_eq!(operation_summary(&copy(&["a"]), OperationOutcome::Succeeded, done, None), "Copied “a” to “/to”.");
        assert_eq!(operation_summary(&copy(&["a", "b"]), OperationOutcome::Cancelled, OperationProgress::default(), None), "Copy of 2 items was cancelled.");
        let failure = OperationFailure { item: path("from").join(&name("b")), kind: OperationErrorKind::NoSpace };
        let partial = OperationProgress { completed: 1, skipped: 1, failed: 0 };
        assert_eq!(operation_summary(&copy(&["a", "b"]), OperationOutcome::Partial, partial, Some(&failure)), "Copy finished partly: 1 of 2 items done. 1 skipped. Couldn’t copy “/from/b” because there isn’t enough space.");
        let rename = OperationIntent::new(OperationKind::Rename { to: name("z") }, path("from"), vec![OperationTarget { name: name("a"), kind: EntryKind::File }], None).unwrap();
        assert_eq!(operation_summary(&rename, OperationOutcome::Succeeded, done, None), "Renamed “a” to “z”.");
    }

    #[test]
    fn every_error_kind_and_rejection_has_wording() {
        for kind in [OperationErrorKind::PermissionDenied, OperationErrorKind::ReadOnly, OperationErrorKind::PrivacyRestricted, OperationErrorKind::ChangedSinceScan, OperationErrorKind::UnsupportedItem, OperationErrorKind::TrashUnavailable, OperationErrorKind::SourceRemovalFailed, OperationErrorKind::JournalUnavailable, OperationErrorKind::FolderNotEmpty, OperationErrorKind::DestinationWithinSource] {
            assert!(!operation_error_text(kind).is_empty());
        }
        assert_eq!(name_rejection_text(OperationRejection::UnchangedName), None, "an unchanged name closes the editor");
        assert_eq!(name_rejection_text(OperationRejection::InvalidName), Some("Enter a name."));
        assert_eq!(name_problem_text(NameProblem::Collision), "An item with this name already exists.");
    }
}
