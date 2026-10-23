use std::fs::{self, DirEntry, FileType};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use dual_pane_application::VisibilityPolicy;
use dual_pane_domain::{Entry, EntryKind, EntryMetadata, EntryName, ListingErrorKind, Location, SortSpec, sort_entries};

use crate::native_location::path_from_location;

type FolderItemsOutcome = Option<Result<Arc<[Entry]>, ListingErrorKind>>;

pub const FOLDER_CHOICE_PAGE_SIZE: usize = 256;

/// One bounded page of navigable child names. `next` is the last name in the
/// page when a later page exists; callers pass it back as an exclusive cursor.
pub struct FolderChoicePage {
    pub names: Arc<[EntryName]>,
    pub next: Option<EntryName>,
    pub selected_found: bool,
}

/// Reads folder choices without accumulating the directory in memory. The
/// worker scans immediate children and retains only the first page in exact
/// byte order. A selected path component is allowed through visibility rules
/// even when its siblings are hidden, but operation temporaries remain hidden.
pub fn read_folder_choices(parent: &Location, selected: Option<&EntryName>, after: Option<&EntryName>, visibility: VisibilityPolicy, cancelled: &AtomicBool, rules: &ListingRules<'_>) -> Option<Result<FolderChoicePage, ListingErrorKind>> {
    if is_cancelled(cancelled) {
        return None;
    }
    let path = path_from_location(parent);
    let directory = match fs::read_dir(&path) {
        Ok(directory) => directory,
        Err(error) => return Some(Err(map_error(&error))),
    };
    let mut names = Vec::with_capacity(FOLDER_CHOICE_PAGE_SIZE + 1);
    let mut selected_found = false;
    for item in directory {
        if is_cancelled(cancelled) {
            return None;
        }
        let item = match item {
            Ok(item) => item,
            Err(error) => return Some(Err(map_error(&error))),
        };
        let own_name = item.file_name();
        let bytes = own_name.as_bytes();
        if after.is_some_and(|after| bytes <= after.as_bytes()) {
            continue;
        }
        let on_path = selected.is_some_and(|selected| selected.as_bytes() == bytes);
        let policy = if on_path { VisibilityPolicy { show_macos_items: true, show_dotfiles: true, show_macos_invisible: true } } else { visibility };
        let entry = match classify_dir_entry(item, &path, policy, cancelled, rules)? {
            Ok(Some(entry)) => entry,
            Ok(None) => continue,
            Err(kind) => return Some(Err(kind)),
        };
        let navigable = matches!(entry.kind(), EntryKind::Directory | EntryKind::Symlink { points_to_directory: true });
        let package_link = matches!(entry.kind(), EntryKind::Symlink { points_to_directory: true }) && fs::canonicalize(path.join(&own_name)).is_ok_and(|target| (rules.package)(&target));
        if !navigable || ((entry.is_package() || package_link) && !on_path) {
            continue;
        }
        if on_path {
            selected_found = true;
        }
        let index = names.binary_search_by(|name: &EntryName| name.as_bytes().cmp(entry.name().as_bytes())).unwrap_or_else(|index| index);
        names.insert(index, entry.name().clone());
        if names.len() > FOLDER_CHOICE_PAGE_SIZE + 1 {
            names.pop();
        }
    }
    let next = if names.len() > FOLDER_CHOICE_PAGE_SIZE {
        names.pop();
        names.last().cloned()
    } else {
        None
    };
    Some(Ok(FolderChoicePage { names: names.into(), next, selected_found }))
}

/// What a listing leaves out and how it recognizes packages.
pub struct ListingRules<'a> {
    /// Names never listed, such as reserved operation temporaries.
    pub hidden: &'a dyn Fn(&[u8]) -> bool,
    /// Whether a folder is a macOS package.
    pub package: &'a dyn Fn(&Path) -> bool,
    /// Resolves the listing folder's volume-root identity for service names.
    pub volume_root: &'a dyn Fn(&Path) -> io::Result<bool>,
    /// Reads the entry's own native invisibility markers.
    pub invisible: &'a dyn Fn(&Path) -> io::Result<bool>,
}

