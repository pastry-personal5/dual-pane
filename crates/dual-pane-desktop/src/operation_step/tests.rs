use std::collections::VecDeque;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::Mutex;

use dual_pane_application::{Command, Event, OperationEffect, OperationOutcome, OperationStatus, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, EntryName};

use super::*;
use crate::operation_journal::{JOURNAL_FILE_NAME, LaunchId};
use crate::operation_lane::{SerialLane, Services};

type CopyHook = Box<dyn Fn(&Path) + Send>;

/// The real file system with injectable faults and a pretend second
/// volume.
#[derive(Default)]
struct TestFs {
    /// Folders at or below this path report another volume.
    other_volume: Mutex<Option<PathBuf>>,
    /// Copies into these folders write a little, then fail with this error.
    failing_copies: Mutex<Vec<(PathBuf, i32)>>,
    /// Removing these files or folders fails with `EACCES` while listed.
    failing_removals: Mutex<Vec<PathBuf>>,
    /// A copy runs this first, for example to cancel.
    before_copy: Mutex<Option<CopyHook>>,
    exclusive_renames: Mutex<usize>,
    /// An exclusive rename first creates its destination, as another
    /// process racing the copy would.
    racing_destination: Mutex<Option<PathBuf>>,
    trash: Mutex<Option<PathBuf>>,
}

impl TestFs {
    /// Runs the copy hook, then writes a partial copy and fails when
    /// `to` is in a failing folder.
    fn before_writing(&self, to: &Path) -> io::Result<()> {
        if let Some(hook) = self.before_copy.lock().unwrap().as_ref() {
            hook(to);
        }
        if let Some((_, errno)) = self.failing_copies.lock().unwrap().iter().find(|(folder, _)| to.starts_with(folder)) {
            fs::write(to, b"part")?;
            return Err(io::Error::from_raw_os_error(*errno));
        }
        Ok(())
    }
}

impl FileSystem for TestFs {
    fn rename_exclusive(&self, from: &Path, to: &Path) -> io::Result<()> {
        *self.exclusive_renames.lock().unwrap() += 1;
        if let Some(racing) = self.racing_destination.lock().unwrap().take_if(|racing| racing == to) {
            fs::write(&racing, b"racer")?;
        }
        native_calls::rename_exclusive(from, to)
    }
    fn clone_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.before_writing(to)?;
        native_calls::clone_file(from, to)
    }
    fn copy_file(&self, from: &Path, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<bool> {
        self.before_writing(to)?;
        native_calls::copy_file(from, to, progress)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        if self.failing_removals.lock().unwrap().iter().any(|failing| failing == path) {
            return Err(io::Error::from_raw_os_error(libc::EACCES));
        }
        fs::remove_file(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        if self.failing_removals.lock().unwrap().iter().any(|failing| failing == path) {
            return Err(io::Error::from_raw_os_error(libc::EACCES));
        }
        fs::remove_dir(path)
    }
    fn device(&self, folder: &Path) -> io::Result<u64> {
        let device = fs::metadata(folder)?.dev();
        Ok(if self.other_volume.lock().unwrap().as_ref().is_some_and(|volume| folder.starts_with(volume)) { device.wrapping_add(1) } else { device })
    }
    fn move_to_trash(&self, path: &Path) -> io::Result<PathBuf> {
        let Some(trash) = self.trash.lock().unwrap().clone() else { return Err(io::Error::other("no Trash")) };
        let target = trash.join(path.file_name().ok_or_else(|| io::Error::other("no name"))?);
        fs::rename(path, &target)?;
        Ok(target)
    }
}

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}

/// A temporary root with `from` and `to`, a workspace showing them, and a
/// serial lane over a test file system and a journal in the root.
struct Run {
    root: tempfile::TempDir,
    ws: Workspace,
    lane: SerialLane,
    fs: Arc<TestFs>,
    events: Vec<Event>,
}

