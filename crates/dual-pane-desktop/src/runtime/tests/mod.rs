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
    WorkRequest::ReadDirectory { browser, tab: TabId::new(index as u64), token: token(index), location: location(name), sort: SortSpec::default(), visibility: Default::default(), previous: None }
}
#[test]
fn byte_progress_keeps_only_the_newest_report_per_operation_after_results() {
    let (mut runner, _) = runtime(source_factory(|_, _| None));
    let (first, second) = (OperationId::new(1), OperationId::new(2));
    let progress = |id, done| Event::OperationProgress { id, generation: 4, bytes: dual_pane_application::ByteProgress { done, total: 100 } };
    for (id, done) in [(first, 10), (first, 60), (second, 5)] {
        runner.delivery.progress.lock().unwrap().insert(id, progress(id, done));
    }
    runner.delivery.events.send(Event::JournalStatus { available: true }).unwrap();
    let (events, more) = runner.take_events(2);
    assert_eq!(events[0], Event::JournalStatus { available: true }, "results come first");
    assert_eq!(events.len(), 2);
    assert!(more, "the other operation's progress waits for the next drain");
    let (rest, more) = runner.take_events(8);
    let mut delivered = events[1..].iter().chain(&rest).cloned().collect::<Vec<_>>();
    delivered.sort_by_key(|event| match event {
        Event::OperationProgress { id, .. } => id.get(),
        _ => 0,
    });
    assert_eq!(delivered, vec![progress(first, 60), progress(second, 5)]);
    assert!(!more);
}

#[test]
fn operation_request_fails_closed_until_an_executor_exists() {
    let (mut runner, _) = runtime(source_factory(|_, _| Some(Ok(Arc::from([])))));
    let id = OperationId::new(7);
    let intent = OperationIntent::new(OperationKind::MoveToTrash, location("source"), vec![OperationTarget { name: EntryName::new("item").unwrap(), kind: dual_pane_domain::EntryKind::File }], None).unwrap();
    assert_eq!(runner.dispatch(WorkRequest::Operation(OperationEffect::Scan { id, generation: 3, intent, skipped: Arc::from([]) })), Some(Event::OperationExecutorUnavailable { id, generation: 3 }));
}

#[test]
fn unavailable_watcher_reports_degraded_status_without_blocking_reads() {
    let (mut runner, _) = runtime(source_factory(|_, _| Some(Ok(Arc::from([])))));
    runner.watcher = WatcherHandle::unavailable();
    let folder = location("folder");
    assert_eq!(runner.dispatch(WorkRequest::WatchLocation { location: folder.clone(), generation: 9 }), Some(Event::WatcherReady { location: folder, generation: 9, status: dual_pane_application::MonitoringStatus::Unavailable }));
    assert_eq!(runner.dispatch(read(0, "folder")), None);
}
#[test]
fn an_attached_lane_runs_operation_requests_and_delivers_their_events() {
    use crate::operation_journal::{JOURNAL_FILE_NAME, Journal, LaunchId};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("from")).unwrap();
    std::fs::create_dir(root.path().join("to")).unwrap();
    std::fs::write(root.path().join("from/a.txt"), b"a").unwrap();
    let (mut runner, wakes) = runtime(source_factory(|_, _| None));
    let journal = Journal::new(Some(root.path().join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
    runner.attach_operations(Services { fs: Arc::new(crate::operation_step::NativeFileSystem), journal, budget: crate::operation_lane::STEP_BUDGET }).unwrap();
    let folder = |name: &str| crate::native_location::location_from_path(&root.path().join(name)).unwrap();
    let id = OperationId::new(1);
    let intent = OperationIntent::new(OperationKind::Copy, folder("from"), vec![OperationTarget { name: EntryName::new("a.txt").unwrap(), kind: dual_pane_domain::EntryKind::File }], Some(folder("to"))).unwrap();
    assert_eq!(runner.dispatch(WorkRequest::Operation(OperationEffect::Scan { id, generation: 1, intent, skipped: Arc::from([]) })), None, "the lane accepts the request");
    let mut events = Vec::new();
    // The startup journal report and the scan result arrive in either order.
    while !(events.iter().any(|event| matches!(event, Event::OperationScanned { .. })) && events.contains(&Event::JournalStatus { available: true })) {
        events.extend(wait(&mut runner, &wakes));
    }
    let scanned = events.iter().find_map(|event| match event {
        Event::OperationScanned { id: owner, generation, plan } if *owner == id => Some((*generation, plan.clone())),
        _ => None,
    });
    let Some((1, Ok(plan))) = scanned else { panic!("the scan result for generation 1: {scanned:?}") };
    assert_eq!(plan.iter().map(|item| item.source.clone()).collect::<Vec<_>>(), vec![folder("from").join(&EntryName::new("a.txt").unwrap())]);
}
fn source_factory(source: impl Fn(&Location, &AtomicBool) -> FolderItemsOutcome + Send + Sync + 'static) -> FolderItemsSourceFactory {
    let source = Arc::new(source);
    Arc::new(move || {
        let source = Arc::clone(&source);
        Box::new(move |location, _, _, cancelled| source(location, cancelled))
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

#[test]
fn filter_projection_runs_on_the_bounded_worker_lane() {
    let (mut runner, wakes) = runtime(source_factory(|_, _| None));
    let request = FilterRequest { browser: BrowserSide::Left, tab: TabId::new(7), listing: RequestToken::first(), visibility_generation: 0, generation: 1, text: "STRASSE".into(), entries: vec![Entry::new(EntryName::new("Straße").unwrap(), dual_pane_domain::EntryKind::File), Entry::new(EntryName::new("other").unwrap(), dual_pane_domain::EntryKind::File)].into() };
    assert!(runner.dispatch(WorkRequest::ProjectFilter { request: request.clone() }).is_none());
    let events = wait(&mut runner, &wakes);
    assert!(events.iter().any(|event| matches!(event, Event::FilterProjected { request: completed, result: Ok(result) } if completed == &request && result.indices.as_ref() == [0])));
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
        Box::new(move |location, _, _, _cancelled| {
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

mod path_resolution;
mod read_admission;
mod worker_lanes;
mod worker_recovery;
