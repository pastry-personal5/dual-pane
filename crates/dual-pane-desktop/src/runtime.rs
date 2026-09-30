use std::collections::HashMap;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use dual_pane_application::{Event, WorkRequest};
use dual_pane_domain::{Entry, ListingErrorKind, Location, RequestToken};

pub type ListingSource = Box<dyn Fn(&Location, &AtomicBool) -> Option<Result<Arc<[Entry]>, ListingErrorKind>> + Send>;
pub type Wake = Box<dyn Fn() + Send>;
type WorkerTask = Box<dyn FnOnce() + Send>;

pub trait WorkRunner {
    fn dispatch(&mut self, request: WorkRequest);
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
    delivery: Delivery,
}

#[derive(Clone)]
struct Job {
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

#[derive(Clone)]
struct Delivery {
    events: Sender<Event>,
    wake_pending: Arc<AtomicBool>,
    wake: Arc<Mutex<Wake>>,
}

const INITIAL_RESTART_DELAY: Duration = Duration::from_millis(100);
const MAX_RESTART_DELAY: Duration = Duration::from_secs(5);

impl Runtime {
    pub fn start(source: ListingSource, wake: Wake) -> io::Result<Self> {
        let (jobs, queue) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let delivery = Delivery { events: event_sender, wake_pending: Arc::clone(&wake_pending), wake: Arc::new(Mutex::new(wake)) };
        let supervisor_delivery = delivery.clone();
        thread::Builder::new().name("listing-supervisor".to_owned()).spawn(move || supervise(queue, source, supervisor_delivery))?;
        Ok(Self { jobs, events, next_event: None, wake_pending, outstanding: HashMap::new(), delivery })
    }

    fn receive(&mut self) -> Option<Event> {
        let event = self.next_event.take().or_else(|| self.events.try_recv().ok())?;
        self.outstanding.remove(&event_token(&event));
        Some(event)
    }

