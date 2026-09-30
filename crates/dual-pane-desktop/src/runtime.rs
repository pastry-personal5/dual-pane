use std::collections::HashMap;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Once};
use std::thread;
use std::time::Duration;

use dual_pane_application::{Event, WorkRequest};
use dual_pane_domain::{Entry, ListingErrorKind, Location, PaneSide, RequestToken};

type ListingOutcome = Option<Result<Arc<[Entry]>, ListingErrorKind>>;
pub type ListingSource = Box<dyn Fn(&Location, &AtomicBool) -> ListingOutcome + Send>;
pub type ListingSourceFactory = Arc<dyn Fn() -> ListingSource + Send + Sync>;
pub type Wake = Box<dyn Fn() + Send>;
type WorkerTask = Box<dyn FnOnce() + Send>;

pub trait WorkRunner {
    /// Dispatches outside work without waiting. A terminal event is returned
    /// directly when the request can finish before leaving the GUI thread.
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event>;
    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool);
}

/// A supervised single-worker runtime. Each worker owns one job; a panic is
/// contained, reported safely, and followed by a replacement after backoff.
pub struct Runtime {
    jobs: Sender<Job>,
    events: Receiver<Event>,
    next_event: Option<Event>,
    wake_pending: Arc<AtomicBool>,
    outstanding: HashMap<RequestToken, Arc<JobState>>,
}

#[derive(Clone)]
struct Job {
    pane: PaneSide,
    token: RequestToken,
    location: Location,
    state: Arc<JobState>,
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

struct Delivery {
    events: Sender<Event>,
    wake_pending: Arc<AtomicBool>,
    wake: Wake,
}

enum WorkerOutcome {
    Completed(ListingOutcome),
    Panicked,
}

const WORKER_THREAD_NAME: &str = "listing-worker";
const INITIAL_RESTART_DELAY: Duration = Duration::from_millis(100);
const MAX_RESTART_DELAY: Duration = Duration::from_secs(5);
static INSTALL_PANIC_HOOK: Once = Once::new();

impl Runtime {
    pub fn start(source_factory: ListingSourceFactory, wake: Wake) -> io::Result<Self> {
        install_panic_hook();
        let (jobs, queue) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let delivery = Delivery { events: event_sender, wake_pending: Arc::clone(&wake_pending), wake };
        thread::Builder::new().name("listing-supervisor".to_owned()).spawn(move || supervise(queue, source_factory, delivery))?;
        Ok(Self { jobs, events, next_event: None, wake_pending, outstanding: HashMap::new() })
    }

    fn receive(&mut self) -> Option<Event> {
        let event = self.next_event.take().or_else(|| self.events.try_recv().ok())?;
        self.outstanding.remove(&event_token(&event));
        Some(event)
    }
}

fn install_panic_hook() {
    INSTALL_PANIC_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |information| {
            if thread::current().name() == Some(WORKER_THREAD_NAME) {
                let mut standard_error = io::stderr();
                standard_error.write_all(b"Dual Pane listing worker stopped unexpectedly.\n").ok();
            } else {
                previous(information);
            }
        }));
    });
}

