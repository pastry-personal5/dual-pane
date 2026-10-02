//! Builds an operation's plan on a worker: the frozen roots, and for Copy,
//! Move, and permanent deletion every entry below them, in contiguous
//! depth-first preorder. Links are recorded as links and never traversed.

use std::collections::BTreeSet;
use std::fs::{self, FileType, Metadata};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dual_pane_application::{OperationErrorKind, OperationFailure, PlannedItem, ScanFailure, ScanSkip};
use dual_pane_domain::{EntryKind, EntryName, Location, OperationIntent, OperationKind};

use crate::native_location::path_from_location;
use crate::operation_journal::{LaunchId, is_launch_temporary};

/// What kind of object an entry is, without following links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeKind {
    Directory,
    File,
    Symlink,
    Other,
}

pub fn native_kind(file_type: FileType) -> NativeKind {
    if file_type.is_symlink() {
        NativeKind::Symlink
    } else if file_type.is_dir() {
        NativeKind::Directory
    } else if file_type.is_file() {
        NativeKind::File
    } else {
        NativeKind::Other
    }
}

fn kind_class(kind: EntryKind) -> NativeKind {
    match kind {
        EntryKind::Directory => NativeKind::Directory,
        EntryKind::File => NativeKind::File,
        EntryKind::Symlink { .. } => NativeKind::Symlink,
        EntryKind::Other => NativeKind::Other,
    }
}

/// What a scan saw of one item, to recognize it again before changing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    pub kind: NativeKind,
    pub size: u64,
    pub modified: (i64, i64),
}

impl Identity {
    /// The identity in no-follow `metadata`.
    pub fn of(metadata: &Metadata) -> Self {
        Self { device: metadata.dev(), inode: metadata.ino(), kind: native_kind(metadata.file_type()), size: metadata.size(), modified: (metadata.mtime(), metadata.mtime_nsec()) }
    }
    /// Whether `current` is still this item: the same device, inode, and
    /// kind, and for a regular file the same size and modification time. A
    /// folder's contents may change, for example while it is merged.
    pub fn matches(&self, current: &Self) -> bool {
        self.device == current.device && self.inode == current.inode && self.kind == current.kind && (self.kind != NativeKind::File || self.size == current.size && self.modified == current.modified)
    }
}

/// A plan and what the scan saw of each entry. A scan-skip placeholder and a
/// New Folder target have no identity.
#[derive(Debug, Clone)]
pub struct ScannedPlan {
    pub plan: Arc<[PlannedItem]>,
    pub identities: Arc<[Option<Identity>]>,
}

/// Maps a native failure to the category the application decides on. Raw
/// errors stay in the driver's log.
pub fn error_kind(error: &io::Error) -> OperationErrorKind {
    match error.raw_os_error() {
        Some(libc::EACCES) => OperationErrorKind::PermissionDenied,
        Some(libc::EPERM) => OperationErrorKind::PrivacyRestricted,
        Some(libc::ENOENT) => OperationErrorKind::NotFound,
        Some(libc::ENOSPC | libc::EDQUOT) => OperationErrorKind::NoSpace,
        Some(libc::EBUSY | libc::ETXTBSY) => OperationErrorKind::Busy,
        Some(libc::EROFS) => OperationErrorKind::ReadOnly,
        Some(libc::ENOTEMPTY) => OperationErrorKind::FolderNotEmpty,
        _ => OperationErrorKind::Other,
    }
}

struct Scanner<'a> {
    intent: &'a OperationIntent,
    skipped: &'a [ScanSkip],
    launch: LaunchId,
    cancelled: &'a AtomicBool,
    plan: Vec<PlannedItem>,
    identities: Vec<Option<Identity>>,
}

/// Scans the frozen roots of `intent`. `skipped` items appear as typed
/// placeholders and are not read. Temporaries of this launch never appear.
/// Returns `None` once cancelled.
pub fn scan(intent: &OperationIntent, skipped: &[ScanSkip], launch: LaunchId, cancelled: &AtomicBool) -> Option<Result<ScannedPlan, ScanFailure>> {
    let mut scanner = Scanner { intent, skipped, launch, cancelled, plan: Vec::new(), identities: Vec::new() };
    let roots: Vec<(Location, EntryKind)> = match intent.kind() {
        OperationKind::NewFolder { name } => vec![(intent.source().join(name), EntryKind::Directory)],
        _ => intent.targets().iter().map(|target| (intent.source().join(&target.name), target.kind)).collect(),
    };
    let recursive = matches!(intent.kind(), OperationKind::Copy | OperationKind::Move | OperationKind::DeletePermanently);
    for (root, kind) in roots {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        if scanner.placeholder(&root) {
            continue;
        }
        if matches!(intent.kind(), OperationKind::NewFolder { .. }) {
            scanner.push(root, kind, None);
            continue;
        }
        let metadata = match fs::symlink_metadata(path_from_location(&root)) {
            Ok(metadata) => metadata,
            Err(error) => return Some(Err(ScanFailure { failure: OperationFailure { item: root, kind: error_kind(&error) }, entry_kind: kind })),
        };
        let identity = Identity::of(&metadata);
        if identity.kind != kind_class(kind) {
            return Some(Err(ScanFailure { failure: OperationFailure { item: root, kind: OperationErrorKind::ChangedSinceScan }, entry_kind: kind }));
        }
        scanner.push(root.clone(), kind, Some(identity));
        if recursive && kind == EntryKind::Directory {
            match scanner.walk(&root)? {
                Ok(()) => {}
                Err(failure) => return Some(Err(failure)),
            }
        }
    }
    Some(Ok(ScannedPlan { plan: scanner.plan.into(), identities: scanner.identities.into() }))
}

