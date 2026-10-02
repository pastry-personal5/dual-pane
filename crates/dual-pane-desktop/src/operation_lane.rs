//! The file-operation lane: two workers that scan, execute, finalize, and
//! clean up operations. Requests of one operation run in order, and a
//! cleanup runs only after that operation's in-flight request has stopped.
//! Runtime-owned leases serialize requests whose write scopes overlap. A
//! worker holds no lease while anyone decides anything, never waits for a
//! person, and contains a panic without replaying the request.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use dual_pane_application::{ByteProgress, Event, FinalizeResult, OperationEffect, OperationProgress, PlannedItem, ScanSkip, StepResult};
use dual_pane_domain::{Location, OperationChoice, OperationId, OperationIntent, OperationKind, starts_with};

use crate::operation_journal::Journal;
use crate::operation_scan::scan;
use crate::operation_step::{FileSystem, OperationState, Outcome, StepContext, clean_up_after_loss};

/// Workers in the lane.
pub const OPERATION_WORKERS: usize = 2;
/// How long one step works before it reports progress and yields.
pub const STEP_BUDGET: Duration = Duration::from_millis(200);
/// Requests that may wait before admission fails.
const PENDING_CAPACITY: usize = 256;
const OPERATION_THREAD_NAME: &str = "file-operation-worker";

/// The work of one request.
enum Work {
    Scan { intent: OperationIntent, skipped: Arc<[ScanSkip]> },
    Execute { plan: Arc<[PlannedItem]>, at: usize, choice: Option<OperationChoice>, progress: OperationProgress },
    Finalize { directories: Arc<[Location]>, at: usize },
    CleanUp,
}

struct Request {
    id: OperationId,
    generation: u64,
    work: Work,
}

/// Lane work that belongs to no operation.
enum Service {
    /// Opens the journal, reports its status, and sweeps earlier leftovers.
    Start,
    ReopenJournal,
}

/// What every request shares.
pub struct Services {
    pub fs: Arc<dyn FileSystem>,
    pub journal: Journal,
    pub budget: Duration,
}

/// Publishes events: `event` for results, `progress` for replaceable byte
/// progress, which the receiver keeps only the newest of per operation.
#[derive(Clone)]
pub struct Publisher {
    pub event: Arc<dyn Fn(Event) + Send + Sync>,
    pub progress: Arc<dyn Fn(OperationId, Event) + Send + Sync>,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<Request>,
    services: VecDeque<Service>,
    /// The write scopes of each operation's running request.
    running: HashMap<OperationId, Arc<[Location]>>,
    /// Each operation's write scopes, from its intent.
    scopes: HashMap<OperationId, Arc<[Location]>>,
    /// Each operation's state while no request of it runs.
    states: HashMap<OperationId, OperationState>,
    cancelled: HashMap<OperationId, Arc<AtomicBool>>,
    /// Dismissed operations whose running request has not finished yet.
    released: Vec<OperationId>,
    /// No new operation work is admitted; cleanups still run.
    closed: bool,
    workers: usize,
}

struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
    finished: Condvar,
    services: Services,
    publisher: Publisher,
}