impl Run {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("from")).unwrap();
        fs::create_dir(root.path().join("to")).unwrap();
        let fs = Arc::new(TestFs::default());
        let journal = Journal::new(Some(root.path().join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
        assert!(journal.reopen());
        let lane = SerialLane::new(Services { fs: Arc::clone(&fs) as Arc<dyn FileSystem>, journal, budget: Duration::from_secs(60) });
        Self { root, ws: Workspace::new(), lane, fs, events: Vec::new() }
    }
    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }
    fn location(&self, relative: &str) -> Location {
        location_from_path(&self.path(relative)).unwrap()
    }
    /// Shows `from` on the left and `to` on the right, freshly listed.
    fn show(&mut self) {
        for (browser, folder) in [(BrowserSide::Left, "from"), (BrowserSide::Right, "to")] {
            let location = self.location(folder);
            let work = self.ws.handle(Command::Navigate { browser, location }.into()).work;
            self.drive(work);
        }
    }
    /// Selects `names` on the left and starts `kind`.
    fn start(&mut self, names: &[&str], kind: OperationKind) -> OperationId {
        self.show();
        let tab = self.ws.active_tab(BrowserSide::Left);
        let entries = self.ws.entries(BrowserSide::Left).to_vec();
        for (index, text) in names.iter().enumerate() {
            let row = entries.iter().position(|entry| entry.name() == &name(text)).unwrap_or_else(|| panic!("{text} is listed"));
            let command = if index == 0 { Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: name(text) } } else { Command::ToggleEntry { browser: BrowserSide::Left, tab, row, name: name(text) } };
            self.ws.handle(command.into());
        }
        let started = self.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind }.into());
        let id = self.ws.operation_jobs().last().unwrap().id();
        assert!(matches!(started.outputs.first(), Some(dual_pane_application::Output::OperationChanged { .. })), "{:?}", started.outputs);
        self.drive(started.work);
        id
    }
    /// Carries out work until none is left: listings through the real
    /// reader, operation effects through the lane.
    fn drive(&mut self, work: Vec<WorkRequest>) {
        let mut work = VecDeque::from(work);
        while let Some(request) = work.pop_front() {
            let events = match request {
                WorkRequest::Operation(effect) => self.lane.run(effect),
                WorkRequest::ReadDirectory { browser, tab, token, location, sort, .. } => {
                    let entries = crate::folder_items::read(&location, sort, &AtomicBool::new(false)).unwrap();
                    vec![match entries {
                        Ok(entries) => Event::FolderItemsLoaded { browser, tab, token, entries, changes: None },
                        Err(kind) => Event::FolderItemsFailed { browser, tab, token, kind },
                    }]
                }
                _ => vec![],
            };
            for event in events {
                self.events.push(event.clone());
                work.extend(self.ws.handle(event.into()).work);
            }
        }
    }
    fn status(&self, id: OperationId) -> OperationStatus {
        self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().status().clone()
    }
    fn decide(&mut self, id: OperationId, choice: OperationChoice, apply_to_all: bool) {
        let job = self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap();
        let decision = job.decision().unwrap_or_else(|| panic!("waiting: {:?}", job.status()));
        let (token, item) = (decision.token, decision.item.clone());
        let work = self.ws.handle(Command::DecideOperation { id, token, item, choice, apply_to_all }.into()).work;
        assert!(!work.is_empty(), "{choice:?} was accepted");
        self.drive(work);
    }
    fn waiting(&self, id: OperationId) -> (OperationIssue, Option<OperationErrorKind>) {
        let job = self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap();
        let decision = job.decision().unwrap_or_else(|| panic!("waiting: {:?}", job.status()));
        (decision.issue, decision.failure.map(|failure| failure.kind))
    }
    fn write(&self, relative: &str, content: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
    }
    fn temporaries(&self) -> Vec<String> {
        walk(self.root.path()).into_iter().filter(|name| name.contains(".dual-pane-")).collect()
    }
}

fn walk(folder: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(folder).unwrap().flatten() {
        names.push(entry.file_name().to_string_lossy().into_owned());
        if entry.file_type().unwrap().is_dir() {
            names.extend(walk(&entry.path()));
        }
    }
    names
}