impl WorkRunner for Runtime {
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
        match request {
            WorkRequest::ReadDirectory { pane, token, location } => {
                let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
                self.outstanding.insert(token, Arc::clone(&state));
                if let Err(error) = self.jobs.send(Job { pane, token, location, state }) {
                    let job = error.0;
                    if job.state.claim_terminal() {
                        self.outstanding.remove(&job.token);
                        return Some(Event::ListingFailed { pane: job.pane, token: job.token, kind: ListingErrorKind::Internal });
                    }
                }
                None
            }
            WorkRequest::Cancel { pane, token } => {
                if let Some(state) = self.outstanding.get(&token) {
                    state.cancelled.store(true, Ordering::Relaxed);
                    if !state.claim_terminal() {
                        return None;
                    }
                    self.outstanding.remove(&token);
                    return Some(Event::ListingCancelled { pane, token });
                }
                None
            }
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

impl Drop for Runtime {
    fn drop(&mut self) {
        for state in self.outstanding.values() {
            state.cancelled.store(true, Ordering::Relaxed);
        }
    }
}

fn supervise(queue: Receiver<Job>, source_factory: ListingSourceFactory, delivery: Delivery) {
    supervise_with(queue, source_factory, delivery, |worker| thread::Builder::new().name(WORKER_THREAD_NAME.to_owned()).spawn(worker).map(|_| ()), thread::sleep);
}

fn supervise_with(queue: Receiver<Job>, source_factory: ListingSourceFactory, delivery: Delivery, mut spawn_worker: impl FnMut(WorkerTask) -> io::Result<()>, mut sleep: impl FnMut(Duration)) {
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
                source(&worker_job.location, &worker_job.state.cancelled)
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
            finish(&delivery, &job, Event::ListingFailed { pane: job.pane, token: job.token, kind: ListingErrorKind::Internal });
            sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        }
        let Ok((job, outcome)) = result_receiver.recv() else {
            finish(&delivery, &job, Event::ListingFailed { pane: job.pane, token: job.token, kind: ListingErrorKind::Internal });
            sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        };
        match outcome {
            WorkerOutcome::Completed(Some(Ok(entries))) => {
                finish(&delivery, &job, Event::ListingLoaded { pane: job.pane, token: job.token, entries });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Completed(Some(Err(kind))) => {
                finish(&delivery, &job, Event::ListingFailed { pane: job.pane, token: job.token, kind });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Completed(None) => {
                finish(&delivery, &job, Event::ListingCancelled { pane: job.pane, token: job.token });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Panicked => {
                finish(&delivery, &job, Event::ListingFailed { pane: job.pane, token: job.token, kind: ListingErrorKind::Internal });
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
        (delivery.wake)();
    }
}

fn event_token(event: &Event) -> RequestToken {
    match event {
        Event::ListingLoaded { token, .. } | Event::ListingFailed { token, .. } | Event::ListingCancelled { token, .. } => *token,
    }
}

#[cfg(all(test, any()))]
mod tests {
    use super::*;
    use dual_pane_domain::EntryName;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    const TIMEOUT: Duration = Duration::from_secs(3);
    fn location(name: &str) -> Location {
        Location::root().join(&EntryName::new(name).unwrap())
    }
    fn token(index: usize) -> RequestToken {
        (0..index).fold(RequestToken::first(), |token, _| token.next())
    }
    fn read(index: usize, name: &str) -> WorkRequest {
        WorkRequest::ReadDirectory { token: token(index), location: location(name) }
    }
    fn source_factory(source: impl Fn(&Location, &AtomicBool) -> ListingOutcome + Send + Sync + 'static) -> ListingSourceFactory {
        let source = Arc::new(source);
        Arc::new(move || {
            let source = Arc::clone(&source);
            Box::new(move |location, cancelled| source(location, cancelled))
        })
    }
    fn runtime(source_factory: ListingSourceFactory) -> (Runtime, Receiver<()>) {
        let (sender, receiver) = mpsc::channel();
        (
            Runtime::start(
                source_factory,
                Box::new(move || {
                    sender.send(()).ok();
                }),
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
                Event::ListingLoaded { token, .. } => Some(*token),
                Event::ListingFailed { .. } | Event::ListingCancelled { .. } => None,
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
        let (call_sender, calls) = mpsc::channel();
        let (gate, gate_receiver) = mpsc::channel::<()>();
        let gate_receiver = Arc::new(Mutex::new(gate_receiver));
        let (wake_sender, wakes) = mpsc::channel();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let source_factory: ListingSourceFactory = Arc::new(move || {
            let call_sender = call_sender.clone();
            let gate_receiver = Arc::clone(&gate_receiver);
            Box::new(move |location, _cancelled| {
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
        (Runtime::start(source_factory, wake).unwrap(), Probe { calls, gate, wakes, wake_count })
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
        assert_eq!(runner.take_events(8), (vec![Event::ListingLoaded { token: token(0), entries: Arc::from([]) }], false));
    }

    #[test]
    fn a_cancelled_queued_read_never_runs() {
        let (mut runner, probe) = start_probe("blocked");
        assert_eq!(runner.dispatch(read(0, "blocked")), None);
        assert_eq!(probe.next_call().0, location("blocked"));
        assert_eq!(runner.dispatch(read(1, "queued")), None);
        assert_eq!(runner.dispatch(WorkRequest::Cancel { token: token(1) }), Some(Event::ListingCancelled { token: token(1) }));
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
    fn delivers_results_in_completion_order_and_bounded_slices() {
        let (mut runner, probe) = start_probe("blocked");
        for (index, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            assert_eq!(runner.dispatch(read(index, name)), None);
        }
        assert_eq!(runner.dispatch(read(5, "blocked")), None);
        while probe.next_call().0 != location("blocked") {}

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
        for index in 0..20 {
            assert_eq!(runner.dispatch(read(index, &format!("item {index}"))), None);
        }
        assert_eq!(runner.dispatch(read(20, "blocked")), None);
        while probe.next_call().0 != location("blocked") {}
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
        assert_eq!(runner.dispatch(WorkRequest::Cancel { token: token(1) }), Some(Event::ListingCancelled { token: token(1) }));
        assert_eq!(runner.dispatch(WorkRequest::Cancel { token: token(0) }), Some(Event::ListingCancelled { token: token(0) }));
        assert!(wakes.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn a_source_reported_cancellation_is_terminal() {
        let (mut runner, wakes) = runtime(source_factory(|_, _| None));
        assert_eq!(runner.dispatch(read(0, "stopped")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingCancelled { token: token(0) }]);
    }

    #[test]
    fn cancelling_after_a_result_is_queued_does_not_duplicate_its_terminal_event() {
        let (mut runner, wakes) = runtime(source_factory(|_, _| Some(Ok(Arc::from([])))));
        assert_eq!(runner.dispatch(read(0, "complete")), None);
        wakes.recv_timeout(TIMEOUT).unwrap();

        assert_eq!(runner.dispatch(WorkRequest::Cancel { token: token(0) }), None);

        assert_eq!(runner.take_events(32), (vec![Event::ListingLoaded { token: token(0), entries: Arc::from([]) }], false));
    }

    #[test]
    fn a_closed_supervisor_queue_fails_an_accepted_read() {
        let (jobs, queue) = mpsc::channel();
        drop(queue);
        let (_event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let mut runner = Runtime { jobs, events, next_event: None, wake_pending, outstanding: HashMap::new() };

        assert_eq!(runner.dispatch(read(0, "unavailable")), Some(Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }));
        assert!(runner.outstanding.is_empty());
    }

    #[test]
    fn worker_start_failures_do_not_stop_later_work() {
        let (jobs, queue) = mpsc::channel();
        for index in 0..3 {
            let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
            jobs.send(Job { token: token(index), location: location("item"), state }).unwrap();
        }
        drop(jobs);
        let source_factory = source_factory(|_, _| Some(Ok(Arc::from([]))));
        let (event_sender, events) = mpsc::channel();
        let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Box::new(|| {}) };
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

        assert_eq!(events.into_iter().collect::<Vec<_>>(), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }, Event::ListingFailed { token: token(1), kind: ListingErrorKind::Internal }, Event::ListingLoaded { token: token(2), entries: Arc::from([]) },]);
    }

    #[test]
    fn panic_fails_then_a_replacement_worker_processes_fifo_work() {
        let created = Arc::new(AtomicUsize::new(0));
        let source_factory: ListingSourceFactory = Arc::new({
            let created = Arc::clone(&created);
            move || {
                let instance = created.fetch_add(1, Ordering::SeqCst);
                Box::new(move |_, _| {
                    if instance == 0 {
                        panic!("secret")
                    }
                    Some(Ok(Arc::from([])))
                })
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(runner.dispatch(read(1, "after")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingLoaded { token: token(1), entries: Arc::from([]) }]);
        assert_eq!(created.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn source_creation_is_inside_the_worker_panic_boundary() {
        let created = Arc::new(AtomicUsize::new(0));
        let source_factory: ListingSourceFactory = Arc::new({
            let created = Arc::clone(&created);
            move || {
                if created.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("factory secret")
                }
                Box::new(|_, _| Some(Ok(Arc::from([]))))
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(runner.dispatch(read(1, "after")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingLoaded { token: token(1), entries: Arc::from([]) }]);
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
        let source_factory: ListingSourceFactory = Arc::new({
            let created = Arc::clone(&created);
            move || {
                let instance = created.fetch_add(1, Ordering::SeqCst);
                Box::new(move |_, _| {
                    if instance == 0 {
                        panic::panic_any(PanicsOnDrop);
                    }
                    Some(Ok(Arc::from([])))
                })
            }
        });
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(runner.dispatch(read(1, "after")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingLoaded { token: token(1), entries: Arc::from([]) }]);
        assert_eq!(created.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn cancellation_during_restart_backoff_is_terminal() {
        let source_factory = source_factory(|_, _| panic!("worker failure"));
        let (mut runner, wakes) = runtime(source_factory);
        assert_eq!(runner.dispatch(read(0, "panic")), None);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(runner.dispatch(read(1, "cancelled")), None);
        assert_eq!(runner.dispatch(WorkRequest::Cancel { token: token(1) }), Some(Event::ListingCancelled { token: token(1) }));
    }

    #[test]
    fn restart_delays_increase_cap_and_reset_after_recovery() {
        let (jobs, queue) = mpsc::channel();
        for index in 0..11 {
            let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
            jobs.send(Job { token: token(index), location: location("item"), state }).unwrap();
        }
        drop(jobs);
        let source_factory = source_factory(|_, _| Some(Ok(Arc::from([]))));
        let (event_sender, events) = mpsc::channel();
        let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Box::new(|| {}) };
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
        assert_eq!(event_token(&events[8]), token(8));
        assert!(matches!(events[8], Event::ListingLoaded { .. }));
        assert_eq!(event_token(&events[9]), token(9));
        assert!(matches!(events[9], Event::ListingFailed { kind: ListingErrorKind::Internal, .. }));
        assert_eq!(event_token(&events[10]), token(10));
        assert!(matches!(events[10], Event::ListingLoaded { .. }));
    }
}
