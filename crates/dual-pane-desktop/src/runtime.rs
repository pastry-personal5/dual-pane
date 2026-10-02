use std::collections::{HashMap, VecDeque};
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::thread;
use std::time::Duration;

use dual_pane_application::{Event, FavoriteProbeOutcome, RowChange, SettingsFailure, WorkRequest, listing_changes};
use dual_pane_domain::{BrowserSide, Entry, ListingErrorKind, Location, RequestToken, SortSpec, TabId};

type FolderItemsOutcome = Option<Result<Arc<[Entry]>, ListingErrorKind>>;
pub type FolderItemsSource = Box<dyn Fn(&Location, SortSpec, &AtomicBool) -> FolderItemsOutcome + Send>;
pub type FolderItemsSourceFactory = Arc<dyn Fn() -> FolderItemsSource + Send + Sync>;
pub type Wake = Box<dyn Fn() + Send>;
/// Checks one location on the probe worker, never on the GUI thread.
pub type LocationProbe = Box<dyn Fn(&Location) -> FavoriteProbeOutcome + Send>;
type WorkerTask = Box<dyn FnOnce() + Send>;

pub trait WorkRunner {
    /// Dispatches outside work without waiting. A terminal event is returned
    /// directly when the request can finish before leaving the GUI thread.
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event>;
    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool);
}

/// One bounded listing queue per browser, served by a fixed number of reader
/// lanes so a slow or blocked folder in one tab cannot stall the Browser's
/// other tabs. A lane reads, sorts, and diffs against the request's previous
/// rows, so the GUI thread only applies the finished change. Each lane runs one job at a time; a panic is contained,
/// reported safely, and followed by a replacement after backoff.
pub struct Runtime {
    left_jobs: JobSender,
    right_jobs: JobSender,
    /// One serialized lane for short location probes, separate from reads.
    probes: SyncSender<ProbeJob>,
    events: Receiver<Event>,
    next_event: Option<Event>,
    wake_pending: Arc<AtomicBool>,
    outstanding: HashMap<RequestToken, (BrowserSide, TabId, Arc<JobState>)>,
    /// Reads per Browser that may be queued, running, or awaiting delivery.
    read_capacity: usize,
}

#[derive(Clone)]
struct Job {
    browser: BrowserSide,
    tab: TabId,
    token: RequestToken,
    location: Location,
    sort: SortSpec,
    /// The shown Folder Items this read's result is diffed against.
    previous: Option<Arc<[Entry]>>,
    state: Arc<JobState>,
}

struct JobQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct QueueState {
    pending: VecDeque<Job>,
    closed: bool,
    receivers: usize,
}

const PENDING_READ_CAPACITY: usize = 16;
/// Probes that may wait for the probe lane before admission fails.
const PENDING_PROBE_CAPACITY: usize = 16;
/// Concurrent readers per Browser. Provisional: P3-M6 and later measurement may
/// tune it, but it stays a finite bound so blocked I/O cannot grow threads.
pub const READER_LANES_PER_BROWSER: usize = 4;

struct JobSender(Arc<JobQueue>);
struct JobReceiver(Arc<JobQueue>);

fn job_queue() -> (JobSender, JobReceiver) {
    let queue = Arc::new(JobQueue { state: Mutex::new(QueueState { receivers: 1, ..QueueState::default() }), ready: Condvar::new() });
    (JobSender(Arc::clone(&queue)), JobReceiver(queue))
}

impl JobSender {
    fn send(&self, job: Job) -> Result<(), (Job, bool)> {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending.retain(|pending| !pending.state.cancelled.load(Ordering::Relaxed));
        if state.closed || state.pending.len() >= PENDING_READ_CAPACITY {
            return Err((job, state.closed));
        }
        state.pending.push_back(job);
        self.0.ready.notify_one();
        Ok(())
    }
}

impl Drop for JobSender {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        self.0.ready.notify_all();
    }
}

impl Iterator for JobReceiver {
    type Item = Job;

    fn next(&mut self) -> Option<Self::Item> {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(job) = state.pending.pop_front() {
                return Some(job);
            }
            if state.closed {
                return None;
            }
            state = self.0.ready.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Every lane of a Browser holds a receiver for the same queue.
impl Clone for JobReceiver {
    fn clone(&self) -> Self {
        self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).receivers += 1;
        Self(Arc::clone(&self.0))
    }
}

impl Drop for JobReceiver {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receivers = state.receivers.saturating_sub(1);
        if state.receivers == 0 {
            state.closed = true;
        }
        self.0.ready.notify_all();
    }
}

/// A probe and the application event that reports its outcome.
struct ProbeJob {
    location: Location,
    event: Box<dyn FnOnce(FavoriteProbeOutcome) -> Event + Send>,
}

struct JobState {
    cancelled: AtomicBool,
    terminal_claimed: AtomicBool,
}

impl JobState {
    fn claim_terminal(&self) -> bool {
        self.terminal_claimed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }
}

#[derive(Clone)]
struct Delivery {
    events: Sender<Event>,
    wake_pending: Arc<AtomicBool>,
    wake: Arc<Mutex<Wake>>,
}

/// A completed listing and its row change against the job's previous rows.
type ListingOutcome = Option<Result<(Arc<[Entry]>, Option<Vec<RowChange>>), ListingErrorKind>>;

enum WorkerOutcome {
    Completed(ListingOutcome),
    Panicked,
}

const WORKER_THREAD_NAME: &str = "listing-worker";
const PROBE_THREAD_NAME: &str = "location-probe";
const INITIAL_RESTART_DELAY: Duration = Duration::from_millis(100);
const MAX_RESTART_DELAY: Duration = Duration::from_secs(5);
static INSTALL_PANIC_HOOK: Once = Once::new();