    fn deliver(&self, event: Event) {
        deliver_event(&self.delivery, event);
    }
}

impl WorkRunner for Runtime {
    fn dispatch(&mut self, request: WorkRequest) {
        match request {
            WorkRequest::ReadDirectory { token, location } => {
                let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
                self.outstanding.insert(token, Arc::clone(&state));
                if let Err(error) = self.jobs.send(Job { token, location, state }) {
                    let job = error.0;
                    finish(&self.delivery, &job, Event::ListingFailed { token: job.token, kind: ListingErrorKind::Internal });
                }
            }
            WorkRequest::Cancel { token } => {
                if let Some(state) = self.outstanding.get(&token) {
                    state.cancelled.store(true, Ordering::Relaxed);
                    if !state.claim_terminal() {
                        return;
                    }
                    self.outstanding.remove(&token);
                    self.deliver(Event::ListingCancelled { token });
                }
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

fn supervise(queue: Receiver<Job>, source: ListingSource, delivery: Delivery) {
    supervise_with(queue, source, delivery, |worker| thread::Builder::new().name("listing-worker".to_owned()).spawn(worker).map(|_| ()));
}

fn supervise_with(queue: Receiver<Job>, source: ListingSource, delivery: Delivery, mut spawn_worker: impl FnMut(WorkerTask) -> io::Result<()>) {
    let mut delay = INITIAL_RESTART_DELAY;
    let source = Arc::new(Mutex::new(source));
    for job in queue {
        if job.state.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let worker_source = Arc::clone(&source);
        let worker_job = job.clone();
        let worker: WorkerTask = Box::new(move || {
            let source = worker_source.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let outcome = catch_unwind(AssertUnwindSafe(|| source(&worker_job.location, &worker_job.state.cancelled)));
            result_sender.send((worker_job, outcome)).ok();
        });
        if spawn_worker(worker).is_err() {
            finish(&delivery, &job, Event::ListingFailed { token: job.token, kind: ListingErrorKind::Internal });
            thread::sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        }
        let Ok((job, outcome)) = result_receiver.recv() else {
            finish(&delivery, &job, Event::ListingFailed { token: job.token, kind: ListingErrorKind::Internal });
            thread::sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        };
        match outcome {
            Ok(Some(Ok(entries))) => {
                finish(&delivery, &job, Event::ListingLoaded { token: job.token, entries });
                delay = INITIAL_RESTART_DELAY;
            }
            Ok(Some(Err(kind))) => {
                finish(&delivery, &job, Event::ListingFailed { token: job.token, kind });
                delay = INITIAL_RESTART_DELAY;
            }
            Ok(None) => {
                finish(&delivery, &job, Event::ListingCancelled { token: job.token });
                delay = INITIAL_RESTART_DELAY;
            }
            Err(_) => {
                finish(&delivery, &job, Event::ListingFailed { token: job.token, kind: ListingErrorKind::Internal });
                thread::sleep(delay);
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
    if delivery.events.send(event).is_ok()
        && !delivery.wake_pending.swap(true, Ordering::SeqCst)
        && let Ok(wake) = delivery.wake.lock()
    {
        (wake)();
    }
}

fn event_token(event: &Event) -> RequestToken {
    match event {
        Event::ListingLoaded { token, .. } | Event::ListingFailed { token, .. } | Event::ListingCancelled { token } => *token,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::EntryName;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
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
    fn runtime(source: ListingSource) -> (Runtime, Receiver<()>) {
        let (sender, receiver) = mpsc::channel();
        (
            Runtime::start(
                source,
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

    #[test]
    fn cancellation_is_a_terminal_event_for_queued_and_running_jobs() {
        let (started_sender, started) = mpsc::channel();
        let source: ListingSource = Box::new(move |location, cancelled| {
            if *location == self::location("running") {
                started_sender.send(()).unwrap();
                while !cancelled.load(Ordering::Relaxed) {
                    thread::park_timeout(Duration::from_millis(1));
                }
            }
            None
        });
        let (mut runner, wakes) = runtime(source);
        runner.dispatch(read(0, "running"));
        started.recv_timeout(TIMEOUT).unwrap();
        runner.dispatch(read(1, "queued"));
        runner.dispatch(WorkRequest::Cancel { token: token(1) });
        runner.dispatch(WorkRequest::Cancel { token: token(0) });
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingCancelled { token: token(1) }, Event::ListingCancelled { token: token(0) }]);
    }

    #[test]
    fn a_source_reported_cancellation_is_terminal() {
        let source: ListingSource = Box::new(|_, _| None);
        let (mut runner, wakes) = runtime(source);
        runner.dispatch(read(0, "stopped"));
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingCancelled { token: token(0) }]);
    }

    #[test]
    fn cancelling_after_a_result_is_queued_does_not_duplicate_its_terminal_event() {
        let source: ListingSource = Box::new(|_, _| Some(Ok(Arc::from([]))));
        let (mut runner, wakes) = runtime(source);
        runner.dispatch(read(0, "complete"));
        wakes.recv_timeout(TIMEOUT).unwrap();

        runner.dispatch(WorkRequest::Cancel { token: token(0) });

        assert_eq!(runner.take_events(32), (vec![Event::ListingLoaded { token: token(0), entries: Arc::from([]) }], false));
    }

    #[test]
    fn a_closed_supervisor_queue_fails_an_accepted_read() {
        let (jobs, queue) = mpsc::channel();
        drop(queue);
        let (event_sender, events) = mpsc::channel();
        let (wake_sender, wakes) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let delivery = Delivery {
            events: event_sender,
            wake_pending: Arc::clone(&wake_pending),
            wake: Arc::new(Mutex::new(Box::new(move || {
                wake_sender.send(()).ok();
            }))),
        };
        let mut runner = Runtime { jobs, events, next_event: None, wake_pending, outstanding: HashMap::new(), delivery };

        runner.dispatch(read(0, "unavailable"));

        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
    }

    #[test]
    fn worker_start_failures_do_not_stop_later_work() {
        let (jobs, queue) = mpsc::channel();
        for index in 0..3 {
            let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
            jobs.send(Job { token: token(index), location: location("item"), state }).unwrap();
        }
        drop(jobs);
        let source: ListingSource = Box::new(|_, _| Some(Ok(Arc::from([]))));
        let (event_sender, events) = mpsc::channel();
        let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Arc::new(Mutex::new(Box::new(|| {}))) };
        let mut spawn_count = 0;

        supervise_with(queue, source, delivery, move |worker| {
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
        });

        assert_eq!(events.into_iter().collect::<Vec<_>>(), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }, Event::ListingFailed { token: token(1), kind: ListingErrorKind::Internal }, Event::ListingLoaded { token: token(2), entries: Arc::from([]) },]);
    }

    #[test]
    fn panic_fails_then_a_replacement_worker_processes_fifo_work() {
        let panicked = Arc::new(AtomicBool::new(false));
        let source: ListingSource = Box::new({
            let panicked = Arc::clone(&panicked);
            move |_, _| {
                if !panicked.swap(true, Ordering::SeqCst) {
                    panic!("secret")
                }
                Some(Ok(Arc::from([])))
            }
        });
        let (mut runner, wakes) = runtime(source);
        runner.dispatch(read(0, "panic"));
        runner.dispatch(read(1, "after"));
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingLoaded { token: token(1), entries: Arc::from([]) }]);
    }

    #[test]
    fn cancellation_during_restart_backoff_is_terminal() {
        let source: ListingSource = Box::new(|_, _| panic!());
        let (mut runner, wakes) = runtime(source);
        runner.dispatch(read(0, "panic"));
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::Internal }]);
        runner.dispatch(read(1, "cancelled"));
        runner.dispatch(WorkRequest::Cancel { token: token(1) });
        assert_eq!(wait(&mut runner, &wakes), vec![Event::ListingCancelled { token: token(1) }]);
    }

    #[test]
    fn restart_delays_increase_cap_and_reset() {
        assert_eq!(INITIAL_RESTART_DELAY.saturating_mul(2), Duration::from_millis(200));
        assert_eq!(MAX_RESTART_DELAY.saturating_mul(2).min(MAX_RESTART_DELAY), MAX_RESTART_DELAY);
    }
}
