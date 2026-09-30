use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use dual_pane_application::{Event, WorkRequest};
use dual_pane_domain::{Entry, ListingErrorKind, Location, RequestToken};

/// Reads the listing of a location on a worker thread. It returns `None` once
/// the flag is set, and checks the flag at least every 1,024 entries.
pub type ListingSource = Box<dyn Fn(&Location, &AtomicBool) -> Option<Result<Arc<[Entry]>, ListingErrorKind>> + Send>;

/// Asks the GUI thread to take delivered events. Called from a worker thread.
pub type Wake = Box<dyn Fn() + Send>;

/// Carries out work requests and hands their results back as events.
pub trait WorkRunner {
    fn dispatch(&mut self, request: WorkRequest);

    /// At most `max` delivered events, oldest first, and whether more remain.
    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool);
}

/// Runs listing reads on one worker thread, in the order they were requested.
///
/// The GUI thread never waits for the worker: dropping the runtime cancels
/// every outstanding read and lets the worker exit after its current one.
pub struct Runtime {
    jobs: Sender<Job>,
    events: Receiver<Event>,
    /// An event received while checking whether more remain.
    next_event: Option<Event>,
    wake_pending: Arc<AtomicBool>,
    outstanding: HashMap<RequestToken, Arc<AtomicBool>>,
}

struct Job {
    token: RequestToken,
    location: Location,
    cancelled: Arc<AtomicBool>,
}

/// What the worker needs to deliver a result.
struct Delivery {
    events: Sender<Event>,
    wake_pending: Arc<AtomicBool>,
    wake: Wake,
}

impl Runtime {
    /// Starts the worker thread.
    pub fn start(source: ListingSource, wake: Wake) -> io::Result<Self> {
        let (jobs, job_queue) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let delivery = Delivery { events: event_sender, wake_pending: Arc::clone(&wake_pending), wake };
        thread::Builder::new().name("listing-worker".to_owned()).spawn(move || run_worker(&job_queue, &source, &delivery))?;
        Ok(Self { jobs, events, next_event: None, wake_pending, outstanding: HashMap::new() })
    }

    fn receive(&mut self) -> Option<Event> {
        let event = self.next_event.take().or_else(|| self.events.try_recv().ok())?;
        self.outstanding.remove(&event_token(&event));
        Some(event)
    }
}

