use std::fs::{self, DirEntry, FileType};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, Location, listing_sort_key};

use crate::native_location::path_from_location;

type ListingOutcome = Option<Result<Arc<[Entry]>, ListingErrorKind>>;

/// Reads one complete directory snapshot, or returns cancellation or one
/// classified terminal failure. The runtime invokes this only on its worker.
pub fn read(location: &Location, cancelled: &AtomicBool) -> ListingOutcome {
    if is_cancelled(cancelled) {
        return None;
    }
    let path = path_from_location(location);
    let directory = match fs::read_dir(path) {
        Ok(directory) => directory,
        Err(error) => return Some(Err(map_error(&error))),
    };
    let entries = match collect_entries(directory, cancelled, classify_dir_entry)? {
        Ok(entries) => entries,
        Err(kind) => return Some(Err(kind)),
    };
    sort_entries(entries, cancelled).map(Ok)
}

fn collect_entries<T>(items: impl IntoIterator<Item = io::Result<T>>, cancelled: &AtomicBool, mut classify: impl FnMut(T, &AtomicBool) -> Option<Result<Option<Entry>, ListingErrorKind>>) -> Option<Result<Vec<Entry>, ListingErrorKind>> {
    let mut entries = Vec::new();
    for item in items {
        if is_cancelled(cancelled) {
            return None;
        }
        let item = match item {
            Ok(item) => item,
            Err(error) => return Some(Err(map_error(&error))),
        };
        match classify(item, cancelled)? {
            Ok(Some(entry)) => entries.push(entry),
            Ok(None) => {}
            Err(kind) => return Some(Err(kind)),
        }
    }
    Some(Ok(entries))
}

fn classify_dir_entry(entry: DirEntry, cancelled: &AtomicBool) -> Option<Result<Option<Entry>, ListingErrorKind>> {
    if is_cancelled(cancelled) {
        return None;
    }
    let name = match entry_name(entry.file_name().as_bytes()) {
        Ok(Some(name)) => name,
        Ok(None) => return Some(Ok(None)),
        Err(kind) => return Some(Err(kind)),
    };
    if is_cancelled(cancelled) {
        return None;
    }
    let own_kind = match classify_own_type(entry.file_type().map(native_kind)) {
        Ok(Some(kind)) => kind,
        Ok(None) => return Some(Ok(None)),
        Err(kind) => return Some(Err(kind)),
    };
    if is_cancelled(cancelled) {
        return None;
    }
    let kind = if own_kind == NativeKind::Symlink {
        let target = fs::metadata(entry.path()).map(|metadata| native_kind(metadata.file_type()));
        if is_cancelled(cancelled) {
            return None;
        }
        classify_kind(own_kind, target)
    } else {
        classify_kind(own_kind, Ok(NativeKind::Other))
    };
    Some(Ok(Some(Entry::new(name, kind))))
}

fn sort_entries(entries: Vec<Entry>, cancelled: &AtomicBool) -> Option<Arc<[Entry]>> {
    if is_cancelled(cancelled) {
        return None;
    }
    let mut keyed = Vec::with_capacity(entries.len());
    for entry in entries {
        if is_cancelled(cancelled) {
            return None;
        }
        keyed.push((listing_sort_key(&entry), entry));
    }
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    if is_cancelled(cancelled) {
        return None;
    }
    Some(keyed.into_iter().map(|(_, entry)| entry).collect())
}

fn entry_name(bytes: &[u8]) -> Result<Option<EntryName>, ListingErrorKind> {
    if bytes == b"." || bytes == b".." {
        return Ok(None);
    }
    EntryName::new(bytes.to_vec()).map(Some).map_err(|_| ListingErrorKind::Internal)
}

