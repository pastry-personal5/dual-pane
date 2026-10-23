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

mod cleanup;
mod copy_and_move;
mod safety;