/// Reads one complete directory snapshot, or returns cancellation or one
/// classified terminal failure. The runtime invokes this only on its worker.
#[cfg(test)]
pub fn read(location: &Location, sort: SortSpec, cancelled: &AtomicBool) -> FolderItemsOutcome {
    read_listing(location, sort, VisibilityPolicy::default(), cancelled, &ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &crate::native_calls::is_volume_root, invisible: &crate::native_calls::is_macos_invisible })
}

/// Reads one complete directory snapshot under `rules`, or returns
/// cancellation or one classified terminal failure. The runtime invokes this
/// only on its worker.
pub fn read_listing(location: &Location, sort: SortSpec, visibility: VisibilityPolicy, cancelled: &AtomicBool, rules: &ListingRules<'_>) -> FolderItemsOutcome {
    if is_cancelled(cancelled) {
        return None;
    }
    let path = path_from_location(location);
    let directory = match fs::read_dir(&path) {
        Ok(directory) => directory,
        Err(error) => return Some(Err(map_error(&error))),
    };
    let entries = match collect_entries(directory, cancelled, |entry, cancelled| classify_dir_entry(entry, &path, visibility, cancelled, rules))? {
        Ok(entries) => entries,
        Err(kind) => return Some(Err(kind)),
    };
    sort_entries_for_read(entries, sort, cancelled).map(Ok)
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

fn classify_dir_entry(entry: DirEntry, directory: &Path, visibility: VisibilityPolicy, cancelled: &AtomicBool, rules: &ListingRules<'_>) -> Option<Result<Option<Entry>, ListingErrorKind>> {
    if is_cancelled(cancelled) {
        return None;
    }
    if (rules.hidden)(entry.file_name().as_bytes()) {
        return Some(Ok(None));
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
    match hidden_by_visibility(entry.file_name().as_bytes(), &entry.path(), directory, own_kind, visibility, rules) {
        Ok(Some(true) | None) => return Some(Ok(None)),
        Ok(Some(false)) => {}
        // One entry's visibility cannot be classified. Leaving it out of this
        // listing is strictly better than failing the whole directory read.
        Err(ListingErrorKind::VisibilityClassificationFailed) => return Some(Ok(None)),
        Err(kind) => return Some(Err(kind)),
    }
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
    let metadata = entry.metadata().ok().map(|metadata| EntryMetadata::new(metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).and_then(|duration| i64::try_from(duration.as_secs()).ok()), Some(metadata.len()))).unwrap_or_default();
    // Packages carry an extension such as `.app`, so only those folders are
    // asked about, which keeps large listings fast.
    let package = kind == EntryKind::Directory && name.as_bytes().iter().skip(1).any(|byte| *byte == b'.') && (rules.package)(&entry.path());
    Some(Ok(Some(Entry::with_metadata(name, kind, metadata).with_package(package))))
}

fn hidden_by_visibility(name: &[u8], path: &Path, directory: &Path, kind: NativeKind, visibility: VisibilityPolicy, rules: &ListingRules<'_>) -> Result<Option<bool>, ListingErrorKind> {
    let macos_named = matches!((name, kind), (b".DS_Store", NativeKind::File) | (b"Icon\r", NativeKind::File) | (b".localized", NativeKind::File) | (b".AppleDouble", NativeKind::Directory)) || (kind == NativeKind::File && name.starts_with(b"._"));
    if macos_named && !visibility.show_macos_items {
        return Ok(Some(true));
    }
    if name.starts_with(b".") && !visibility.show_dotfiles {
        return Ok(Some(true));
    }
    let service_directory = kind == NativeKind::Directory && matches!(name, b".fseventsd" | b".Spotlight-V100" | b".Trashes" | b".TemporaryItems" | b".DocumentRevisions-V100");
    let service_at_volume_root = if service_directory && !visibility.show_macos_items { (rules.volume_root)(directory).map_err(|_| ListingErrorKind::VisibilityClassificationFailed)? } else { false };
    if service_at_volume_root {
        return Ok(Some(true));
    }
    let invisible = if !visibility.show_macos_invisible {
        match (rules.invisible)(path) {
            Ok(invisible) => invisible,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ListingErrorKind::VisibilityClassificationFailed),
        }
    } else {
        false
    };
    Ok(Some(invisible))
}