impl Runtime {
    pub fn start(source_factory: FolderItemsSourceFactory, probe: LocationProbe, wake: Wake) -> io::Result<Self> {
        Self::start_with_lanes(source_factory, probe, wake, READER_LANES_PER_BROWSER)
    }

    fn start_with_lanes(source_factory: FolderItemsSourceFactory, probe: LocationProbe, wake: Wake, lanes: usize) -> io::Result<Self> {
        Self::start_with(source_factory, probe, wake, lanes, |browser, queue, source_factory, delivery| thread::Builder::new().name(format!("listing-supervisor-{browser:?}")).spawn(move || supervise(queue, source_factory, delivery)).map(|_| ()))
    }

    /// Starts `lanes` supervisors per Browser, then the probe lane. If any
    /// cannot start, every queue closes so the threads that did start finish.
    fn start_with(source_factory: FolderItemsSourceFactory, probe: LocationProbe, wake: Wake, lanes: usize, mut spawn: impl FnMut(BrowserSide, JobReceiver, FolderItemsSourceFactory, Delivery) -> io::Result<()>) -> io::Result<Self> {
        install_panic_hook();
        let lanes = lanes.max(1);
        let (left_jobs, left_queue) = job_queue();
        let (right_jobs, right_queue) = job_queue();
        let (event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let delivery = Delivery { events: event_sender, wake_pending: Arc::clone(&wake_pending), wake: Arc::new(Mutex::new(wake)) };
        for (browser, queue) in [(BrowserSide::Left, left_queue), (BrowserSide::Right, right_queue)] {
            for _ in 0..lanes {
                spawn(browser, queue.clone(), Arc::clone(&source_factory), delivery.clone())?;
            }
        }
        let (probes, probe_jobs) = mpsc::sync_channel(PENDING_PROBE_CAPACITY);
        thread::Builder::new().name(PROBE_THREAD_NAME.to_owned()).spawn(move || run_probes(probe_jobs, probe, delivery))?;
        Ok(Self { left_jobs, right_jobs, probes, events, next_event: None, wake_pending, outstanding: HashMap::new(), read_capacity: PENDING_READ_CAPACITY + lanes })
    }

    fn receive(&mut self) -> Option<Event> {
        let event = self.next_event.take().or_else(|| self.events.try_recv().ok())?;
        if let Some((browser, tab, token)) = event_address(&event)
            && self.outstanding.get(&token).is_some_and(|(owner, owner_tab, _)| *owner == browser && *owner_tab == tab)
        {
            self.outstanding.remove(&token);
        }
        Some(event)
    }
}

fn install_panic_hook() {
    INSTALL_PANIC_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |information| {
            let message: Option<&[u8]> = match thread::current().name() {
                Some(WORKER_THREAD_NAME) => Some(b"Dual Pane listing worker stopped unexpectedly.\n"),
                Some(PROBE_THREAD_NAME) => Some(b"Dual Pane location probe stopped unexpectedly.\n"),
                _ => None,
            };
            if let Some(message) = message {
                io::stderr().write_all(message).ok();
            } else {
                previous(information);
            }
        }));
    });
}