#[test]
fn a_copy_merges_folders_keeps_metadata_and_never_clobbers() {
    let mut run = Run::new();
    run.write("from/dir/keep.txt", "new");
    run.write("from/dir/sub/fresh.txt", "fresh");
    run.write("to/dir/keep.txt", "old");
    fs::set_permissions(run.path("from/dir/sub"), fs::Permissions::from_mode(0o750)).unwrap();
    fs::set_permissions(run.path("from/dir/sub/fresh.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    symlink("sub", run.path("from/dir/link")).unwrap();
    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id), (OperationIssue::FileConflict, None), "the existing folder merges; its file conflicts");
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/keep.txt"), "old", "a skipped conflict keeps the destination");
    assert_eq!(run.read("to/dir/sub/fresh.txt"), "fresh");
    assert_eq!(fs::metadata(run.path("to/dir/sub")).unwrap().permissions().mode() & 0o777, 0o750, "a created folder gets its source's metadata");
    assert_eq!(fs::metadata(run.path("to/dir/sub/fresh.txt")).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(fs::symlink_metadata(run.path("to/dir/link")).unwrap().file_type().is_symlink(), "a link is copied as a link");
    assert_eq!(run.read("from/dir/keep.txt"), "new", "the source is untouched");
    assert!(run.temporaries().is_empty());

    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    run.decide(id, OperationChoice::Replace, true);
    assert_eq!(run.waiting(id).0, OperationIssue::LinkCollision, "a link is never replaced");
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/keep.txt"), "new", "Replace swaps in the complete copy");
    assert!(run.temporaries().is_empty(), "no temporary is left over");
}

#[test]
fn an_unlisted_destination_child_still_requires_a_conflict_decision() {
    let mut run = Run::new();
    run.write("from/dir/.DS_Store", "new");
    run.write("to/dir/.DS_Store", "old");
    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.read("to/dir/.DS_Store"), "old");
}

#[test]
fn a_kind_mismatch_replaces_neither_side() {
    let mut run = Run::new();
    run.write("from/clash/inner.txt", "inner");
    run.write("to/clash", "a file");
    run.write("from/file.txt", "file");
    fs::create_dir(run.path("to/file.txt")).unwrap();
    let id = run.start(&["clash", "file.txt"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::KindMismatch);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.waiting(id).0, OperationIssue::KindMismatch);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/clash"), "a file");
    assert!(run.path("to/file.txt").is_dir());
}

#[test]
fn a_failed_replacement_leaves_the_destination_and_removes_only_its_temporary() {
    let mut run = Run::new();
    run.write("from/a.txt", "new content");
    run.write("to/a.txt", "old content");
    let foreign = run.root.path().join("to").join(crate::operation_journal::temporary_name(LaunchId::from_bytes([1; 16]), 1, 0));
    fs::write(&foreign, b"another launch").unwrap();
    let id = run.start(&["a.txt"], OperationKind::Copy);
    run.fs.failing_copies.lock().unwrap().push((run.path("to"), libc::ENOSPC));
    run.decide(id, OperationChoice::Replace, false);
    assert_eq!(run.waiting(id), (OperationIssue::RecoverableError, Some(OperationErrorKind::NoSpace)));
    assert_eq!(run.read("to/a.txt"), "old content", "the destination is untouched until the data is complete");
    assert_eq!(run.temporaries(), vec![foreign.file_name().unwrap().to_string_lossy().into_owned()], "only this operation's temporary is removed");
    run.fs.failing_copies.lock().unwrap().clear();
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict, "trying again rediscovers the conflict");
    run.decide(id, OperationChoice::Replace, false);
    assert_eq!(run.read("to/a.txt"), "new content");
}

#[test]
fn the_journal_records_a_folder_before_its_first_temporary_and_forgets_it_after() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    let journal = run.lane.services.journal.clone();
    let folder = run.path("to");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&seen);
    *run.fs.before_copy.lock().unwrap() = Some(Box::new(move |_| observed.lock().unwrap().push(journal.folders(1))));
    let id = run.start(&["a.txt"], OperationKind::Copy);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert_eq!(seen.lock().unwrap().as_slice(), &[vec![folder]], "recorded before the copy wrote its temporary");
    assert!(run.lane.services.journal.folders(id.get()).is_empty(), "cleared once nothing is left");
}

#[test]
fn a_same_volume_move_renames_a_whole_folder_once() {
    let mut run = Run::new();
    run.write("from/dir/a/x.txt", "x");
    run.write("from/dir/b.txt", "b");
    let id = run.start(&["dir"], OperationKind::Move);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert_eq!(*run.fs.exclusive_renames.lock().unwrap(), 1);
    assert_eq!(run.ws.operation_jobs()[0].progress().completed, 4);
    assert_eq!(run.read("to/dir/a/x.txt"), "x");
    assert!(!run.path("from/dir").exists(), "finalization finds the folder already gone");
}