fn sort_entries_for_read(mut entries: Vec<Entry>, sort: SortSpec, cancelled: &AtomicBool) -> Option<Arc<[Entry]>> {
    if is_cancelled(cancelled) {
        return None;
    }
    sort_entries(&mut entries, sort);
    if is_cancelled(cancelled) {
        return None;
    }
    Some(entries.into())
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
    fn folder_choice_pages_are_bounded_and_keep_a_hidden_confirmed_child() {
        let temporary = tempdir().unwrap();
        for index in 0..260 {
            fs::create_dir(temporary.path().join(format!("folder-{index:03}"))).unwrap();
        }
        fs::create_dir(temporary.path().join(".hidden")).unwrap();
        fs::create_dir(temporary.path().join(".sibling")).unwrap();
        File::create(temporary.path().join("file")).unwrap();
        let parent = location_from_path(temporary.path()).unwrap();
        let selected = EntryName::new(b".hidden".to_vec()).unwrap();
        let cancelled = AtomicBool::new(false);
        let rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|_| Ok(false) };
        let policy = VisibilityPolicy { show_dotfiles: false, ..VisibilityPolicy::default() };
        let first = read_folder_choices(&parent, Some(&selected), None, policy, &cancelled, &rules).unwrap().unwrap();
        assert_eq!(first.names.len(), FOLDER_CHOICE_PAGE_SIZE);
        assert_eq!(first.names[0], selected);
        assert!(!first.names.iter().any(|name| name.as_bytes() == b".sibling"));
        assert!(first.next.is_some());
        let second = read_folder_choices(&parent, Some(&selected), first.next.as_ref(), policy, &cancelled, &rules).unwrap().unwrap();
        assert_eq!(second.names.len(), 5);
        assert!(second.next.is_none());
        assert!(!first.names.iter().chain(second.names.iter()).any(|name| name.as_bytes() == b"file"));
    }

    #[test]
    fn folder_choices_include_folder_links_and_skip_packages() {
        let temporary = tempdir().unwrap();
        fs::create_dir(temporary.path().join("folder")).unwrap();
        fs::create_dir(temporary.path().join("bundle.app")).unwrap();
        symlink("folder", temporary.path().join("link")).unwrap();
        symlink("bundle.app", temporary.path().join("bundle-link")).unwrap();
        symlink("missing", temporary.path().join("dangling")).unwrap();
        let parent = location_from_path(temporary.path()).unwrap();
        let cancelled = AtomicBool::new(false);
        let rules = ListingRules { hidden: &|_| false, package: &|path| path.extension().is_some_and(|extension| extension == "app"), volume_root: &|_| Ok(false), invisible: &|_| Ok(false) };
        let page = read_folder_choices(&parent, None, None, VisibilityPolicy::default(), &cancelled, &rules).unwrap().unwrap();
        let names = page.names.iter().map(|name| name.as_bytes()).collect::<Vec<_>>();
        assert_eq!(names, vec![b"folder".as_slice(), b"link".as_slice()]);
    }

    #[test]
    fn folder_choices_preserve_undecodable_names_and_report_missing_parents() {
        use std::ffi::OsStr;

        let temporary = tempdir().unwrap();
        let raw = b"folder-\xff";
        if let Err(error) = fs::create_dir(temporary.path().join(OsStr::from_bytes(raw))) {
            // APFS can reject arbitrary byte names with EILSEQ.
            if error.raw_os_error() == Some(92) {
                return;
            }
            panic!("could not create undecodable folder name: {error}");
        }
        let parent = location_from_path(temporary.path()).unwrap();
        let cancelled = AtomicBool::new(false);
        let rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|_| Ok(false) };
        let page = read_folder_choices(&parent, None, None, VisibilityPolicy::default(), &cancelled, &rules).unwrap().unwrap();
        assert_eq!(page.names.len(), 1);
        assert_eq!(page.names[0].as_bytes(), raw);

        fs::remove_dir(temporary.path().join(OsStr::from_bytes(raw))).unwrap();
        fs::remove_dir(temporary.path()).unwrap();
        assert!(read_folder_choices(&parent, None, None, VisibilityPolicy::default(), &cancelled, &rules).unwrap().is_err());
    }

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
        let entries = read(&location, SortSpec::default(), &AtomicBool::new(false)).unwrap().unwrap();

        assert!(entries.windows(2).all(|pair| dual_pane_domain::listing_sort_key(&pair[0]) < dual_pane_domain::listing_sort_key(&pair[1])));
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
        assert_eq!(read(&Location::root(), SortSpec::default(), &AtomicBool::new(true)), None);
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
    fn hidden_names_are_left_out_and_packages_are_marked() {
        let temporary = tempdir().unwrap();
        fs::create_dir(temporary.path().join("Tool.app")).unwrap();
        fs::create_dir(temporary.path().join("plain")).unwrap();
        File::create(temporary.path().join(".dual-pane-temporary")).unwrap();
        let location = location_from_path(temporary.path()).unwrap();
        let rules = ListingRules { hidden: &|name| name.starts_with(b".dual-pane-"), package: &|path| path.extension().is_some_and(|extension| extension == "app"), volume_root: &crate::native_calls::is_volume_root, invisible: &crate::native_calls::is_macos_invisible };
        let entries = read_listing(&location, SortSpec::default(), VisibilityPolicy::default(), &AtomicBool::new(false), &rules).unwrap().unwrap();
        let listed = entries.iter().map(|entry| (entry.name().to_text_lossy().into_owned(), entry.is_package())).collect::<Vec<_>>();
        assert_eq!(listed, [("plain".to_owned(), false), ("Tool.app".to_owned(), true)]);
    }

    #[test]
    fn visibility_policy_respects_exact_names_kinds_and_dotfiles() {
        let temporary = tempdir().unwrap();
        File::create(temporary.path().join(".DS_Store")).unwrap();
        File::create(temporary.path().join("._fork")).unwrap();
        File::create(temporary.path().join(".localized")).unwrap();
        fs::create_dir(temporary.path().join(".AppleDouble")).unwrap();
        fs::create_dir(temporary.path().join(".localized-directory")).unwrap();
        fs::create_dir(temporary.path().join(".localized").with_extension("directory")).unwrap();
        File::create(temporary.path().join(".env")).unwrap();
        File::create(temporary.path().join("Icon")).unwrap();
        File::create(temporary.path().join("Icon\r")).unwrap();
        let location = location_from_path(temporary.path()).unwrap();
        let rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &crate::native_calls::is_volume_root, invisible: &crate::native_calls::is_macos_invisible };
        let names = |visibility| read_listing(&location, SortSpec::default(), visibility, &AtomicBool::new(false), &rules).unwrap().unwrap().iter().map(|entry| entry.name().as_bytes().to_vec()).collect::<Vec<_>>();
        let defaults = names(VisibilityPolicy::default());
        assert!(defaults.contains(&b".env".to_vec()));
        assert!(defaults.contains(&b".localized-directory".to_vec()));
        assert!(defaults.contains(&b"Icon".to_vec()));
        assert!(!defaults.contains(&b".DS_Store".to_vec()));
        assert!(!defaults.contains(&b"Icon\r".to_vec()));
        let all = names(VisibilityPolicy { show_macos_items: true, show_dotfiles: true, show_macos_invisible: true });
        assert!(all.contains(&b".DS_Store".to_vec()));
        assert!(all.contains(&b".AppleDouble".to_vec()));
        let no_dotfiles = names(VisibilityPolicy { show_macos_items: true, show_dotfiles: false, show_macos_invisible: true });
        assert!(!no_dotfiles.contains(&b".env".to_vec()));
        assert!(no_dotfiles.contains(&b"Icon".to_vec()));
    }

    #[test]
    fn injected_volume_root_distinguishes_service_folders_and_uncertainty() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join(".Trashes");
        let root_rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(true), invisible: &|_| Ok(false) };
        let nested_rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|_| Ok(false) };
        let uncertain_rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Err(io::Error::other("unknown mount")), invisible: &|_| Ok(false) };
        let default = VisibilityPolicy::default();
        assert_eq!(hidden_by_visibility(b".Trashes", &path, temporary.path(), NativeKind::Directory, default, &root_rules), Ok(Some(true)));
        assert_eq!(hidden_by_visibility(b".Trashes", &path, temporary.path(), NativeKind::Directory, default, &nested_rules), Ok(Some(false)));
        assert_eq!(hidden_by_visibility(b".Trashes", &path, temporary.path(), NativeKind::File, default, &root_rules), Ok(Some(false)));
        assert_eq!(hidden_by_visibility(b".Trashes", &path, temporary.path(), NativeKind::Directory, default, &uncertain_rules), Err(ListingErrorKind::VisibilityClassificationFailed));
        assert_eq!(hidden_by_visibility(b".Trashes", &path, temporary.path(), NativeKind::Directory, VisibilityPolicy { show_macos_items: true, ..default }, &uncertain_rules), Ok(Some(false)));
    }

    #[test]
    fn all_visibility_combinations_require_every_matching_switch() {
        let temporary = tempdir().unwrap();
        for name in [".DS_Store", ".env", "marked", "plain"] {
            File::create(temporary.path().join(name)).unwrap();
        }
        let location = location_from_path(temporary.path()).unwrap();
        let rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|path| Ok(path.file_name().is_some_and(|name| name.as_bytes() == b"marked")) };
        for show_macos_items in [false, true] {
            for show_dotfiles in [false, true] {
                for show_macos_invisible in [false, true] {
                    let policy = VisibilityPolicy { show_macos_items, show_dotfiles, show_macos_invisible };
                    let rows = read_listing(&location, SortSpec::default(), policy, &AtomicBool::new(false), &rules).unwrap().unwrap();
                    let has = |name: &[u8]| rows.iter().any(|entry| entry.name().as_bytes() == name);
                    assert!(has(b"plain"));
                    assert_eq!(has(b".env"), show_dotfiles);
                    assert_eq!(has(b".DS_Store"), show_macos_items && show_dotfiles);
                    assert_eq!(has(b"marked"), show_macos_invisible);
                }
            }
        }
    }

    #[test]
    fn invisible_status_failure_and_disappeared_entry_have_distinct_results() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("plain");
        let missing = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|_| Err(io::Error::from(io::ErrorKind::NotFound)) };
        let unreadable = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|_| Err(io::Error::from(io::ErrorKind::PermissionDenied)) };
        let default = VisibilityPolicy::default();
        assert_eq!(hidden_by_visibility(b"plain", &path, temporary.path(), NativeKind::File, default, &missing), Ok(None));
        assert_eq!(hidden_by_visibility(b"plain", &path, temporary.path(), NativeKind::File, default, &unreadable), Err(ListingErrorKind::VisibilityClassificationFailed));
        assert_eq!(hidden_by_visibility(b"plain", &path, temporary.path(), NativeKind::File, VisibilityPolicy { show_macos_invisible: true, ..default }, &unreadable), Ok(Some(false)));
    }

    #[test]
    fn an_entry_whose_visibility_cannot_be_classified_is_left_out_but_the_listing_still_succeeds() {
        let temporary = tempdir().unwrap();
        File::create(temporary.path().join("plain")).unwrap();
        File::create(temporary.path().join("unreadable")).unwrap();
        let location = location_from_path(temporary.path()).unwrap();
        let rules = ListingRules { hidden: &|_| false, package: &|_| false, volume_root: &|_| Ok(false), invisible: &|path| if path.file_name().is_some_and(|name| name.as_bytes() == b"unreadable") { Err(io::Error::from(io::ErrorKind::PermissionDenied)) } else { Ok(false) } };
        let entries = read_listing(&location, SortSpec::default(), VisibilityPolicy::default(), &AtomicBool::new(false), &rules).unwrap().unwrap();
        let names = entries.iter().map(|entry| entry.name().as_bytes().to_vec()).collect::<Vec<_>>();
        assert_eq!(names, [b"plain".to_vec()]);
    }

    #[test]
    fn missing_and_non_directory_locations_fail_without_rows() {
        let temporary = tempdir().unwrap();
        let missing = location_from_path(&temporary.path().join("missing")).unwrap();
        assert_eq!(read(&missing, SortSpec::default(), &AtomicBool::new(false)), Some(Err(ListingErrorKind::ItemMissing)));

        let file_path = temporary.path().join("file");
        File::create(&file_path).unwrap();
        let file = location_from_path(&file_path).unwrap();
        assert_eq!(read(&file, SortSpec::default(), &AtomicBool::new(false)), Some(Err(ListingErrorKind::NotADirectory)));
    }
}