impl Queue {
    /// Drops everything kept for a dismissed operation.
    fn forget(&mut self, id: OperationId) {
        self.states.remove(&id);
        self.scopes.remove(&id);
        self.cancelled.remove(&id);
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub struct OperationLane {
    shared: Arc<Shared>,
}

/// The paths an operation writes: its destination roots, plus the source
/// roots of a move, Trash, or deletion.
fn write_scopes(intent: &OperationIntent) -> Arc<[Location]> {
    let source = intent.source();
    let roots = intent.targets().iter().map(|target| source.join(&target.name));
    let destination = |name| intent.destination().map(|destination: &Location| destination.join(name));
    match intent.kind() {
        OperationKind::Copy => intent.targets().iter().filter_map(|target| destination(&target.name)).collect(),
        OperationKind::Move => roots.chain(intent.targets().iter().filter_map(|target| destination(&target.name))).collect(),
        OperationKind::Rename { to } => roots.chain([source.join(to)]).collect(),
        OperationKind::NewFolder { name } => [source.join(name)].into(),
        OperationKind::MoveToTrash | OperationKind::DeletePermanently => roots.collect(),
    }
}

fn overlaps(left: &[Location], right: &[Location]) -> bool {
    left.iter().any(|a| right.iter().any(|b| starts_with(a, b) || starts_with(b, a)))
}

impl OperationLane {
    /// Starts the workers, then opens the journal and sweeps leftovers of
    /// earlier launches on one of them.
    pub fn start(services: Services, publisher: Publisher) -> std::io::Result<Self> {
        let shared = Arc::new(Shared { queue: Mutex::new(Queue::default()), ready: Condvar::new(), finished: Condvar::new(), services, publisher });
        shared.lock().services.push_back(Service::Start);
        for _ in 0..OPERATION_WORKERS {
            let worker = Arc::clone(&shared);
            thread::Builder::new().name(OPERATION_THREAD_NAME.to_owned()).spawn(move || work(&worker))?;
            shared.lock().workers += 1;
        }
        Ok(Self { shared })
    }

    /// Accepts one operation effect without waiting. A request that cannot
    /// be admitted returns its terminal event at once.
    pub fn submit(&self, effect: OperationEffect) -> Option<Event> {
        let mut queue = self.shared.lock();
        let (id, generation, work) = match effect {
            OperationEffect::Release { id } => {
                queue.pending.retain(|request| request.id != id);
                if queue.running.contains_key(&id) {
                    queue.released.push(id);
                } else {
                    queue.forget(id);
                }
                return None;
            }
            OperationEffect::Cancel { id, generation } => {
                // Stop the in-flight request at its next safe boundary; the
                // cleanup runs after it. Queued requests of this job are stale.
                queue.cancelled.entry(id).or_default().store(true, Ordering::Relaxed);
                queue.pending.retain(|request| request.id != id);
                queue.pending.push_back(Request { id, generation, work: Work::CleanUp });
                drop(queue);
                self.shared.ready.notify_all();
                return None;
            }
            OperationEffect::Scan { id, generation, intent, skipped } => {
                queue.scopes.insert(id, write_scopes(&intent));
                queue.cancelled.entry(id).or_default();
                (id, generation, Work::Scan { intent, skipped })
            }
            OperationEffect::Execute { id, generation, plan, at, choice, progress, .. } => (id, generation, Work::Execute { plan, at, choice, progress }),
            OperationEffect::Finalize { id, generation, directories, at } => (id, generation, Work::Finalize { directories, at }),
        };
        if queue.closed || queue.workers == 0 || queue.pending.len() >= PENDING_CAPACITY {
            return Some(Event::OperationExecutorUnavailable { id, generation });
        }
        queue.pending.push_back(Request { id, generation, work });
        drop(queue);
        self.shared.ready.notify_all();
        None
    }

    /// Asks a worker to reopen the journal; its status arrives as an event.
    pub fn reopen_journal(&self) {
        self.shared.lock().services.push_back(Service::ReopenJournal);
        self.shared.ready.notify_all();
    }

    /// Admits no new work, stops running requests at their next safe
    /// boundary, lets queued cleanups run, and waits at most `timeout` for
    /// the workers to finish. Leftovers are swept at the next launch.
    pub fn shut_down(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut queue = self.shared.lock();
        queue.closed = true;
        queue.pending.retain(|request| matches!(request.work, Work::CleanUp));
        for flag in queue.cancelled.values() {
            flag.store(true, Ordering::Relaxed);
        }
        self.shared.ready.notify_all();
        while queue.workers > 0 {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else { break };
            queue = self.shared.finished.wait_timeout(queue, left).unwrap_or_else(|poisoned| poisoned.into_inner()).0;
        }
    }
}

impl Drop for OperationLane {
    fn drop(&mut self) {
        let mut queue = self.shared.lock();
        queue.closed = true;
        for flag in queue.cancelled.values() {
            flag.store(true, Ordering::Relaxed);
        }
        drop(queue);
        self.shared.ready.notify_all();
    }
}

/// The write scopes a request holds while it runs. A scan only reads, so it
/// holds none.
fn leases(queue: &Queue, request: &Request) -> Arc<[Location]> {
    if matches!(request.work, Work::Scan { .. }) { Arc::from([]) } else { queue.scopes.get(&request.id).cloned().unwrap_or_else(|| Arc::from([])) }
}

enum Next {
    Request(Box<(Request, Option<OperationState>, Arc<AtomicBool>)>),
    Service(Service),
}

fn work(shared: &Shared) {
    loop {
        let mut queue = shared.lock();
        let next = loop {
            let eligible = queue.pending.iter().position(|request| !queue.running.contains_key(&request.id) && !queue.running.values().any(|held| overlaps(&leases(&queue, request), held)));
            if let Some(index) = eligible
                && let Some(request) = queue.pending.remove(index)
            {
                let scopes = leases(&queue, &request);
                queue.running.insert(request.id, scopes);
                let state = queue.states.remove(&request.id);
                let cancelled = Arc::clone(queue.cancelled.entry(request.id).or_default());
                break Next::Request(Box::new((request, state, cancelled)));
            }
            if let Some(service) = queue.services.pop_front() {
                break Next::Service(service);
            }
            if queue.closed && queue.pending.is_empty() {
                queue.workers -= 1;
                shared.finished.notify_all();
                return;
            }
            queue = shared.ready.wait(queue).unwrap_or_else(|poisoned| poisoned.into_inner());
        };
        drop(queue);
        match next {
            Next::Service(service) => serve(shared, service),
            Next::Request(next) => {
                let (request, mut state, cancelled) = *next;
                let (id, generation, cleanup) = (request.id, request.generation, matches!(request.work, Work::CleanUp));
                let progress = Arc::clone(&shared.publisher.progress);
                let mut publish_progress = move |bytes: ByteProgress| progress(id, Event::OperationProgress { id, generation, bytes });
                let outcome = catch_unwind(AssertUnwindSafe(|| respond(&shared.services, &mut state, request, &cancelled, &mut publish_progress)));
                let events = match outcome {
                    Ok(events) => events,
                    Err(payload) => {
                        drop(payload);
                        // The state may be inconsistent, so it is dropped and the
                        // step is never replayed. A fresh pass removes this
                        // operation's temporaries through its journal records.
                        state = None;
                        let journal = shared.services.journal.clone();
                        catch_unwind(AssertUnwindSafe(|| clean_up_after_loss(id, shared.services.fs.as_ref(), &journal))).ok();
                        vec![Event::OperationExecutorUnavailable { id, generation }]
                    }
                };
                let mut queue = shared.lock();
                queue.running.remove(&id);
                if let Some(position) = queue.released.iter().position(|released| *released == id) {
                    queue.released.swap_remove(position);
                    state = None;
                    queue.forget(id);
                }
                match state {
                    Some(state) if !cleanup => {
                        queue.states.insert(id, state);
                    }
                    _ => {
                        queue.scopes.remove(&id);
                        if cleanup {
                            queue.cancelled.remove(&id);
                        }
                    }
                }
                drop(queue);
                shared.ready.notify_all();
                for event in events {
                    (shared.publisher.event)(event);
                }
            }
        }
    }
}

fn serve(shared: &Shared, service: Service) {
    let journal = &shared.services.journal;
    let available = catch_unwind(AssertUnwindSafe(|| journal.reopen())).unwrap_or(false);
    (shared.publisher.event)(Event::JournalStatus { available });
    if matches!(service, Service::Start)
        && let Ok((removed, failed)) = catch_unwind(AssertUnwindSafe(|| journal.sweep()))
    {
        (shared.publisher.event)(Event::TemporariesSwept { removed, failed });
    }
}

/// Runs one request with its operation's state and returns the events it
/// produced. Every cleanup produces exactly one `OperationCleaned`, even with
/// nothing in flight or no state at all.
fn respond(services: &Services, state: &mut Option<OperationState>, request: Request, cancelled: &AtomicBool, progress: &mut dyn FnMut(ByteProgress)) -> Vec<Event> {
    let Request { id, generation, work } = request;
    let unavailable = || vec![Event::OperationExecutorUnavailable { id, generation }];
    match work {
        Work::Scan { intent, skipped } => {
            let launch = services.journal.launch();
            match scan(&intent, &skipped, launch, cancelled) {
                None => vec![],
                Some(Err(failure)) => {
                    state.get_or_insert_with(|| OperationState::new(id, intent));
                    vec![Event::OperationScanned { id, generation, plan: Err(failure) }]
                }
                Some(Ok(scanned)) => {
                    let plan = Arc::clone(&scanned.plan);
                    state.get_or_insert_with(|| OperationState::new(id, intent)).scanned(scanned);
                    vec![Event::OperationScanned { id, generation, plan: Ok(plan) }]
                }
            }
        }
        Work::Execute { plan, at, choice, progress: accepted } => {
            let Some(operation) = state.as_mut() else { return unavailable() };
            let mut events = Vec::new();
            // Trying again after the journal failed reopens it first.
            if choice == Some(OperationChoice::TryAgain) && !services.journal.is_available() {
                events.push(Event::JournalStatus { available: services.journal.reopen() });
            }
            let mut context = StepContext { fs: services.fs.as_ref(), journal: &services.journal, cancelled, budget: services.budget, progress };
            match operation.execute(&mut context, &plan, at, choice, accepted) {
                Outcome::Done(result) => {
                    if matches!(result, StepResult::Finished { .. } | StepResult::Failed { .. }) {
                        operation.release_journal(&services.journal);
                    }
                    events.extend(operation.trash_folders.drain(..).map(|location| Event::LocationInvalidated { location }));
                    events.push(Event::OperationStepped { id, generation, result });
                    events
                }
                Outcome::Stopped => events,
                Outcome::Unavailable => {
                    events.extend(unavailable());
                    events
                }
            }
        }
        Work::Finalize { directories, at } => {
            let Some(operation) = state.as_mut() else { return unavailable() };
            let context = StepContext { fs: services.fs.as_ref(), journal: &services.journal, cancelled, budget: services.budget, progress };
            match operation.finalize(&context, &directories, at) {
                Outcome::Done(result) => {
                    if result == FinalizeResult::Finished {
                        operation.release_journal(&services.journal);
                    }
                    vec![Event::OperationFinalized { id, generation, result }]
                }
                Outcome::Stopped => vec![],
                Outcome::Unavailable => unavailable(),
            }
        }
        Work::CleanUp => {
            let result = match state.take() {
                Some(mut operation) => operation.clean_up(services.fs.as_ref(), &services.journal),
                None => clean_up_after_loss(id, services.fs.as_ref(), &services.journal),
            };
            vec![Event::OperationCleaned { id, generation, result }]
        }
    }
}

/// Runs operation effects one at a time on the calling thread, as the lane's
/// workers would, so tests can follow a job through a real `Workspace`.
#[cfg(test)]
pub struct SerialLane {
    pub services: Services,
    pub states: HashMap<OperationId, OperationState>,
    pub cancelled: HashMap<OperationId, Arc<AtomicBool>>,
    pub progress: Vec<Event>,
}

#[cfg(test)]
impl SerialLane {
    pub fn new(services: Services) -> Self {
        Self { services, states: HashMap::new(), cancelled: HashMap::new(), progress: Vec::new() }
    }
    /// The events one effect produces; a cancel stops nothing in flight here.
    pub fn run(&mut self, effect: OperationEffect) -> Vec<Event> {
        let (id, generation, work) = match effect {
            OperationEffect::Release { id } => {
                self.states.remove(&id);
                self.cancelled.remove(&id);
                return vec![];
            }
            OperationEffect::Cancel { id, generation } => (id, generation, Work::CleanUp),
            OperationEffect::Scan { id, generation, intent, skipped } => (id, generation, Work::Scan { intent, skipped }),
            OperationEffect::Execute { id, generation, plan, at, choice, progress, .. } => (id, generation, Work::Execute { plan, at, choice, progress }),
            OperationEffect::Finalize { id, generation, directories, at } => (id, generation, Work::Finalize { directories, at }),
        };
        let cleanup = matches!(work, Work::CleanUp);
        let mut state = self.states.remove(&id);
        let cancelled = Arc::clone(self.cancelled.entry(id).or_default());
        let mut sink = Vec::new();
        let events = respond(&self.services, &mut state, Request { id, generation, work }, &cancelled, &mut |bytes| sink.push(Event::OperationProgress { id, generation, bytes }));
        self.progress.extend(sink);
        if let Some(state) = state.filter(|_| !cleanup) {
            self.states.insert(id, state);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver, Sender};

    use dual_pane_application::{Command, OperationOutcome, OperationStatus, WorkRequest, Workspace};
    use dual_pane_domain::{BrowserSide, EntryName};

    use super::*;
    use crate::native_calls;
    use crate::native_location::location_from_path;
    use crate::operation_journal::{JOURNAL_FILE_NAME, LaunchId, temporary_name};

    const TIMEOUT: Duration = Duration::from_secs(5);

    /// Copies that can wait at a gate or panic after writing their target.
    struct GateFs {
        started: Mutex<Sender<PathBuf>>,
        gate: Mutex<Option<Receiver<()>>>,
        panics: AtomicBool,
    }

    impl GateFs {
        fn write(&self, to: &Path) -> io::Result<()> {
            self.started.lock().unwrap().send(to.to_path_buf()).ok();
            if self.panics.swap(false, Ordering::SeqCst) {
                fs::write(to, b"partial")?;
                panic!("injected copy panic");
            }
            if let Some(gate) = self.gate.lock().unwrap().as_ref() {
                gate.recv_timeout(TIMEOUT).ok();
            }
            Ok(())
        }
    }

    impl FileSystem for GateFs {
        fn clone_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.write(to)?;
            native_calls::clone_file(from, to)
        }
        fn copy_file(&self, from: &Path, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<bool> {
            self.write(to)?;
            native_calls::copy_file(from, to, progress)
        }
        fn move_to_trash(&self, _path: &Path) -> io::Result<PathBuf> {
            Err(io::Error::other("no Trash"))
        }
    }

    struct Fixture {
        root: tempfile::TempDir,
        lane: OperationLane,
        events: Receiver<Event>,
        started: Receiver<PathBuf>,
        release: Sender<()>,
        fs: Arc<GateFs>,
        ws: Workspace,
        log: Vec<Event>,
    }

    impl Fixture {
        fn new(budget: Duration, gated: bool) -> Self {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("from")).unwrap();
            fs::create_dir(root.path().join("to")).unwrap();
            let (started_sender, started) = mpsc::channel();
            let (release, gate) = mpsc::channel();
            let fs = Arc::new(GateFs { started: Mutex::new(started_sender), gate: Mutex::new(gated.then_some(gate)), panics: AtomicBool::new(false) });
            let journal = Journal::new(Some(root.path().join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([5; 16]));
            let (sender, events) = mpsc::channel();
            let sender = Arc::new(Mutex::new(sender));
            let progress_sender = Arc::clone(&sender);
            let publisher = Publisher { event: Arc::new(move |event| drop(sender.lock().unwrap().send(event))), progress: Arc::new(move |_, event| drop(progress_sender.lock().unwrap().send(event))) };
            let lane = OperationLane::start(Services { fs: Arc::clone(&fs) as Arc<dyn FileSystem>, journal, budget }, publisher).unwrap();
            Self { root, lane, events, started, release, fs, ws: Workspace::new(), log: Vec::new() }
        }
        fn location(&self, relative: &str) -> Location {
            location_from_path(&self.root.path().join(relative)).unwrap()
        }
        fn show(&mut self, entries: &[&str]) {
            for (browser, folder, rows) in [(BrowserSide::Left, "from", entries), (BrowserSide::Right, "to", &[][..])] {
                let location = self.location(folder);
                let read = self.ws.handle(Command::Navigate { browser, location }.into());
                let Some(WorkRequest::ReadDirectory { tab, token, .. }) = read.work.into_iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) else { panic!() };
                let rows = rows.iter().map(|name| dual_pane_domain::Entry::new(EntryName::new(*name).unwrap(), dual_pane_domain::EntryKind::File)).collect();
                self.ws.handle(Event::FolderItemsLoaded { browser, tab, token, entries: rows, changes: None }.into());
            }
        }
        /// Starts `kind` on the shown item `name` and submits its work.
        fn start(&mut self, name: &str, kind: OperationKind) -> OperationId {
            let tab = self.ws.active_tab(BrowserSide::Left);
            let row = self.ws.entries(BrowserSide::Left).iter().position(|entry| entry.name().as_bytes() == name.as_bytes()).unwrap_or(0);
            if !matches!(kind, OperationKind::NewFolder { .. }) {
                self.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: EntryName::new(name).unwrap() }.into());
            }
            let started = self.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind }.into());
            let id = self.ws.operation_jobs().last().unwrap().id();
            self.submit(started.work);
            id
        }
        fn submit(&mut self, work: Vec<WorkRequest>) {
            let mut work = VecDeque::from(work);
            while let Some(request) = work.pop_front() {
                if let WorkRequest::Operation(effect) = request
                    && let Some(event) = self.lane.submit(effect)
                {
                    self.log.push(event.clone());
                    work.extend(self.ws.handle(event.into()).work);
                }
            }
        }
        /// Handles lane events until `done` holds.
        fn until(&mut self, done: impl Fn(&Workspace) -> bool) {
            let deadline = Instant::now() + TIMEOUT;
            while !done(&self.ws) {
                let left = deadline.checked_duration_since(Instant::now()).expect("the expected state arrives");
                let event = self.events.recv_timeout(left).expect("an event");
                self.log.push(event.clone());
                let work = self.ws.handle(event.into()).work;
                self.submit(work);
            }
        }
        fn status(&self, id: OperationId) -> OperationStatus {
            self.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().status().clone()
        }
    }

    fn running(id: OperationId) -> impl Fn(&Workspace) -> bool {
        move |ws| ws.operation_jobs().iter().any(|job| job.id() == id && matches!(job.status(), OperationStatus::Running { .. }))
    }

    fn finished(id: OperationId) -> impl Fn(&Workspace) -> bool {
        move |ws| ws.operation_jobs().iter().any(|job| job.id() == id && matches!(job.status(), OperationStatus::Finished(_)))
    }

    #[test]
    fn startup_reports_the_journal_and_sweeps_earlier_leftovers() {
        let fixture = Fixture::new(STEP_BUDGET, false);
        let first = fixture.events.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(first, Event::JournalStatus { available: true });
        assert_eq!(fixture.events.recv_timeout(TIMEOUT).unwrap(), Event::TemporariesSwept { removed: 0, failed: vec![] });
    }

    #[test]
    fn a_panicking_step_ends_its_job_removes_its_temporary_and_the_lane_continues() {
        let mut fixture = Fixture::new(STEP_BUDGET, false);
        fs::write(fixture.root.path().join("from/a.txt"), b"a").unwrap();
        fixture.show(&["a.txt"]);
        fixture.fs.panics.store(true, Ordering::SeqCst);
        let id = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(finished(id));
        assert_eq!(fixture.status(id), OperationStatus::Finished(OperationOutcome::Failed));
        assert_eq!(fixture.ws.operation_jobs()[0].failure().map(|failure| failure.kind), Some(dual_pane_application::OperationErrorKind::ExecutorUnavailable));
        let leftovers = fs::read_dir(fixture.root.path().join("to")).unwrap().count();
        assert_eq!(leftovers, 0, "the panicked step's temporary is removed through the journal");
        assert!(!fixture.root.path().join("to").join(temporary_name(LaunchId::from_bytes([5; 16]), id.get(), 0)).exists());
        let again = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(finished(again));
        assert_eq!(fixture.status(again), OperationStatus::Finished(OperationOutcome::Succeeded), "the step is never replayed, and the lane keeps working");
    }

    #[test]
    fn a_cancel_waits_for_the_running_step_and_reports_its_cleanup_once() {
        let mut fixture = Fixture::new(STEP_BUDGET, true);
        fs::write(fixture.root.path().join("from/a.txt"), b"a").unwrap();
        fixture.show(&["a.txt"]);
        let id = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(running(id));
        fixture.started.recv_timeout(TIMEOUT).expect("the copy runs");
        let cancel = fixture.ws.handle(Command::CancelOperation { id }.into()).work;
        fixture.submit(cancel);
        assert!(fixture.events.recv_timeout(Duration::from_millis(150)).iter().all(|event| !matches!(event, Event::OperationCleaned { .. })), "cleanup waits for the running step");
        fixture.release.send(()).unwrap();
        fixture.until(finished(id));
        assert_eq!(fixture.log.iter().filter(|event| matches!(event, Event::OperationCleaned { .. })).count(), 1);
        assert_eq!(fs::read_dir(fixture.root.path().join("to")).unwrap().filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().contains(".dual-pane-")).count(), 0);
    }

    #[test]
    fn overlapping_write_scopes_wait_while_others_proceed() {
        let mut fixture = Fixture::new(STEP_BUDGET, true);
        fs::write(fixture.root.path().join("from/a.txt"), b"a").unwrap();
        fixture.show(&["a.txt"]);
        let first = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(running(first));
        fixture.started.recv_timeout(TIMEOUT).expect("the first copy runs");
        let second = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(running(second));
        let folder = fixture.start("a.txt", OperationKind::NewFolder { name: EntryName::new("made").unwrap() });
        fixture.until(finished(folder));
        assert!(!matches!(fixture.status(second), OperationStatus::Waiting { .. } | OperationStatus::Finished(_)), "the second copy into the same item waits for the first");
        assert!(fixture.started.recv_timeout(Duration::from_millis(100)).is_err());
        fixture.release.send(()).unwrap();
        fixture.until(finished(first));
        fixture.until(|ws| ws.operation_jobs().iter().any(|job| job.id() == second && job.decision().is_some()));
        assert_eq!(fixture.ws.operation_jobs().iter().find(|job| job.id() == second).unwrap().decision().unwrap().issue, dual_pane_domain::OperationIssue::FileConflict, "it then finds the first copy's result");
    }

    /// Copies `a.txt` onto an existing one, waits for the conflict, and
    /// answers it with `choice`.
    fn decide_conflict(choice: dual_pane_domain::OperationChoice) -> (Fixture, OperationId) {
        let mut fixture = Fixture::new(STEP_BUDGET, false);
        fs::write(fixture.root.path().join("from/a.txt"), b"new").unwrap();
        fs::write(fixture.root.path().join("to/a.txt"), b"old").unwrap();
        fixture.show(&["a.txt"]);
        let id = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(|ws| ws.operation_jobs().iter().any(|job| job.id() == id && job.decision().is_some()));
        let decision = fixture.ws.operation_jobs()[0].decision().unwrap();
        assert_eq!(decision.issue, dual_pane_domain::OperationIssue::FileConflict);
        let (token, item) = (decision.token, decision.item.clone());
        let decided = fixture.ws.handle(Command::DecideOperation { id, token, item, choice, apply_to_all: false }.into()).work;
        fixture.submit(decided);
        (fixture, id)
    }

    fn leftovers(fixture: &Fixture) -> usize {
        fs::read_dir(fixture.root.path().join("to")).unwrap().filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().contains(".dual-pane-")).count()
    }

    #[test]
    fn a_replace_decision_round_trips_through_the_lane_and_a_stale_result_is_ignored() {
        let (mut fixture, id) = decide_conflict(dual_pane_domain::OperationChoice::Replace);
        let stale = fixture.log.iter().rev().find(|event| matches!(event, Event::OperationStepped { result: dual_pane_application::StepResult::DecisionRequired { .. }, .. })).cloned().expect("the conflict result");
        let before = fixture.status(id);
        let replayed = fixture.ws.handle(stale.into());
        assert!(replayed.work.is_empty(), "the pre-decision result asks for nothing");
        assert_eq!(fixture.status(id), before, "and changes nothing");
        fixture.until(finished(id));
        assert_eq!(fixture.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
        assert_eq!(fs::read(fixture.root.path().join("to/a.txt")).unwrap(), b"new");
        assert_eq!(leftovers(&fixture), 0, "the replacement's temporary is gone");
    }

    #[test]
    fn a_skip_decision_leaves_the_destination_untouched() {
        let (mut fixture, id) = decide_conflict(dual_pane_domain::OperationChoice::Skip);
        fixture.until(finished(id));
        assert_eq!(fixture.status(id), OperationStatus::Finished(OperationOutcome::Partial));
        assert_eq!(fs::read(fixture.root.path().join("to/a.txt")).unwrap(), b"old");
        assert_eq!(leftovers(&fixture), 0);
    }

    #[test]
    fn shutting_down_waits_no_longer_than_its_bound_for_a_stuck_step() {
        let mut fixture = Fixture::new(STEP_BUDGET, true);
        fs::write(fixture.root.path().join("from/a.txt"), b"a").unwrap();
        fixture.show(&["a.txt"]);
        let id = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(running(id));
        fixture.started.recv_timeout(TIMEOUT).expect("the copy is stuck at its gate");
        let started = Instant::now();
        fixture.lane.shut_down(Duration::from_millis(200));
        assert!(started.elapsed() < Duration::from_secs(2), "quitting is not held up by a step that ignores cancellation");
        fixture.release.send(()).unwrap();
    }

    #[test]
    fn dismissing_a_finished_job_releases_what_the_lane_kept() {
        let mut fixture = Fixture::new(STEP_BUDGET, false);
        fs::write(fixture.root.path().join("from/a.txt"), b"a").unwrap();
        fixture.show(&["a.txt"]);
        let id = fixture.start("a.txt", OperationKind::Copy);
        fixture.until(finished(id));
        assert!(fixture.lane.shared.lock().states.contains_key(&id), "a finished job's state waits for dismissal");
        let dismissed = fixture.ws.handle(Command::DismissOperation { id }.into()).work;
        fixture.submit(dismissed);
        let queue = fixture.lane.shared.lock();
        assert!(!queue.states.contains_key(&id) && !queue.scopes.contains_key(&id) && !queue.cancelled.contains_key(&id));
    }

    #[test]
    fn a_step_yields_after_its_time_budget() {
        let mut fixture = Fixture::new(Duration::ZERO, false);
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(fixture.root.path().join("from").join(name), name).unwrap();
        }
        fixture.show(&["a.txt", "b.txt", "c.txt"]);
        let tab = fixture.ws.active_tab(BrowserSide::Left);
        fixture.ws.handle(Command::SelectAll { browser: BrowserSide::Left, tab }.into());
        let started = fixture.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Copy }.into());
        let id = fixture.ws.operation_jobs()[0].id();
        fixture.submit(started.work);
        fixture.until(finished(id));
        let steps = fixture.log.iter().filter(|event| matches!(event, Event::OperationStepped { .. })).count();
        assert_eq!(steps, 3, "each step handled one item before yielding");
        assert_eq!(fixture.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    }

    #[test]
    fn a_closed_lane_fails_new_work_at_once_but_still_cleans_up() {
        let fixture = Fixture::new(STEP_BUDGET, false);
        fixture.lane.shut_down(TIMEOUT);
        let id = OperationId::new(3);
        let intent = OperationIntent::new(OperationKind::MoveToTrash, fixture.location("from"), vec![dual_pane_domain::OperationTarget { name: EntryName::new("x").unwrap(), kind: dual_pane_domain::EntryKind::File }], None).unwrap();
        assert_eq!(fixture.lane.submit(OperationEffect::Scan { id, generation: 1, intent, skipped: Arc::from([]) }), Some(Event::OperationExecutorUnavailable { id, generation: 1 }));
    }

    #[test]
    fn write_scopes_cover_destinations_and_moved_or_removed_sources() {
        let name = |text: &str| dual_pane_domain::EntryName::new(text).unwrap();
        let location = |parts: &[&str]| parts.iter().fold(Location::root(), |location, part| location.join(&name(part)));
        let target = vec![dual_pane_domain::OperationTarget { name: name("a"), kind: dual_pane_domain::EntryKind::File }];
        let copy = OperationIntent::new(OperationKind::Copy, location(&["src"]), target.clone(), Some(location(&["dst"]))).unwrap();
        assert_eq!(write_scopes(&copy).to_vec(), vec![location(&["dst", "a"])]);
        let moved = OperationIntent::new(OperationKind::Move, location(&["src"]), target.clone(), Some(location(&["dst"]))).unwrap();
        assert_eq!(write_scopes(&moved).to_vec(), vec![location(&["src", "a"]), location(&["dst", "a"])]);
        assert!(overlaps(&[location(&["dst"])], &[location(&["dst", "a", "b"])]));
        assert!(!overlaps(&[location(&["dst", "a"])], &[location(&["dst", "ab"])]));
    }
}