#[test]
fn a_merging_move_moves_entries_one_by_one_and_keeps_folders_with_skipped_content() {
    let mut run = Run::new();
    run.write("from/dir/a/x.txt", "new x");
    run.write("from/dir/b/y.txt", "y");
    run.write("to/dir/a/x.txt", "old x");
    let id = run.start(&["dir"], OperationKind::Move);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/b/y.txt"), "y");
    assert!(!run.path("from/dir/b").exists(), "an emptied folder is removed");
    assert_eq!(run.read("from/dir/a/x.txt"), "new x", "the skipped file and the folders above it stay");
    assert_eq!(run.read("to/dir/a/x.txt"), "old x");
}

#[test]
fn a_cross_volume_move_copies_first_and_removes_each_source_after() {
    let mut run = Run::new();
    run.write("from/dir/a.txt", "a");
    run.write("from/b.txt", "b");
    *run.fs.other_volume.lock().unwrap() = Some(run.path("to"));
    run.fs.failing_removals.lock().unwrap().push(run.path("from/b.txt"));
    let id = run.start(&["dir", "b.txt"], OperationKind::Move);
    assert_eq!(*run.fs.exclusive_renames.lock().unwrap(), 2, "each file is placed from a temporary, never renamed across volumes");
    assert_eq!(run.waiting(id), (OperationIssue::RecoverableError, Some(OperationErrorKind::SourceRemovalFailed)));
    assert_eq!((run.read("to/b.txt"), run.read("from/b.txt")), ("b".to_owned(), "b".to_owned()), "the copy is kept with its source");
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::SourceRemovalFailed), "Try Again retries only the removal");
    run.fs.failing_removals.lock().unwrap().clear();
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(!run.path("from/b.txt").exists() && !run.path("from/dir").exists());
    assert_eq!(run.read("to/dir/a.txt"), "a");
    assert!(run.temporaries().is_empty());

    run.write("from/c.txt", "c");
    run.fs.failing_removals.lock().unwrap().push(run.path("from/c.txt"));
    let id = run.start(&["c.txt"], OperationKind::Move);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!((run.read("to/c.txt"), run.read("from/c.txt")), ("c".to_owned(), "c".to_owned()), "Skip keeps both copies");
}

#[test]
fn a_cross_volume_move_without_a_journal_stops_at_the_item_until_it_reopens() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    *run.fs.other_volume.lock().unwrap() = Some(run.path("to"));
    let blocked = run.path("blocked");
    fs::write(&blocked, b"").unwrap();
    run.lane.services.journal = Journal::new(Some(blocked.join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
    let id = run.start(&["a.txt"], OperationKind::Move);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::JournalUnavailable));
    assert!(run.temporaries().is_empty() && run.path("from/a.txt").exists());
    run.lane.services.journal = Journal::new(Some(run.path(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
    run.decide(id, OperationChoice::TryAgain, false);
    assert!(run.events.contains(&Event::JournalStatus { available: true }), "Try Again reopens the journal");
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
}

#[test]
fn rename_refuses_collisions_and_hard_links_but_allows_a_case_only_change() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.write("from/b.txt", "b");
    let id = run.start(&["a.txt"], OperationKind::Rename { to: name("b.txt") });
    assert_eq!(run.status(id), OperationStatus::NameCollision { item: run.location("from/a.txt") });
    assert_eq!(run.read("from/b.txt"), "b");
    run.ws.handle(Command::CancelOperation { id }.into());

    fs::hard_link(run.path("from/a.txt"), run.path("from/link.txt")).unwrap();
    let id = run.start(&["a.txt"], OperationKind::Rename { to: name("link.txt") });
    assert_eq!(run.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().name_problem(), Some(NameProblem::Collision), "a hard link to the same file is still another item");
    run.ws.handle(Command::CancelOperation { id }.into());
    fs::remove_file(run.path("from/link.txt")).unwrap();

    let id = run.start(&["a.txt"], OperationKind::Rename { to: name("A.txt") });
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    let names = fs::read_dir(run.path("from")).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect::<HashSet<_>>();
    assert!(names.contains("A.txt") && !names.contains("a.txt"), "{names:?}");

    let long = "n".repeat(300);
    let id = run.start(&["A.txt"], OperationKind::Rename { to: name(&long) });
    assert_eq!(run.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().name_problem(), Some(NameProblem::TooLong));
}

#[test]
fn new_folder_creates_or_reports_a_name_problem() {
    let mut run = Run::new();
    run.write("from/taken/inner", "x");
    let id = run.start(&[], OperationKind::NewFolder { name: name("made") });
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(run.path("from/made").is_dir());
    let id = run.start(&[], OperationKind::NewFolder { name: name("taken") });
    assert_eq!(run.status(id), OperationStatus::NameCollision { item: run.location("from/taken") });
    assert!(run.path("from/taken/inner").exists());
}

#[test]
fn a_destination_inside_the_source_through_a_link_is_refused() {
    let mut run = Run::new();
    fs::create_dir_all(run.path("from/dir/inner")).unwrap();
    fs::remove_dir(run.path("to")).unwrap();
    symlink(run.path("from/dir/inner"), run.path("to")).unwrap();
    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::DestinationWithinSource));
    assert!(fs::read_dir(run.path("from/dir/inner")).unwrap().next().is_none(), "nothing was copied into itself");
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
}

