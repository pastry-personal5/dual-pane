//! Checks that decide when a step must stop, and how failures are reported.

use super::*;

/// Checks that `path` is still the item the scan saw.
pub(super) fn revalidate(path: &Path, identity: Identity) -> Result<Metadata, Stop> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if identity.matches(&Identity::of(&metadata)) => Ok(metadata),
        Ok(_) => Err(Stop::Error(OperationErrorKind::ChangedSinceScan)),
        Err(error) => Err(Stop::Error(error_kind(&error))),
    }
}

/// The conflict an existing `target` raises for an arriving `kind`.
pub(super) fn conflict(kind: EntryKind, existing: &Metadata, target: &Path) -> Stop {
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
pub(super) fn existing_destination(kind: EntryKind, target: &Path) -> Stop {
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
pub(super) fn same_entry_variant(source: &Path, target: &Path, identity: Identity) -> bool {
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
pub(super) fn name_error(id: OperationId, action: &str, error: &io::Error) -> Stop {
    match error.raw_os_error() {
        Some(libc::EEXIST) => Stop::Name(NameProblem::Collision),
        Some(libc::ENAMETOOLONG) => Stop::Name(NameProblem::TooLong),
        Some(libc::EILSEQ | libc::EINVAL) => Stop::Name(NameProblem::RejectedCharacters),
        _ => error_stop(id, action, error),
    }
}

pub(super) fn error_stop(id: OperationId, action: &str, error: &io::Error) -> Stop {
    log(id, action, error);
    Stop::Error(error_kind(error))
}

/// Keeps the raw native error in the log only.
pub(super) fn log(id: OperationId, action: &str, error: &io::Error) {
    eprintln!("Dual Pane operation {}: {action} failed: {error}", id.get());
}
