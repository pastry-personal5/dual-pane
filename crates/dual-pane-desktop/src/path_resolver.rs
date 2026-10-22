//! Resolves an entered path to the stored spelling of every component.
//!
//! The walk never uses `fs::canonicalize`: a link keeps its own name in the
//! result and is followed only so the walk can continue. It runs on a
//! Browser's read lanes, never on the GUI thread.

use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use dual_pane_application::{PathTarget, ResolvedPath};
use dual_pane_domain::{EntryName, ListingErrorKind, Location};

use crate::native_calls::stored_name;

/// What the resolver reports: the rebuilt location and what it names, a
/// listing error category, or `None` when `cancelled` was set.
pub type Resolution = Option<Result<ResolvedPath, ListingErrorKind>>;

pub fn resolve_path(candidate: &Location, cancelled: &AtomicBool) -> Resolution {
    let mut walked = PathBuf::from("/");
    let mut stored = Vec::with_capacity(candidate.components().len());
    for component in candidate.components() {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let typed = walked.join(std::ffi::OsString::from_vec(component.as_bytes().to_vec()));
        let name = match stored_name(&typed) {
            Ok(name) => name,
            Err(error) => return Some(Err(map_error(&error))),
        };
        let Ok(name) = EntryName::new(name) else { return Some(Err(ListingErrorKind::Internal)) };
        walked.push(std::ffi::OsString::from_vec(name.as_bytes().to_vec()));
        stored.push(name);
    }
    if cancelled.load(Ordering::Relaxed) {
        return None;
    }
    // Following links here classifies the target: a link to a folder or
    // package is a folder, and a dangling link is missing.
    let target = match fs::metadata(&walked) {
        Ok(metadata) if metadata.is_dir() => PathTarget::Folder,
        Ok(_) => PathTarget::File,
        Err(error) => return Some(Err(map_error(&error))),
    };
    Some(Ok(ResolvedPath { location: Location::from_components(stored), target }))
}

/// `ELOOP`, `ENOENT`, and a dangling link are missing, a non-folder
/// intermediate component is not a folder, `EACCES` is permission denied,
/// `EPERM` is a privacy denial, and every other errno is internal.
fn map_error(error: &io::Error) -> ListingErrorKind {
    match error.raw_os_error() {
        Some(libc::ELOOP | libc::ENOENT) => ListingErrorKind::ItemMissing,
        Some(libc::ENOTDIR) => ListingErrorKind::NotADirectory,
        Some(libc::EACCES) => ListingErrorKind::PermissionDenied,
        Some(libc::EPERM) => ListingErrorKind::PrivacyRestricted,
        _ => ListingErrorKind::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::native_location::{location_from_path, path_from_location};

    fn resolve(path: &Path) -> Resolution {
        resolve_path(&location_from_path(path).unwrap(), &AtomicBool::new(false))
    }
    fn resolved(path: &Path) -> ResolvedPath {
        resolve(path).expect("not cancelled").expect("resolves")
    }
    fn failure(path: &Path) -> ListingErrorKind {
        resolve(path).expect("not cancelled").expect_err("fails")
    }
    /// A temporary directory under a canonical path, because `/var` is a link.
    fn temporary() -> (tempfile::TempDir, PathBuf) {
        let directory = tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        (directory, root)
    }

    #[test]
    fn root_is_a_folder() {
        let found = resolved(Path::new("/"));
        assert_eq!((found.location, found.target), (Location::root(), PathTarget::Folder));
    }

    #[test]
    fn a_folder_and_a_file_are_classified() {
        let (_guard, root) = temporary();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("file"), b"").unwrap();
        assert_eq!(resolved(&root.join("folder")).target, PathTarget::Folder);
        assert_eq!(resolved(&root.join("file")).target, PathTarget::File);
        assert_eq!(resolved(&root.join("file")).location, location_from_path(&root.join("file")).unwrap());
    }

    #[test]
    fn a_package_is_a_folder() {
        let (_guard, root) = temporary();
        fs::create_dir_all(root.join("Tool.app/Contents")).unwrap();
        assert_eq!(resolved(&root.join("Tool.app")).target, PathTarget::Folder);
    }

    #[test]
    fn a_link_keeps_its_name_and_a_link_to_a_file_is_a_file() {
        let (_guard, root) = temporary();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("file"), b"").unwrap();
        symlink("folder", root.join("to-folder")).unwrap();
        symlink("file", root.join("to-file")).unwrap();
        let folder = resolved(&root.join("to-folder"));
        assert_eq!(folder.target, PathTarget::Folder);
        assert_eq!(path_from_location(&folder.location), root.join("to-folder"));
        let file = resolved(&root.join("to-file"));
        assert_eq!(file.target, PathTarget::File);
        assert_eq!(path_from_location(&file.location), root.join("to-file"));
        fs::write(root.join("folder/inner"), b"").unwrap();
        assert_eq!(path_from_location(&resolved(&root.join("to-folder/inner")).location), root.join("to-folder/inner"), "a link in the middle is followed but keeps its name");
    }

    #[test]
    fn missing_dangling_and_loop_report_missing() {
        let (_guard, root) = temporary();
        symlink("missing", root.join("dangling")).unwrap();
        symlink("loop-b", root.join("loop-a")).unwrap();
        symlink("loop-a", root.join("loop-b")).unwrap();
        for name in ["absent", "dangling", "loop-a", "absent/deeper"] {
            assert_eq!(failure(&root.join(name)), ListingErrorKind::ItemMissing, "{name}");
        }
    }

    #[test]
    fn a_file_in_the_middle_is_not_a_folder() {
        let (_guard, root) = temporary();
        fs::write(root.join("file"), b"").unwrap();
        assert_eq!(failure(&root.join("file/inside")), ListingErrorKind::NotADirectory);
    }

    #[test]
    fn an_unsearchable_folder_is_permission_denied() {
        let (_guard, root) = temporary();
        fs::create_dir_all(root.join("locked/inner")).unwrap();
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let outcome = failure(&root.join("locked/inner"));
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(outcome, ListingErrorKind::PermissionDenied);
    }

    #[test]
    fn typed_spelling_variants_resolve_to_the_stored_spelling() {
        let (_guard, root) = temporary();
        let decomposed = "cafe\u{301}";
        let precomposed = "caf\u{e9}";
        fs::create_dir(root.join("Stored")).unwrap();
        fs::create_dir(root.join(precomposed)).unwrap();
        let ignores_case = fs::metadata(root.join("stored")).is_ok();
        let ignores_normalization = fs::metadata(root.join(decomposed)).is_ok();
        if ignores_case {
            assert_eq!(path_from_location(&resolved(&root.join("stored")).location), root.join("Stored"));
        }
        if ignores_normalization {
            assert_eq!(path_from_location(&resolved(&root.join(decomposed)).location), root.join(precomposed));
        }
        assert!(ignores_case || ignores_normalization || cfg!(not(target_os = "macos")), "the temporary volume should be APFS, which ignores at least normalization");
    }

    #[test]
    fn names_that_are_not_valid_utf8_cannot_exist_on_this_volume() {
        let (_guard, root) = temporary();
        let odd = root.join(std::ffi::OsString::from_vec(b"caf\xFF".to_vec()));
        assert!(fs::create_dir(&odd).is_err(), "APFS refuses such names (EILSEQ)");
    }

    #[test]
    fn a_cancelled_walk_reports_nothing() {
        let (_guard, root) = temporary();
        fs::create_dir(root.join("folder")).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(resolve_path(&location_from_path(&root.join("folder")).unwrap(), &cancelled).is_none());
    }
}