#[test]
fn an_item_changed_since_the_scan_is_not_touched() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.show();
    let tab = run.ws.active_tab(BrowserSide::Left);
    run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("a.txt") }.into());
    let started = run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::DeletePermanently }.into());
    assert!(started.work.is_empty());
    let id = run.ws.operation_jobs()[0].id();
    let confirmed = run.ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work;
    let [WorkRequest::Operation(scan @ OperationEffect::Scan { .. })] = confirmed.as_slice() else { panic!() };
    let scanned = run.lane.run(scan.clone());
    let mut work = Vec::new();
    for event in scanned {
        work.extend(run.ws.handle(event.into()).work);
    }
    run.write("from/a.txt", "rewritten after the scan");
    run.drive(work);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::ChangedSinceScan));
    assert_eq!(run.read("from/a.txt"), "rewritten after the scan");
}

#[test]
fn permanent_deletion_unlinks_files_then_removes_folders_deepest_first() {
    let mut run = Run::new();
    run.write("from/dir/a/x.txt", "x");
    run.write("from/dir/b.txt", "b");
    symlink(run.path("to"), run.path("from/dir/outside")).unwrap();
    run.write("to/kept.txt", "kept");
    run.fs.failing_removals.lock().unwrap().push(run.path("from/dir/a"));
    run.show();
    let tab = run.ws.active_tab(BrowserSide::Left);
    run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("dir") }.into());
    run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::DeletePermanently }.into());
    let id = run.ws.operation_jobs()[0].id();
    let work = run.ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work;
    run.drive(work);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::PermissionDenied), "a folder that cannot be removed asks");
    assert!(!run.path("from/dir/a/x.txt").exists() && fs::symlink_metadata(run.path("from/dir/outside")).is_err());
    assert_eq!(run.read("to/kept.txt"), "kept", "a link is removed, never followed");
    run.fs.failing_removals.lock().unwrap().clear();
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(!run.path("from/dir").exists());
}

#[test]
fn trash_moves_each_root_or_reports_trash_unavailable_without_deleting() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    let id = run.start(&["a.txt"], OperationKind::MoveToTrash);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::TrashUnavailable));
    assert_eq!(run.read("from/a.txt"), "a", "never deleted instead");
    let trash = run.path("Trash");
    fs::create_dir(&trash).unwrap();
    *run.fs.trash.lock().unwrap() = Some(trash.clone());
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(trash.join("a.txt").exists());
    assert!(run.events.contains(&Event::LocationInvalidated { location: location_from_path(&trash).unwrap() }));
}