impl Scanner<'_> {
    fn destination(&self, source: &Location) -> Option<Location> {
        match self.intent.kind() {
            OperationKind::Copy | OperationKind::Move => self.intent.destination().map(|destination| Location::from_components(destination.components().iter().cloned().chain(source.components()[self.intent.source().components().len()..].iter().cloned()))),
            OperationKind::Rename { to } => Some(self.intent.source().join(to)),
            _ => None,
        }
    }
    fn push(&mut self, source: Location, kind: EntryKind, identity: Option<Identity>) {
        let destination = self.destination(&source);
        self.plan.push(PlannedItem { source, destination, kind });
        self.identities.push(identity);
    }
    /// Adds the placeholder for a skipped `location`, if it is one.
    fn placeholder(&mut self, location: &Location) -> bool {
        let Some(skip) = self.skipped.iter().find(|skip| skip.item == *location) else { return false };
        self.push(location.clone(), skip.kind, None);
        true
    }
    /// Adds every entry below `folder` in sorted byte order. A skipped item
    /// keeps its place even if it has disappeared, so the plan stays valid.
    fn walk(&mut self, folder: &Location) -> Option<Result<(), ScanFailure>> {
        let entries = match fs::read_dir(path_from_location(folder)) {
            Ok(entries) => entries,
            Err(error) => return Some(Err(ScanFailure { failure: OperationFailure { item: folder.clone(), kind: error_kind(&error) }, entry_kind: EntryKind::Directory })),
        };
        let mut names = BTreeSet::new();
        for entry in entries {
            if self.cancelled.load(Ordering::Relaxed) {
                return None;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => return Some(Err(ScanFailure { failure: OperationFailure { item: folder.clone(), kind: error_kind(&error) }, entry_kind: EntryKind::Directory })),
            };
            let bytes = entry.file_name().as_bytes().to_vec();
            if !is_launch_temporary(&bytes, self.launch) {
                names.insert(bytes);
            }
        }
        names.extend(self.skipped.iter().filter(|skip| skip.item.parent().as_ref() == Some(folder)).filter_map(|skip| skip.item.components().last()).map(|name| name.as_bytes().to_vec()));
        for bytes in names {
            if self.cancelled.load(Ordering::Relaxed) {
                return None;
            }
            let Ok(name) = EntryName::new(bytes) else { continue };
            let child = folder.join(&name);
            if self.placeholder(&child) {
                continue;
            }
            let path = path_from_location(&child);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                // It disappeared after the folder was listed.
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Some(Err(ScanFailure { failure: OperationFailure { item: child, kind: error_kind(&error) }, entry_kind: EntryKind::File })),
            };
            let identity = Identity::of(&metadata);
            let kind = match identity.kind {
                NativeKind::Directory => EntryKind::Directory,
                NativeKind::File => EntryKind::File,
                NativeKind::Symlink => EntryKind::Symlink { points_to_directory: fs::metadata(&path).is_ok_and(|target| target.is_dir()) },
                NativeKind::Other => EntryKind::Other,
            };
            self.push(child.clone(), kind, Some(identity));
            if kind == EntryKind::Directory {
                match self.walk(&child)? {
                    Ok(()) => {}
                    Err(failure) => return Some(Err(failure)),
                }
            }
        }
        Some(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    use dual_pane_application::{Command, Event, OperationEffect, WorkRequest, Workspace};
    use dual_pane_domain::{BrowserSide, Entry, OperationTarget};

    use super::*;
    use crate::native_location::location_from_path;
    use crate::operation_journal::temporary_name;

    fn launch() -> LaunchId {
        LaunchId::from_bytes([3; 16])
    }
    fn location(path: &Path) -> Location {
        location_from_path(path).unwrap()
    }
    fn name(text: &str) -> EntryName {
        EntryName::new(text).unwrap()
    }
    fn intent(kind: OperationKind, source: &Path, targets: &[(&str, EntryKind)], destination: Option<&Path>) -> OperationIntent {
        OperationIntent::new(kind, location(source), targets.iter().map(|(text, kind)| OperationTarget { name: name(text), kind: *kind }).collect(), destination.map(location)).unwrap()
    }
    fn plan(scanned: &ScannedPlan) -> Vec<(String, EntryKind)> {
        scanned.plan.iter().map(|item| (item.source.components().last().unwrap().to_text_lossy().into_owned(), item.kind)).collect()
    }
    /// A workspace showing `source` on the left and `destination` on the
    /// right, with `targets` selected, so a real `Workspace` validates plans.
    fn workspace(source: &Path, entries: &[(&str, EntryKind)], destination: &Path) -> Workspace {
        let mut ws = Workspace::new();
        for (browser, path, rows) in [(BrowserSide::Left, source, entries), (BrowserSide::Right, destination, &[][..])] {
            let read = ws.handle(Command::Navigate { browser, location: location(path) }.into());
            let Some(WorkRequest::ReadDirectory { tab, token, .. }) = read.work.into_iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) else { panic!() };
            ws.handle(Event::FolderItemsLoaded { browser, tab, token, entries: rows.iter().map(|(text, kind)| Entry::new(name(text), *kind)).collect(), changes: None }.into());
        }
        let tab = ws.active_tab(BrowserSide::Left);
        for (row, (text, _)) in entries.iter().enumerate() {
            ws.handle(if row == 0 { Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: name(text) } } else { Command::ToggleEntry { browser: BrowserSide::Left, tab, row, name: name(text) } }.into());
        }
        ws
    }
    /// Starts `kind` in `ws`, scans natively, and returns what the
    /// application did with the result.
    fn scan_through(ws: &mut Workspace, kind: OperationKind) -> (ScannedPlan, Vec<WorkRequest>) {
        let tab = ws.active_tab(BrowserSide::Left);
        let started = ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind }.into());
        let Some(WorkRequest::Operation(OperationEffect::Scan { id, generation, intent, skipped })) = started.work.into_iter().next() else { panic!("a scan") };
        let scanned = scan(&intent, &skipped, launch(), &AtomicBool::new(false)).unwrap().unwrap();
        let work = ws.handle(Event::OperationScanned { id, generation, plan: Ok(Arc::clone(&scanned.plan)) }.into()).work;
        (scanned, work)
    }

    #[test]
    fn a_copy_scan_lists_the_tree_in_preorder_with_links_as_links_and_is_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let (source, destination) = (directory.path().join("from"), directory.path().join("to"));
        fs::create_dir_all(source.join("dir/sub")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("dir/b"), b"b").unwrap();
        fs::write(source.join("dir/sub/a"), b"a").unwrap();
        symlink("sub", source.join("dir/link")).unwrap();
        fs::write(source.join("dir").join(temporary_name(launch(), 1, 0)), b"partial").unwrap();
        let mut ws = workspace(&source, &[("dir", EntryKind::Directory)], &destination);
        let (scanned, work) = scan_through(&mut ws, OperationKind::Copy);
        assert_eq!(plan(&scanned), vec![("dir".to_owned(), EntryKind::Directory), ("b".to_owned(), EntryKind::File), ("link".to_owned(), EntryKind::Symlink { points_to_directory: true }), ("sub".to_owned(), EntryKind::Directory), ("a".to_owned(), EntryKind::File)]);
        assert!(scanned.identities.iter().all(Option::is_some));
        assert!(matches!(work.as_slice(), [WorkRequest::Operation(OperationEffect::Execute { at: 0, .. })]), "the application accepts the plan: {work:?}");
    }

    #[test]
    fn trash_and_rename_scan_roots_only() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("from");
        fs::create_dir_all(source.join("dir/inner")).unwrap();
        let mut ws = workspace(&source, &[("dir", EntryKind::Directory)], directory.path());
        let (scanned, work) = scan_through(&mut ws, OperationKind::MoveToTrash);
        assert_eq!(plan(&scanned), vec![("dir".to_owned(), EntryKind::Directory)]);
        assert!(!work.is_empty());
        let mut ws = workspace(&source, &[("dir", EntryKind::Directory)], directory.path());
        let (scanned, work) = scan_through(&mut ws, OperationKind::Rename { to: name("renamed") });
        assert_eq!(scanned.plan[0].destination, Some(location(&source.join("renamed"))));
        assert!(!work.is_empty());
    }

    #[test]
    fn an_unreadable_folder_becomes_a_decision_and_skip_keeps_a_placeholder() {
        let directory = tempfile::tempdir().unwrap();
        let (source, destination) = (directory.path().join("from"), directory.path().join("to"));
        fs::create_dir_all(source.join("dir/locked/hidden")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("dir/z"), b"z").unwrap();
        fs::set_permissions(source.join("dir/locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let mut ws = workspace(&source, &[("dir", EntryKind::Directory)], &destination);
        let tab = ws.active_tab(BrowserSide::Left);
        let started = ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Copy }.into());
        let Some(WorkRequest::Operation(OperationEffect::Scan { id, generation, intent, skipped })) = started.work.into_iter().next() else { panic!() };
        let failed = scan(&intent, &skipped, launch(), &AtomicBool::new(false)).unwrap();
        let Err(failure) = &failed else { panic!("{failed:?}") };
        assert_eq!((failure.failure.kind, failure.failure.item.clone()), (OperationErrorKind::PermissionDenied, location(&source.join("dir/locked"))));
        ws.handle(Event::OperationScanned { id, generation, plan: failed.map(|scanned| scanned.plan) }.into());
        let decision = ws.operation_jobs()[0].decision().map(|decision| (decision.token, decision.item.clone())).unwrap();
        let rescan = ws.handle(Command::DecideOperation { id, token: decision.0, item: decision.1, choice: dual_pane_domain::OperationChoice::Skip, apply_to_all: false }.into());
        let Some(WorkRequest::Operation(OperationEffect::Scan { generation, intent, skipped, .. })) = rescan.work.into_iter().next() else { panic!() };
        let scanned = scan(&intent, &skipped, launch(), &AtomicBool::new(false)).unwrap().unwrap();
        fs::set_permissions(source.join("dir/locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(plan(&scanned), vec![("dir".to_owned(), EntryKind::Directory), ("locked".to_owned(), EntryKind::Directory), ("z".to_owned(), EntryKind::File)]);
        assert_eq!(scanned.identities[1], None, "the placeholder was not read");
        let work = ws.handle(Event::OperationScanned { id, generation, plan: Ok(scanned.plan) }.into()).work;
        assert!(matches!(work.as_slice(), [WorkRequest::Operation(OperationEffect::Execute { .. })]), "{work:?}");
    }

    #[test]
    fn a_missing_or_changed_root_is_a_recoverable_scan_failure() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("was-a-file")).unwrap();
        let missing = intent(OperationKind::MoveToTrash, directory.path(), &[("gone", EntryKind::File)], None);
        let Some(Err(failure)) = scan(&missing, &[], launch(), &AtomicBool::new(false)) else { panic!() };
        assert_eq!(failure.failure.kind, OperationErrorKind::NotFound);
        let changed = intent(OperationKind::MoveToTrash, directory.path(), &[("was-a-file", EntryKind::File)], None);
        let Some(Err(failure)) = scan(&changed, &[], launch(), &AtomicBool::new(false)) else { panic!() };
        assert_eq!((failure.failure.kind, failure.entry_kind), (OperationErrorKind::ChangedSinceScan, EntryKind::File));
        assert!(scan(&changed, &[], launch(), &AtomicBool::new(true)).is_none(), "a cancelled scan reports nothing");
    }

    #[test]
    fn identities_tell_a_replaced_item_from_a_merged_folder() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("file");
        fs::write(&file, b"one").unwrap();
        let before = Identity::of(&fs::symlink_metadata(&file).unwrap());
        fs::write(&file, b"longer").unwrap();
        assert!(!before.matches(&Identity::of(&fs::symlink_metadata(&file).unwrap())), "a rewritten file changed");
        let folder = directory.path().join("folder");
        fs::create_dir(&folder).unwrap();
        let before = Identity::of(&fs::symlink_metadata(&folder).unwrap());
        fs::write(folder.join("new"), b"").unwrap();
        assert!(before.matches(&Identity::of(&fs::symlink_metadata(&folder).unwrap())), "a folder's contents may change");
    }

    #[test]
    fn native_errors_map_to_the_application_categories() {
        let kind = |errno| error_kind(&io::Error::from_raw_os_error(errno));
        assert_eq!(kind(libc::EACCES), OperationErrorKind::PermissionDenied);
        assert_eq!(kind(libc::EPERM), OperationErrorKind::PrivacyRestricted);
        assert_eq!(kind(libc::ENOENT), OperationErrorKind::NotFound);
        assert_eq!(kind(libc::ENOSPC), OperationErrorKind::NoSpace);
        assert_eq!(kind(libc::EDQUOT), OperationErrorKind::NoSpace);
        assert_eq!(kind(libc::EBUSY), OperationErrorKind::Busy);
        assert_eq!(kind(libc::EROFS), OperationErrorKind::ReadOnly);
        assert_eq!(kind(libc::ENOTEMPTY), OperationErrorKind::FolderNotEmpty);
        assert_eq!(kind(libc::EIO), OperationErrorKind::Other);
    }
}