impl WorkRunner for Runtime {
    fn dispatch(&mut self, request: WorkRequest) {
        match request {
            WorkRequest::ReadDirectory { token, location } => {
                let cancelled = Arc::new(AtomicBool::new(false));
                self.outstanding.insert(token, Arc::clone(&cancelled));
                // The worker only stops after the runtime is dropped, so the
                // queue is open while `self` exists.
                self.jobs.send(Job { token, location, cancelled }).ok();
            }
            WorkRequest::Cancel { token } => {
                if let Some(cancelled) = self.outstanding.remove(&token) {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
        // Clear the flag before reading, so an event sent from now on wakes
        // the GUI thread again. A swap, unlike a store, synchronizes with the
        // worker's swap, so an event it sent without waking is read below.
        self.wake_pending.swap(false, Ordering::SeqCst);
        let events: Vec<Event> = std::iter::from_fn(|| self.receive()).take(max).collect();
        if self.next_event.is_none() {
            self.next_event = self.events.try_recv().ok();
        }
        (events, self.next_event.is_some())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for cancelled in self.outstanding.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

fn run_worker(job_queue: &Receiver<Job>, source: &ListingSource, delivery: &Delivery) {
    // The loop ends when the runtime, which owns the only job sender, is dropped.
    for job in job_queue {
        if job.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let Some(result) = source(&job.location, &job.cancelled) else {
            continue;
        };
        // A read cancelled while it ran is no longer wanted.
        if job.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let event = match result {
            Ok(entries) => Event::ListingLoaded { token: job.token, entries },
            Err(kind) => Event::ListingFailed { token: job.token, kind },
        };
        if delivery.events.send(event).is_err() {
            return;
        }
        if !delivery.wake_pending.swap(true, Ordering::SeqCst) {
            (delivery.wake)();
        }
    }
}

fn event_token(event: &Event) -> RequestToken {
    match event {
        Event::ListingLoaded { token, .. } | Event::ListingFailed { token, .. } => *token,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use dual_pane_domain::EntryName;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn location(name: &str) -> Location {
        Location::root().join(&EntryName::new(name).unwrap())
    }

    fn token(index: usize) -> RequestToken {
        (0..index).fold(RequestToken::first(), |token, _| token.next())
    }

    fn read(index: usize, name: &str) -> WorkRequest {
        WorkRequest::ReadDirectory { token: token(index), location: location(name) }
    }

    fn loaded_tokens(events: &[Event]) -> Vec<RequestToken> {
        events.iter().map(event_token).collect()
    }

    /// A source that reports each call and each worker thread name. Reads of
    /// `blocked` wait until the test sends on the gate.
    struct Probe {
        calls: Receiver<(Location, Option<String>)>,
        gate: Sender<()>,
        wakes: Receiver<()>,
        wake_count: Arc<AtomicUsize>,
    }

    fn start_probe(blocked: &'static str) -> (Runtime, Probe) {
        let (call_sender, calls) = mpsc::channel();
        let (gate, gate_receiver) = mpsc::channel::<()>();
        let (wake_sender, wakes) = mpsc::channel();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let source: ListingSource = Box::new(move |location, _cancelled| {
            call_sender.send((location.clone(), thread::current().name().map(str::to_owned))).unwrap();
            if *location == self::location(blocked) {
                gate_receiver.recv_timeout(TIMEOUT).unwrap();
            }
            Some(Ok(Arc::from([])))
        });
        let counter = Arc::clone(&wake_count);
        let wake: Wake = Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            wake_sender.send(()).ok();
        });
        (Runtime::start(source, wake).unwrap(), Probe { calls, gate, wakes, wake_count })
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
        let (mut runtime, probe) = start_probe("blocked");
        runtime.dispatch(read(0, "alpha"));
        assert_eq!(probe.next_call(), (location("alpha"), Some("listing-worker".to_owned())));
        probe.wait_for_wake();
        let (events, more) = runtime.take_events(8);
        assert_eq!(events, vec![Event::ListingLoaded { token: token(0), entries: Arc::from([]) }]);
        assert!(!more);
    }

    #[test]
    fn delivers_a_failed_read_as_a_failure() {
        let (wake_sender, wakes) = mpsc::channel();
        let source: ListingSource = Box::new(|_, _| Some(Err(ListingErrorKind::PermissionDenied)));
        let mut runtime = Runtime::start(source, Box::new(move || wake_sender.send(()).unwrap())).unwrap();
        runtime.dispatch(read(0, "alpha"));
        wakes.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(runtime.take_events(8).0, vec![Event::ListingFailed { token: token(0), kind: ListingErrorKind::PermissionDenied }]);
    }

    #[test]
    fn a_cancelled_queued_read_never_runs() {
        let (mut runtime, probe) = start_probe("blocked");
        runtime.dispatch(read(0, "blocked"));
        assert_eq!(probe.next_call().0, location("blocked"));
        runtime.dispatch(read(1, "queued"));
        runtime.dispatch(WorkRequest::Cancel { token: token(1) });
        probe.gate.send(()).unwrap();
        runtime.dispatch(read(2, "later"));
        assert_eq!(probe.next_call().0, location("later"));
        probe.wait_for_wake();
        let mut delivered = Vec::new();
        loop {
            let (events, more) = runtime.take_events(8);
            delivered.extend(events);
            if delivered.len() >= 2 {
                break;
            }
            // An event sent after this take wakes again; waiting bounds the loop.
            if !more {
                probe.wait_for_wake();
            }
        }
        assert_eq!(loaded_tokens(&delivered), vec![token(0), token(2)]);
    }

    #[test]
    fn a_cancelled_running_read_delivers_nothing() {
        let (mut runtime, probe) = start_probe("blocked");
        runtime.dispatch(read(0, "blocked"));
        assert_eq!(probe.next_call().0, location("blocked"));
        runtime.dispatch(WorkRequest::Cancel { token: token(0) });
        probe.gate.send(()).unwrap();
        runtime.dispatch(read(1, "later"));
        assert_eq!(probe.next_call().0, location("later"));
        probe.wait_for_wake();
        assert_eq!(loaded_tokens(&runtime.take_events(8).0), vec![token(1)]);
    }

    #[test]
    fn passes_the_cancellation_flag_to_the_source() {
        let (started_sender, started) = mpsc::channel();
        let (wake_sender, wakes) = mpsc::channel();
        let source: ListingSource = Box::new(move |location, cancelled| {
            if *location == self::location("stoppable") {
                started_sender.send(()).unwrap();
                // Return only once cancelled, as a source checking its flag would.
                while !cancelled.load(Ordering::Relaxed) {
                    thread::park_timeout(Duration::from_millis(1));
                }
                return None;
            }
            Some(Ok(Arc::from([])))
        });
        let mut runtime = Runtime::start(source, Box::new(move || wake_sender.send(()).unwrap())).unwrap();
        runtime.dispatch(read(0, "stoppable"));
        started.recv_timeout(TIMEOUT).unwrap();
        runtime.dispatch(WorkRequest::Cancel { token: token(0) });
        runtime.dispatch(read(1, "later"));
        wakes.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(loaded_tokens(&runtime.take_events(8).0), vec![token(1)]);
    }

    #[test]
    fn delivers_results_in_completion_order_and_in_bounded_slices() {
        let (mut runtime, probe) = start_probe("blocked");
        for (index, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            runtime.dispatch(read(index, name));
        }
        runtime.dispatch(read(5, "blocked"));
        while probe.next_call().0 != location("blocked") {}
        let (first, more) = runtime.take_events(2);
        assert_eq!(loaded_tokens(&first), vec![token(0), token(1)]);
        assert!(more);
        let (rest, more) = runtime.take_events(8);
        assert_eq!(loaded_tokens(&rest), vec![token(2), token(3), token(4)]);
        assert!(!more);
        probe.gate.send(()).unwrap();
    }

    #[test]
    fn many_results_cause_one_pending_wake() {
        let (mut runtime, probe) = start_probe("blocked");
        for index in 0..20 {
            runtime.dispatch(read(index, &format!("item {index}")));
        }
        // The worker runs reads in order, so once the blocked read starts,
        // every earlier result has been sent.
        runtime.dispatch(read(20, "blocked"));
        while probe.next_call().0 != location("blocked") {}
        assert_eq!(probe.wake_count.load(Ordering::SeqCst), 1);

        runtime.take_events(64);
        probe.gate.send(()).unwrap();
        probe.wait_for_wake();
        probe.wait_for_wake();
        assert_eq!(probe.wake_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn dropping_the_runtime_cancels_outstanding_reads() {
        let (mut runtime, probe) = start_probe("blocked");
        runtime.dispatch(read(0, "blocked"));
        assert_eq!(probe.next_call().0, location("blocked"));
        runtime.dispatch(read(1, "queued"));
        drop(runtime);
        probe.gate.send(()).unwrap();
        assert!(probe.calls.recv_timeout(Duration::from_millis(200)).is_err());
    }
}