impl WorkRunner for Runtime {
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
        match request {
            WorkRequest::Operation(effect) => {
                let (id, generation) = match effect {
                    dual_pane_application::OperationEffect::Scan { id, generation, .. } | dual_pane_application::OperationEffect::Execute { id, generation, .. } | dual_pane_application::OperationEffect::Cancel { id, generation } => (id, generation),
                };
                Some(Event::OperationExecutorUnavailable { id, generation })
            }
            WorkRequest::ReadDirectory { browser, tab, token, location, sort, previous } => {
                if self.outstanding.values().filter(|(owner, _, _)| *owner == browser).count() >= self.read_capacity {
                    return Some(Event::FolderItemsFailed { browser, tab, token, kind: ListingErrorKind::Busy });
                }
                let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
                self.outstanding.insert(token, (browser, tab, Arc::clone(&state)));
                let jobs = match browser {
                    BrowserSide::Left => &self.left_jobs,
                    BrowserSide::Right => &self.right_jobs,
                };
                if let Err((job, closed)) = jobs.send(Job { browser, tab, token, location, sort, previous, state })
                    && job.state.claim_terminal()
                {
                    self.outstanding.remove(&job.token);
                    return Some(Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: if closed { ListingErrorKind::Internal } else { ListingErrorKind::Busy } });
                }
                None
            }
            WorkRequest::Cancel { browser, tab, token } => {
                if let Some((owner, owner_tab, state)) = self.outstanding.get(&token) {
                    if *owner != browser || *owner_tab != tab {
                        return None;
                    }
                    state.cancelled.store(true, Ordering::Relaxed);
                    if !state.claim_terminal() {
                        return None;
                    }
                    self.outstanding.remove(&token);
                    return Some(Event::FolderItemsCancelled { browser, tab, token });
                }
                None
            }
            // Settings work belongs to the settings worker; reaching the
            // listing runtime means there is none.
            WorkRequest::SaveSettings { revision, .. } => Some(Event::SettingsSaveFailed { revision, failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::LoadSettings => Some(Event::SettingsLoadFailed { failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::ResetSettings => Some(Event::SettingsResetFailed { failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::ProbeScreenshotsFolder { location } => self.probe(location.clone(), Box::new(move |outcome| Event::ScreenshotsFolderProbed { location, outcome })),
            WorkRequest::ProbeFavoriteTarget { item_id, target } => self.probe(target.clone(), Box::new(move |outcome| Event::FavoriteTargetProbed { item_id, target, outcome })),
        }
    }

    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
        self.wake_pending.swap(false, Ordering::SeqCst);
        let events = std::iter::from_fn(|| self.receive()).take(max).collect();
        if self.next_event.is_none() {
            self.next_event = self.events.try_recv().ok();
        }
        (events, self.next_event.is_some())
    }
}

impl Runtime {
    /// Queues a probe without waiting. If the probe queue is full or the lane
    /// has stopped, the outcome is `Failed` at once so no request is left
    /// pending; a failed probe proves nothing about its location.
    fn probe(&self, location: Location, event: Box<dyn FnOnce(FavoriteProbeOutcome) -> Event + Send>) -> Option<Event> {
        self.probes.try_send(ProbeJob { location, event }).err().map(|(TrySendError::Full(job) | TrySendError::Disconnected(job))| (job.event)(FavoriteProbeOutcome::Failed))
    }
}

/// Runs probes in order. A panicking probe reports `Failed` and the lane
/// continues, because a probe keeps no state between calls.
fn run_probes(jobs: Receiver<ProbeJob>, probe: LocationProbe, delivery: Delivery) {
    for job in jobs {
        let outcome = catch_unwind(AssertUnwindSafe(|| probe(&job.location))).unwrap_or(FavoriteProbeOutcome::Failed);
        deliver_event(&delivery, (job.event)(outcome));
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for (_, _, state) in self.outstanding.values() {
            state.cancelled.store(true, Ordering::Relaxed);
        }
    }
}

fn supervise(queue: impl IntoIterator<Item = Job>, source_factory: FolderItemsSourceFactory, delivery: Delivery) {
    supervise_with(queue, source_factory, delivery, |worker| thread::Builder::new().name(WORKER_THREAD_NAME.to_owned()).spawn(worker).map(|_| ()), thread::sleep);
}

fn supervise_with(queue: impl IntoIterator<Item = Job>, source_factory: FolderItemsSourceFactory, delivery: Delivery, mut spawn_worker: impl FnMut(WorkerTask) -> io::Result<()>, mut sleep: impl FnMut(Duration)) {
    let mut delay = INITIAL_RESTART_DELAY;
    for job in queue {
        if job.state.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let worker_source_factory = Arc::clone(&source_factory);
        let worker_job = job.clone();
        let worker: WorkerTask = Box::new(move || {
            let outcome = match catch_unwind(AssertUnwindSafe(|| {
                let source = worker_source_factory();
                let entries = source(&worker_job.location, worker_job.sort, &worker_job.state.cancelled)?;
                Some(entries.map(|entries| {
                    let changes = worker_job.previous.as_deref().map(|previous| listing_changes(previous, &entries));
                    (entries, changes)
                }))
            })) {
                Ok(outcome) => WorkerOutcome::Completed(outcome),
                Err(payload) => {
                    drop(payload);
                    WorkerOutcome::Panicked
                }
            };
            result_sender.send((worker_job, outcome)).ok();
        });
        if spawn_worker(worker).is_err() {
            finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: ListingErrorKind::Internal });
            sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        }
        let Ok((job, outcome)) = result_receiver.recv() else {
            finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: ListingErrorKind::Internal });
            sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        };
        match outcome {
            WorkerOutcome::Completed(Some(Ok((entries, changes)))) => {
                finish(&delivery, &job, Event::FolderItemsLoaded { browser: job.browser, tab: job.tab, token: job.token, entries, changes });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Completed(Some(Err(kind))) => {
                finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Completed(None) => {
                finish(&delivery, &job, Event::FolderItemsCancelled { browser: job.browser, tab: job.tab, token: job.token });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Panicked => {
                finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: ListingErrorKind::Internal });
                sleep(delay);
                delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            }
        }
    }
}

fn finish(delivery: &Delivery, job: &Job, event: Event) {
    if job.state.claim_terminal() {
        deliver_event(delivery, event);
    }
}

fn deliver_event(delivery: &Delivery, event: Event) {
    if delivery.events.send(event).is_ok() && !delivery.wake_pending.swap(true, Ordering::SeqCst) {
        (delivery.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))();
    }
}