#[test]
fn cancelling_mid_copy_keeps_completed_work_and_removes_only_its_temporaries() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    fs::write(run.path("from/large.bin"), vec![1_u8; 4 * 1024 * 1024]).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    run.lane.cancelled.insert(OperationId::new(1), Arc::clone(&cancelled));
    let flag = Arc::clone(&cancelled);
    *run.fs.before_copy.lock().unwrap() = Some(Box::new(move |to| {
        if !to.to_string_lossy().contains("-1-0.") {
            flag.store(true, Ordering::Relaxed);
        }
    }));
    let id = run.start(&["a.txt", "large.bin"], OperationKind::Copy);
    assert_eq!(id, OperationId::new(1));
    assert_eq!(run.status(id), OperationStatus::Running { at: 1 }, "the step reported the copy it completed, and the next one stopped at once");
    assert_eq!(run.read("to/a.txt"), "a");
    assert!(run.temporaries().is_empty(), "the interrupted copy discards its temporary at once");
    let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
    run.drive(work);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert!(run.temporaries().is_empty());
    assert!(!run.path("to/large.bin").exists());
    assert_eq!(run.read("to/a.txt"), "a", "completed work stays");
}

#[test]
fn a_cancel_with_nothing_in_flight_still_reports_its_cleanup() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.write("to/a.txt", "old");
    let id = run.start(&["a.txt"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
    run.drive(work);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Cancelled));
    let mut lost = Run::new();
    assert_eq!(lost.lane.run(OperationEffect::Cancel { id: OperationId::new(42), generation: 3 }), vec![Event::OperationCleaned { id: OperationId::new(42), generation: 3, result: CleanupResult::Clean }], "a cancel with no state still reports");
}

#[test]
fn cleanup_removes_a_temporary_the_step_could_not_and_reports_what_it_cannot() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.write("to/a.txt", "old");
    let id = run.start(&["a.txt"], OperationKind::Copy);
    // A temporary that this operation recorded but could not discard.
    let leftover = run.path("to").join(crate::operation_journal::temporary_name(LaunchId::from_bytes([9; 16]), id.get(), 7));
    fs::write(&leftover, b"partial").unwrap();
    run.lane.services.journal.record(id.get(), &run.path("to"), fs::metadata(run.path("to")).unwrap().dev()).unwrap();
    run.fs.failing_removals.lock().unwrap().push(leftover.clone());
    let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
    run.drive(work);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::CleanupUncertain), "a temporary that cannot be shown removed is reported");
    assert_eq!(run.read("to/a.txt"), "old");
}

#[test]
fn the_write_primitive_decides_a_conflict_the_fast_path_missed() {
    let mut run = Run::new();
    run.write("from/a.txt", "mine");
    *run.fs.racing_destination.lock().unwrap() = Some(run.path("to/a.txt"));
    let id = run.start(&["a.txt"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    assert_eq!(run.read("to/a.txt"), "racer", "the item that appeared first is kept");
    assert!(run.temporaries().is_empty());
}

#[test]
fn a_moved_folder_holding_skipped_content_moves_entry_by_entry() {
    let mut run = Run::new();
    run.write("from/dir/a.txt", "a");
    run.write("from/dir/locked/secret.txt", "secret");
    fs::set_permissions(run.path("from/dir/locked"), fs::Permissions::from_mode(0o000)).unwrap();
    let id = run.start(&["dir"], OperationKind::Move);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::PermissionDenied), "the unreadable folder asks during the scan");
    run.decide(id, OperationChoice::Skip, false);
    fs::set_permissions(run.path("from/dir/locked"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/a.txt"), "a");
    assert_eq!(run.read("from/dir/locked/secret.txt"), "secret", "the skipped content and the folder above it stay");
    assert!(!run.path("to/dir/locked").exists());
}

#[test]
fn a_destination_inside_the_source_by_another_spelling_is_refused() {
    let mut run = Run::new();
    fs::create_dir_all(run.path("from/dir/inner")).unwrap();
    let variant = run.path("FROM/DIR/inner");
    if !variant.is_dir() {
        // A case-sensitive volume has no second spelling to test.
        return;
    }
    fs::remove_dir(run.path("to")).unwrap();
    run.show();
    let destination = location_from_path(&variant).unwrap();
    let read = run.ws.handle(Command::Navigate { browser: BrowserSide::Right, location: destination }.into()).work;
    run.drive(read);
    let tab = run.ws.active_tab(BrowserSide::Left);
    run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("dir") }.into());
    let started = run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Copy }.into());
    let id = run.ws.operation_jobs()[0].id();
    run.drive(started.work);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::DestinationWithinSource), "device and inode catch what name comparison cannot");
}