fn classify_own_type(result: io::Result<NativeKind>) -> Result<Option<NativeKind>, ListingErrorKind> {
    match result {
        Ok(kind) => Ok(Some(kind)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(map_error(&error)),
    }
}

fn classify_kind(own: NativeKind, target: io::Result<NativeKind>) -> EntryKind {
    match own {
        NativeKind::Directory => EntryKind::Directory,
        NativeKind::File => EntryKind::File,
        NativeKind::Symlink => EntryKind::Symlink { points_to_directory: matches!(target, Ok(NativeKind::Directory)) },
        NativeKind::Other => EntryKind::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeKind {
    Directory,
    File,
    Symlink,
    Other,
}

fn native_kind(kind: FileType) -> NativeKind {
    if kind.is_dir() {
        NativeKind::Directory
    } else if kind.is_file() {
        NativeKind::File
    } else if kind.is_symlink() {
        NativeKind::Symlink
    } else {
        NativeKind::Other
    }
}

fn map_error(error: &io::Error) -> ListingErrorKind {
    match error.raw_os_error() {
        Some(1) => return ListingErrorKind::PrivacyRestricted,
        Some(13) => return ListingErrorKind::PermissionDenied,
        _ => {}
    }
    match error.kind() {
        io::ErrorKind::NotFound => ListingErrorKind::ItemMissing,
        io::ErrorKind::NotADirectory => ListingErrorKind::NotADirectory,
        io::ErrorKind::PermissionDenied => ListingErrorKind::PermissionDenied,
        _ => ListingErrorKind::Unknown,
    }
}

fn is_cancelled(cancelled: &AtomicBool) -> bool {
    cancelled.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs::File;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    use tempfile::tempdir;

    use super::*;
    use crate::native_location::location_from_path;

    #[test]
    fn reads_exact_names_classifies_every_kind_and_sorts() {
        let temporary = tempdir().unwrap();
        fs::create_dir(temporary.path().join("folder10")).unwrap();
        fs::create_dir(temporary.path().join("folder2")).unwrap();
        File::create(temporary.path().join("file10")).unwrap();
        File::create(temporary.path().join("file2")).unwrap();
        File::create(temporary.path().join(".hidden")).unwrap();
        let invalid = OsString::from_vec(b"caf\xFF".to_vec());
        let invalid_created = match File::create(temporary.path().join(&invalid)) {
            Ok(_) => true,
            Err(error) if matches!(error.raw_os_error(), Some(1 | 92)) => false,
            Err(error) => panic!("unexpected invalid-byte filename error: {error}"),
        };
        symlink("folder2", temporary.path().join("folder-link")).unwrap();
        symlink("file2", temporary.path().join("file-link")).unwrap();
        symlink("missing", temporary.path().join("dangling-link")).unwrap();
        let socket_path = temporary.path().join("socket");
        let _socket = UnixListener::bind(&socket_path).unwrap();

        let location = location_from_path(temporary.path()).unwrap();
        let entries = read(&location, &AtomicBool::new(false)).unwrap().unwrap();

        assert!(entries.windows(2).all(|pair| listing_sort_key(&pair[0]) < listing_sort_key(&pair[1])));
        let kind = |bytes: &[u8]| entries.iter().find(|entry| entry.name().as_bytes() == bytes).map(Entry::kind);
        assert_eq!(kind(b"folder2"), Some(EntryKind::Directory));
        assert_eq!(kind(b"file2"), Some(EntryKind::File));
        assert_eq!(kind(b"folder-link"), Some(EntryKind::Symlink { points_to_directory: true }));
        assert_eq!(kind(b"file-link"), Some(EntryKind::Symlink { points_to_directory: false }));
        assert_eq!(kind(b"dangling-link"), Some(EntryKind::Symlink { points_to_directory: false }));
        assert_eq!(kind(b"socket"), Some(EntryKind::Other));
        assert_eq!(kind(b".hidden"), Some(EntryKind::File));
        if invalid_created {
            assert_eq!(kind(b"caf\xFF"), Some(EntryKind::File));
        }
        assert!(entries.iter().all(|entry| !matches!(entry.name().as_bytes(), b"." | b"..")));
    }

    #[test]
    fn entry_names_remain_byte_exact_when_text_is_not_valid_utf8() {
        let name = entry_name(b"caf\xFF").unwrap().unwrap();
        assert_eq!(name.as_bytes(), b"caf\xFF");
    }

    #[test]
    fn exact_navigation_names_are_rejected_defensively() {
        assert_eq!(entry_name(b"."), Ok(None));
        assert_eq!(entry_name(b".."), Ok(None));
        assert_eq!(entry_name(b".hidden").unwrap().unwrap().as_bytes(), b".hidden");
    }

    #[test]
    fn own_not_found_is_omitted_but_other_own_errors_fail() {
        assert_eq!(classify_own_type(Err(io::Error::from(io::ErrorKind::NotFound))), Ok(None));
        assert_eq!(classify_own_type(Err(io::Error::from_raw_os_error(13))), Err(ListingErrorKind::PermissionDenied));
    }

    #[test]
    fn every_target_probe_error_retains_a_non_enterable_link() {
        let errors = [io::Error::from_raw_os_error(13), io::Error::from_raw_os_error(1), io::Error::from_raw_os_error(62), io::Error::from(io::ErrorKind::NotFound), io::Error::from(io::ErrorKind::NotADirectory), io::Error::other("unclassified")];
        for error in errors {
            assert_eq!(classify_kind(NativeKind::Symlink, Err(error)), EntryKind::Symlink { points_to_directory: false });
        }
        assert_eq!(classify_kind(NativeKind::Symlink, Ok(NativeKind::Directory)), EntryKind::Symlink { points_to_directory: true });
    }

    #[test]
    fn iterator_errors_discard_partial_results() {
        let items = [Ok(1_u8), Err(io::Error::other("iteration failed")), Ok(2_u8)];
        let result = collect_entries(items, &AtomicBool::new(false), |value, _| Some(Ok(Some(Entry::new(EntryName::new(format!("item{value}")).unwrap(), EntryKind::File)))));
        assert_eq!(result, Some(Err(ListingErrorKind::Unknown)));
    }

    #[test]
    fn cancellation_is_checked_before_and_during_collection() {
        assert_eq!(read(&Location::root(), &AtomicBool::new(true)), None);
        let cancelled = AtomicBool::new(false);
        let result = collect_entries([Ok(1_u8), Ok(2_u8)], &cancelled, |value, flag| {
            if value == 1 {
                flag.store(true, Ordering::Relaxed);
            }
            Some(Ok(Some(Entry::new(EntryName::new(format!("item{value}")).unwrap(), EntryKind::File))))
        });
        assert_eq!(result, None);
    }

    #[test]
    fn terminal_errors_map_to_application_categories() {
        assert_eq!(map_error(&io::Error::from(io::ErrorKind::NotFound)), ListingErrorKind::ItemMissing);
        assert_eq!(map_error(&io::Error::from(io::ErrorKind::NotADirectory)), ListingErrorKind::NotADirectory);
        assert_eq!(map_error(&io::Error::from_raw_os_error(13)), ListingErrorKind::PermissionDenied);
        assert_eq!(map_error(&io::Error::from_raw_os_error(1)), ListingErrorKind::PrivacyRestricted);
        assert_eq!(map_error(&io::Error::other("unclassified")), ListingErrorKind::Unknown);
    }

    #[test]
    fn missing_and_non_directory_locations_fail_without_rows() {
        let temporary = tempdir().unwrap();
        let missing = location_from_path(&temporary.path().join("missing")).unwrap();
        assert_eq!(read(&missing, &AtomicBool::new(false)), Some(Err(ListingErrorKind::ItemMissing)));

        let file_path = temporary.path().join("file");
        File::create(&file_path).unwrap();
        let file = location_from_path(&file_path).unwrap();
        assert_eq!(read(&file, &AtomicBool::new(false)), Some(Err(ListingErrorKind::NotADirectory)));
    }
}