/// The read a listing event completes. Other events carry no read token.
fn event_address(event: &Event) -> Option<(BrowserSide, TabId, RequestToken)> {
    match event {
        Event::FolderItemsLoaded { browser, tab, token, .. } | Event::FolderItemsFailed { browser, tab, token, .. } | Event::FolderItemsCancelled { browser, tab, token, .. } => Some((*browser, *tab, *token)),
        Event::OperationScanned { .. } | Event::OperationStepped { .. } | Event::OperationCleaned { .. } | Event::OperationExecutorUnavailable { .. } | Event::LocationInvalidated { .. } | Event::FavoriteTargetProbed { .. } | Event::ScreenshotsFolderProbed { .. } | Event::SettingsSaved { .. } | Event::SettingsSaveFailed { .. } | Event::SettingsLoaded { .. } | Event::SettingsLoadFailed { .. } | Event::SettingsReset { .. } | Event::SettingsResetFailed { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_application::OperationEffect;
    use dual_pane_domain::{EntryName, SortSpec, TabId};
    use dual_pane_domain::{OperationId, OperationIntent, OperationKind, OperationTarget};
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    const TIMEOUT: Duration = Duration::from_secs(3);
    /// Most tests pin one reader lane so queue order is deterministic; the lane
    /// tests below start several.
    const SINGLE_LANE_CAPACITY: usize = PENDING_READ_CAPACITY + 1;
    fn location(name: &str) -> Location {
        Location::root().join(&EntryName::new(name).unwrap())
    }
    fn token(index: usize) -> RequestToken {
        (0..index).fold(RequestToken::first(), |token, _| token.next())
    }
    fn read(index: usize, name: &str) -> WorkRequest {
        browser_read(BrowserSide::Left, index, name)
    }
    fn browser_read(browser: BrowserSide, index: usize, name: &str) -> WorkRequest {
        WorkRequest::ReadDirectory { browser, tab: TabId::new(index as u64), token: token(index), location: location(name), sort: SortSpec::default(), previous: None }
    }
    #[test]
    fn operation_request_fails_closed_until_an_executor_exists() {
        let (mut runner, _) = runtime(source_factory(|_, _| Some(Ok(Arc::from([])))));
        let id = OperationId::new(7);
        let intent = OperationIntent::new(OperationKind::MoveToTrash, location("source"), vec![OperationTarget { name: EntryName::new("item").unwrap(), kind: dual_pane_domain::EntryKind::File }], None).unwrap();
        assert_eq!(runner.dispatch(WorkRequest::Operation(OperationEffect::Scan { id, generation: 3, intent })), Some(Event::OperationExecutorUnavailable { id, generation: 3 }));
    }
    fn source_factory(source: impl Fn(&Location, &AtomicBool) -> FolderItemsOutcome + Send + Sync + 'static) -> FolderItemsSourceFactory {
        let source = Arc::new(source);
        Arc::new(move || {
            let source = Arc::clone(&source);
            Box::new(move |location, _, cancelled| source(location, cancelled))
        })
    }
    fn runtime(source_factory: FolderItemsSourceFactory) -> (Runtime, Receiver<()>) {
        runtime_with_lanes(source_factory, 1)
    }
    /// A probe for runtimes whose tests do not probe locations.
    fn unused_probe() -> LocationProbe {
        Box::new(|_| FavoriteProbeOutcome::Failed)
    }
    fn runtime_with_lanes(source_factory: FolderItemsSourceFactory, lanes: usize) -> (Runtime, Receiver<()>) {
        runtime_with_probe(source_factory, unused_probe(), lanes)
    }
    fn runtime_with_probe(source_factory: FolderItemsSourceFactory, probe: LocationProbe, lanes: usize) -> (Runtime, Receiver<()>) {
        let (sender, receiver) = mpsc::channel();
        (
            Runtime::start_with_lanes(
                source_factory,
                probe,
                Box::new(move || {
                    sender.send(()).ok();
                }),
                lanes,
            )
            .unwrap(),
            receiver,
        )
    }
    fn wait(runtime: &mut Runtime, wakes: &Receiver<()>) -> Vec<Event> {
        wakes.recv_timeout(TIMEOUT).unwrap();
        runtime.take_events(32).0
    }
    fn loaded_tokens(events: &[Event]) -> Vec<RequestToken> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::FolderItemsLoaded { token, .. } => Some(*token),
                _ => None,
            })
            .collect()
    }

    struct Probe {
        calls: Receiver<(Location, Option<String>)>,
        gate: Sender<()>,
        wakes: Receiver<()>,
        wake_count: Arc<AtomicUsize>,
    }

    fn start_probe(blocked: &'static str) -> (Runtime, Probe) {
        start_probe_with_lanes(blocked, 1)
    }

    fn start_probe_with_lanes(blocked: &'static str, lanes: usize) -> (Runtime, Probe) {
        let (call_sender, calls) = mpsc::channel();
        let (gate, gate_receiver) = mpsc::channel::<()>();
        let gate_receiver = Arc::new(Mutex::new(gate_receiver));
        let (wake_sender, wakes) = mpsc::channel();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let source_factory: FolderItemsSourceFactory = Arc::new(move || {
            let call_sender = call_sender.clone();
            let gate_receiver = Arc::clone(&gate_receiver);
            Box::new(move |location, _, _cancelled| {
                call_sender.send((location.clone(), thread::current().name().map(str::to_owned))).unwrap();
                if *location == self::location(blocked) {
                    gate_receiver.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
                }
                Some(Ok(Arc::from([])))
            })
        });
        let counter = Arc::clone(&wake_count);
        let wake: Wake = Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            wake_sender.send(()).ok();
        });
        (Runtime::start_with_lanes(source_factory, unused_probe(), wake, lanes).unwrap(), Probe { calls, gate, wakes, wake_count })
    }

    impl Probe {
        fn next_call(&self) -> (Location, Option<String>) {
            self.calls.recv_timeout(TIMEOUT).unwrap()
        }

        fn wait_for_wake(&self) {
            self.wakes.recv_timeout(TIMEOUT).unwrap();
        }
    }

    #[test]
    fn runs_a_read_on_the_worker_and_delivers_its_result() {
        let (mut runner, probe) = start_probe("blocked");
        assert_eq!(runner.dispatch(read(0, "alpha")), None);
        assert_eq!(probe.next_call(), (location("alpha"), Some("listing-worker".to_owned())));
        probe.wait_for_wake();
        assert_eq!(runner.take_events(8), (vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }], false));
    }

    #[test]
    fn the_worker_diffs_a_reload_against_the_previous_rows() {
        let file = |name: &str| Entry::new(EntryName::new(name).unwrap(), dual_pane_domain::EntryKind::File);
        let listed: Arc<[Entry]> = Arc::from(vec![file("a"), file("b"), file("c")]);
        let source_rows = Arc::clone(&listed);
        let (mut runner, wakes) = runtime(source_factory(move |_, _| Some(Ok(Arc::clone(&source_rows)))));
        let previous: Arc<[Entry]> = Arc::from(vec![file("a"), file("c")]);
        let reload = WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), location: location("folder"), sort: SortSpec::default(), previous: Some(previous) };
        assert_eq!(runner.dispatch(reload), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::clone(&listed), changes: Some(vec![RowChange { row: 1, removed: 0, inserted: 1 }]) }]);
        assert_eq!(runner.dispatch(read(1, "folder")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), entries: listed, changes: None }]);
    }

    #[test]
    fn probes_run_off_the_calling_thread_and_survive_a_panicking_probe() {
        let caller = thread::current().id();
        let probe: LocationProbe = Box::new(move |location| {
            assert_ne!(thread::current().id(), caller, "a probe ran on the dispatching thread");
            if *location == self::location("panics") {
                panic!("probe failure");
            }
            if *location == self::location("present") { FavoriteProbeOutcome::Available } else { FavoriteProbeOutcome::Unavailable }
        });
        let (mut runner, wakes) = runtime_with_probe(source_factory(|_, _| None), probe, 1);
        let mut events = Vec::new();
        for name in ["panics", "present", "absent"] {
            assert_eq!(runner.dispatch(WorkRequest::ProbeScreenshotsFolder { location: location(name) }), None);
        }
        while events.len() < 3 {
            events.extend(wait(&mut runner, &wakes));
        }
        let outcome = |location| Event::ScreenshotsFolderProbed { location, outcome: FavoriteProbeOutcome::Failed };
        assert_eq!(events, vec![outcome(location("panics")), Event::ScreenshotsFolderProbed { location: location("present"), outcome: FavoriteProbeOutcome::Available }, Event::ScreenshotsFolderProbed { location: location("absent"), outcome: FavoriteProbeOutcome::Unavailable }]);
    }

    #[test]
    fn favorite_target_probes_share_the_probe_lane() {
        let probe: LocationProbe = Box::new(|location| if *location == self::location("present") { FavoriteProbeOutcome::Available } else { FavoriteProbeOutcome::Unavailable });
        let (mut runner, wakes) = runtime_with_probe(source_factory(|_, _| None), probe, 1);
        assert_eq!(runner.dispatch(WorkRequest::ProbeFavoriteTarget { item_id: 7, target: location("missing") }), None);
        assert_eq!(runner.dispatch(WorkRequest::ProbeFavoriteTarget { item_id: 8, target: location("present") }), None);
        let mut events = Vec::new();
        while events.len() < 2 {
            events.extend(wait(&mut runner, &wakes));
        }
        assert_eq!(events, vec![Event::FavoriteTargetProbed { item_id: 7, target: location("missing"), outcome: FavoriteProbeOutcome::Unavailable }, Event::FavoriteTargetProbed { item_id: 8, target: location("present"), outcome: FavoriteProbeOutcome::Available }]);
        runner.probes = mpsc::sync_channel(1).0;
        assert_eq!(runner.dispatch(WorkRequest::ProbeFavoriteTarget { item_id: 9, target: location("any") }), Some(Event::FavoriteTargetProbed { item_id: 9, target: location("any"), outcome: FavoriteProbeOutcome::Failed }), "a stopped lane fails, which removes nothing");
    }

    #[test]
    fn a_stopped_probe_lane_fails_a_probe_at_once() {
        let (mut runner, _wakes) = runtime(source_factory(|_, _| None));
        runner.probes = mpsc::sync_channel(1).0;
        assert_eq!(runner.dispatch(WorkRequest::ProbeScreenshotsFolder { location: location("any") }), Some(Event::ScreenshotsFolderProbed { location: location("any"), outcome: FavoriteProbeOutcome::Failed }));
    }

    #[test]
    fn a_full_probe_queue_fails_further_probes_at_once() {
        let (started, wait_started) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let probe: LocationProbe = Box::new(move |_| {
            started.send(()).ok();
            gate.lock().unwrap().recv_timeout(TIMEOUT).ok();
            FavoriteProbeOutcome::Available
        });
        let (mut runner, wakes) = runtime_with_probe(source_factory(|_, _| None), probe, 1);
        assert_eq!(runner.dispatch(WorkRequest::ProbeScreenshotsFolder { location: location("running") }), None);
        wait_started.recv_timeout(TIMEOUT).unwrap();
        for index in 0..PENDING_PROBE_CAPACITY {
            assert_eq!(runner.dispatch(WorkRequest::ProbeScreenshotsFolder { location: location(&format!("queued-{index}")) }), None);
        }
        assert_eq!(runner.dispatch(WorkRequest::ProbeScreenshotsFolder { location: location("overflow") }), Some(Event::ScreenshotsFolderProbed { location: location("overflow"), outcome: FavoriteProbeOutcome::Failed }));
        for _ in 0..=PENDING_PROBE_CAPACITY {
            release.send(()).unwrap();
        }
        let mut delivered = 0;
        while delivered <= PENDING_PROBE_CAPACITY {
            delivered += wait(&mut runner, &wakes).len();
        }
        assert_eq!(delivered, PENDING_PROBE_CAPACITY + 1);
    }

    #[test]
    fn blocked_left_read_does_not_delay_right_read() {
        let (mut runner, probe) = start_probe("blocked");
        assert_eq!(runner.dispatch(read(0, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(browser_read(BrowserSide::Right, 1, "right")), None);
        assert_eq!(probe.next_call().0, location("right"));
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        probe.gate.send(()).unwrap();
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }]);
    }

    #[test]
    fn wrong_browser_or_tab_cancellation_cannot_claim_a_read() {
        let (mut runner, probe) = start_probe("blocked");
        runner.dispatch(read(0, "blocked"));
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Right, tab: TabId::new(0), token: token(0) }), None);
        assert!(!runner.outstanding.get(&token(0)).unwrap().2.cancelled.load(Ordering::Relaxed));
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(1), token: token(0) }), None);
        assert!(!runner.outstanding.get(&token(0)).unwrap().2.cancelled.load(Ordering::Relaxed));
        runner.dispatch(browser_read(BrowserSide::Right, 1, "right"));
        assert_eq!(probe.next_call().0, location("right"));
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        probe.gate.send(()).unwrap();
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }]);
    }

    #[test]
    fn cancelled_left_result_is_discarded_while_right_remains_usable() {
        let (mut runner, probe) = start_probe("blocked");
        runner.dispatch(read(0, "blocked"));
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) }));
        runner.dispatch(read(1, "replacement"));
        runner.dispatch(browser_read(BrowserSide::Right, 2, "right"));
        assert_eq!(probe.next_call().0, location("right"));
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(2), token: token(2), entries: Arc::from([]), changes: None }]);
        probe.gate.send(()).unwrap();
        assert_eq!(probe.next_call().0, location("replacement"));
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        assert!(runner.take_events(8).0.is_empty());
    }

    #[test]
    fn failure_and_panic_recovery_stay_in_their_browser() {
        let (gate, release) = mpsc::channel::<()>();
        let release = Arc::new(Mutex::new(release));
        let (started_sender, started) = mpsc::channel();
        let source_factory = source_factory(move |location, _| {
            if *location == self::location("right-blocked") {
                started_sender.send(()).unwrap();
                release.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
                Some(Ok(Arc::from([])))
            } else if *location == self::location("left-failed") {
                Some(Err(ListingErrorKind::PermissionDenied))
            } else if *location == self::location("left-panic") {
                panic!("worker failed")
            } else {
                Some(Ok(Arc::from([])))
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        runner.dispatch(browser_read(BrowserSide::Right, 0, "right-blocked"));
        started.recv_timeout(TIMEOUT).unwrap();
        runner.dispatch(read(1, "left-failed"));
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), kind: ListingErrorKind::PermissionDenied }]);
        runner.dispatch(read(2, "left-panic"));
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(2), token: token(2), kind: ListingErrorKind::Internal }]);
        runner.dispatch(read(3, "left-recovered"));
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(3), token: token(3), entries: Arc::from([]), changes: None }]);
        gate.send(()).unwrap();
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }]);
    }

    #[test]
    fn startup_closes_first_queue_when_second_supervisor_fails() {
        let (observed, closed) = mpsc::channel();
        let result = Runtime::start_with(source_factory(|_, _| Some(Ok(Arc::from([])))), unused_probe(), Box::new(|| {}), READER_LANES_PER_BROWSER, move |browser, queue, _, _| {
            if browser == BrowserSide::Right {
                return Err(io::Error::other("right supervisor unavailable"));
            }
            let observed = observed.clone();
            thread::spawn(move || {
                for _ in queue {}
                observed.send(()).ok();
            });
            Ok(())
        });
        assert!(result.is_err());
        closed.recv_timeout(TIMEOUT).unwrap();
    }

    #[test]
    fn a_cancelled_queued_read_never_runs() {
        let (mut runner, probe) = start_probe("blocked");
        assert_eq!(runner.dispatch(read(0, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(read(1, "queued")), None);
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }));
        probe.gate.send(()).unwrap();
        assert_eq!(runner.dispatch(read(2, "later")), None);
        assert_eq!(probe.next_call().0, location("later"));

        let mut delivered = Vec::new();
        while delivered.len() < 2 {
            probe.wait_for_wake();
            delivered.extend(runner.take_events(8).0);
        }
        assert_eq!(loaded_tokens(&delivered), vec![token(0), token(2)]);
        assert!(probe.calls.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn repeated_navigation_keeps_only_the_latest_pending_read() {
        let (mut runner, probe) = start_probe("blocked");
        assert_eq!(runner.dispatch(read(0, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        for index in 1..=20 {
            if index > 1 {
                assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new((index - 1) as u64), token: token(index - 1) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new((index - 1) as u64), token: token(index - 1) }));
            }
            assert_eq!(runner.dispatch(read(index, &format!("item {index}"))), None);
        }
        assert_eq!(runner.outstanding.len(), 2);
        probe.gate.send(()).unwrap();
        assert_eq!(probe.next_call().0, location("item 20"));
        let mut delivered = Vec::new();
        while delivered.len() < 2 {
            probe.wait_for_wake();
            delivered.extend(runner.take_events(8).0);
        }
        assert_eq!(loaded_tokens(&delivered), vec![token(0), token(20)]);
        assert!(probe.calls.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn queued_reads_for_distinct_tabs_are_admitted() {
        let (mut runner, probe) = start_probe("blocked");
        runner.dispatch(read(0, "blocked"));
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(read(1, "pending")), None);
        assert_eq!(runner.dispatch(read(2, "extra")), None);
        assert_eq!(runner.outstanding.len(), 3);
        probe.gate.send(()).unwrap();
        assert_eq!(probe.next_call().0, location("pending"));
        assert_eq!(probe.next_call().0, location("extra"));
    }

    #[test]
    fn delivers_results_in_completion_order_and_bounded_slices() {
        let (mut runner, probe) = start_probe("blocked");
        for (index, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            assert_eq!(runner.dispatch(read(index, name)), None);
            assert_eq!(probe.next_call().0, location(name));
        }
        assert_eq!(runner.dispatch(read(5, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));

        let (first, more) = runner.take_events(2);
        assert_eq!(loaded_tokens(&first), vec![token(0), token(1)]);
        assert!(more);
        let (rest, more) = runner.take_events(8);
        assert_eq!(loaded_tokens(&rest), vec![token(2), token(3), token(4)]);
        assert!(!more);
        probe.gate.send(()).unwrap();
    }

    #[test]
    fn many_results_cause_one_pending_wake() {
        let (mut runner, probe) = start_probe("blocked");
        for index in 0..SINGLE_LANE_CAPACITY - 1 {
            assert_eq!(runner.dispatch(read(index, &format!("item {index}"))), None);
            assert_eq!(probe.next_call().0, location(&format!("item {index}")));
        }
        assert_eq!(runner.dispatch(read(SINGLE_LANE_CAPACITY - 1, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(probe.wake_count.load(Ordering::SeqCst), 1);

        runner.take_events(64);
        probe.gate.send(()).unwrap();
        probe.wait_for_wake();
        probe.wait_for_wake();
        assert_eq!(probe.wake_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn dropping_the_runtime_cancels_outstanding_reads() {
        let (mut runner, probe) = start_probe("blocked");
        assert_eq!(runner.dispatch(read(0, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(read(1, "queued")), None);
        drop(runner);
        probe.gate.send(()).unwrap();
        assert!(probe.calls.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn cancellation_is_a_terminal_event_for_queued_and_running_jobs() {
        let (started_sender, started) = mpsc::channel();
        let source_factory = source_factory(move |location, cancelled| {
            if *location == self::location("running") {
                started_sender.send(()).unwrap();
                while !cancelled.load(Ordering::Relaxed) {
                    thread::park_timeout(Duration::from_millis(1));
                }
            }
            None
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "running")), None);
        started.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(runner.dispatch(read(1, "queued")), None);
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }));
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) }));
        assert!(wakes.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn a_source_reported_cancellation_is_terminal() {
        let (mut runner, wakes) = runtime(source_factory(|_, _| None));
        assert_eq!(runner.dispatch(read(0, "stopped")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) }]);
    }

    #[test]
    fn cancelling_after_a_result_is_queued_does_not_duplicate_its_terminal_event() {
        let (mut runner, wakes) = runtime(source_factory(|_, _| Some(Ok(Arc::from([])))));
        assert_eq!(runner.dispatch(read(0, "complete")), None);
        wakes.recv_timeout(TIMEOUT).unwrap();

        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) }), None);

        assert_eq!(runner.take_events(32), (vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }], false));
    }

    #[test]
    fn a_closed_supervisor_queue_fails_an_accepted_read() {
        let (jobs, queue) = job_queue();
        drop(queue);
        let (_event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let mut runner = Runtime { left_jobs: jobs, right_jobs: job_queue().0, probes: mpsc::sync_channel(1).0, events, next_event: None, wake_pending, outstanding: HashMap::new(), read_capacity: SINGLE_LANE_CAPACITY };

        assert_eq!(runner.dispatch(read(0, "unavailable")), Some(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), kind: ListingErrorKind::Internal }));
        assert!(runner.outstanding.is_empty());
    }

    #[test]
    fn undelivered_results_count_against_read_admission() {
        assert_undelivered_results_count_against_read_admission(1);
        assert_undelivered_results_count_against_read_admission(READER_LANES_PER_BROWSER);
    }

    fn assert_undelivered_results_count_against_read_admission(lanes: usize) {
        let capacity = PENDING_READ_CAPACITY + lanes;
        let (mut runner, wakes) = runtime_with_lanes(source_factory(|_, _| Some(Ok(Arc::from([])))), lanes);
        for index in 0..capacity {
            let request = read(index, &format!("item-{index}"));
            loop {
                if runner.dispatch(request.clone()).is_none() {
                    break;
                }
                thread::yield_now();
            }
        }
        assert_eq!(runner.outstanding.len(), capacity);
        assert_eq!(runner.dispatch(read(capacity, "overflow")), Some(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(capacity as u64), token: token(capacity), kind: ListingErrorKind::Busy }));
        assert_eq!(runner.dispatch(browser_read(BrowserSide::Right, capacity + 1, "right")), None);
        wakes.recv_timeout(TIMEOUT).unwrap();
        let drained = runner.take_events(1).0;
        assert_eq!(drained.len(), 1);
        assert_eq!(runner.outstanding.len(), capacity);
        let deadline = Instant::now() + TIMEOUT;
        while !runner.outstanding.is_empty() && Instant::now() < deadline {
            runner.take_events(2 * capacity);
            thread::yield_now();
        }
        assert!(runner.outstanding.is_empty());
        assert_eq!(runner.dispatch(read(capacity + 2, "admitted")), None);
    }

    #[test]
    fn worker_start_failures_do_not_stop_later_work() {
        let (jobs, queue) = mpsc::channel();
        for index in 0..3 {
            let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
            jobs.send(Job { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), location: location("item"), sort: SortSpec::default(), previous: None, state }).unwrap();
        }
        drop(jobs);
        let source_factory = source_factory(|_, _| Some(Ok(Arc::from([]))));
        let (event_sender, events) = mpsc::channel();
        let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Arc::new(Mutex::new(Box::new(|| {}))) };
        let mut spawn_count = 0;

        supervise_with(
            queue,
            source_factory,
            delivery,
            move |worker| {
                spawn_count += 1;
                if spawn_count == 1 {
                    Err(io::Error::other("injected worker spawn failure"))
                } else if spawn_count == 2 {
                    drop(worker);
                    Ok(())
                } else {
                    worker();
                    Ok(())
                }
            },
            |_| {},
        );

        assert_eq!(events.into_iter().collect::<Vec<_>>(), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), kind: ListingErrorKind::Internal }, Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), kind: ListingErrorKind::Internal }, Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(2), token: token(2), entries: Arc::from([]), changes: None },]);
    }

    #[test]
    fn panic_fails_then_a_replacement_worker_processes_fifo_work() {
        let created = Arc::new(AtomicUsize::new(0));
        let source_factory: FolderItemsSourceFactory = Arc::new({
            let created = Arc::clone(&created);
            move || {
                let instance = created.fetch_add(1, Ordering::SeqCst);
                Box::new(move |_, _, _| {
                    if instance == 0 {
                        panic!("secret")
                    }
                    Some(Ok(Arc::from([])))
                })
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(runner.dispatch(read(1, "after")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        assert_eq!(created.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn source_creation_is_inside_the_worker_panic_boundary() {
        let created = Arc::new(AtomicUsize::new(0));
        let source_factory: FolderItemsSourceFactory = Arc::new({
            let created = Arc::clone(&created);
            move || {
                if created.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("factory secret")
                }
                Box::new(|_, _, _| Some(Ok(Arc::from([]))))
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(runner.dispatch(read(1, "after")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        assert_eq!(created.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_panicking_payload_destructor_disconnects_only_its_worker() {
        struct PanicsOnDrop;
        impl Drop for PanicsOnDrop {
            fn drop(&mut self) {
                panic!("payload destructor")
            }
        }

        let created = Arc::new(AtomicUsize::new(0));
        let source_factory: FolderItemsSourceFactory = Arc::new({
            let created = Arc::clone(&created);
            move || {
                let instance = created.fetch_add(1, Ordering::SeqCst);
                Box::new(move |_, _, _| {
                    if instance == 0 {
                        panic::panic_any(PanicsOnDrop);
                    }
                    Some(Ok(Arc::from([])))
                })
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(runner.dispatch(read(1, "after")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        assert_eq!(created.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn cancellation_during_restart_backoff_is_terminal() {
        let source_factory = source_factory(|_, _| panic!("worker failure"));
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(runner.dispatch(read(1, "cancelled")), None);
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }));
    }

    #[test]
    fn restart_delays_increase_cap_and_reset_after_recovery() {
        let (jobs, queue) = mpsc::channel();
        for index in 0..11 {
            let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
            jobs.send(Job { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), location: location("item"), sort: SortSpec::default(), previous: None, state }).unwrap();
        }
        drop(jobs);
        let source_factory = source_factory(|_, _| Some(Ok(Arc::from([]))));
        let (event_sender, events) = mpsc::channel();
        let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Arc::new(Mutex::new(Box::new(|| {}))) };
        let mut spawn_count = 0;
        let mut delays = Vec::new();

        supervise_with(
            queue,
            source_factory,
            delivery,
            |worker| {
                spawn_count += 1;
                if spawn_count <= 8 || spawn_count == 10 {
                    Err(io::Error::other("injected worker spawn failure"))
                } else {
                    worker();
                    Ok(())
                }
            },
            |delay| delays.push(delay),
        );

        assert_eq!(delays, vec![Duration::from_millis(100), Duration::from_millis(200), Duration::from_millis(400), Duration::from_millis(800), Duration::from_millis(1_600), Duration::from_millis(3_200), MAX_RESTART_DELAY, MAX_RESTART_DELAY, INITIAL_RESTART_DELAY,]);
        let events = events.into_iter().collect::<Vec<_>>();
        assert_eq!(events.len(), 11);
        assert_eq!(event_address(&events[8]).map(|address| address.2), Some(token(8)));
        assert!(matches!(events[8], Event::FolderItemsLoaded { .. }));
        assert_eq!(event_address(&events[9]).map(|address| address.2), Some(token(9)));
        assert!(matches!(events[9], Event::FolderItemsFailed { kind: ListingErrorKind::Internal, .. }));
        assert_eq!(event_address(&events[10]).map(|address| address.2), Some(token(10)));
        assert!(matches!(events[10], Event::FolderItemsLoaded { .. }));
    }

    #[test]
    fn a_blocked_tab_does_not_delay_another_tab_in_the_same_browser() {
        let (mut runner, probe) = start_probe_with_lanes("blocked", READER_LANES_PER_BROWSER);
        assert_eq!(runner.dispatch(read(0, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(read(1, "other")), None);
        assert_eq!(probe.next_call().0, location("other"));
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1), entries: Arc::from([]), changes: None }]);
        probe.gate.send(()).unwrap();
        assert_eq!(wait(&mut runner, &probe.wakes), vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }]);
    }

    #[test]
    fn reads_queue_when_every_lane_is_blocked_and_a_cancelled_one_never_runs() {
        let lanes = 2;
        let (mut runner, probe) = start_probe_with_lanes("blocked", lanes);
        for index in 0..lanes {
            assert_eq!(runner.dispatch(read(index, "blocked")), None);
            assert_eq!(probe.next_call().0, location("blocked"));
        }
        assert_eq!(runner.dispatch(read(lanes, "queued")), None);
        assert!(probe.calls.recv_timeout(Duration::from_millis(100)).is_err());
        assert_eq!(runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(lanes as u64), token: token(lanes) }), Some(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(lanes as u64), token: token(lanes) }));
        for _ in 0..lanes {
            probe.gate.send(()).unwrap();
        }
        assert_eq!(runner.dispatch(read(lanes + 1, "later")), None);
        assert_eq!(probe.next_call().0, location("later"));
        let mut delivered = Vec::new();
        let deadline = Instant::now() + TIMEOUT;
        while delivered.len() < lanes + 1 && Instant::now() < deadline {
            delivered.extend(runner.take_events(8).0);
            thread::yield_now();
        }
        let mut tokens = loaded_tokens(&delivered);
        tokens.sort_by_key(|token| (0..=lanes + 1).position(|index| self::token(index) == *token));
        assert_eq!(tokens, (0..lanes).map(token).chain([token(lanes + 1)]).collect::<Vec<_>>());
        assert!(probe.calls.recv_timeout(Duration::from_millis(100)).is_err());
    }
}
