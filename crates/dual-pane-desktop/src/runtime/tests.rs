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

#[test]
fn runs_a_read_on_the_worker_and_delivers_its_result() {
    let (mut runner, probe) = start_probe("blocked");
    assert_eq!(runner.dispatch(read(0, "alpha")), None);
    assert_eq!(probe.next_call(), (location("alpha"), Some("listing-worker".to_owned())));
    probe.wait_for_wake();
    assert_eq!(runner.take_events(8), (vec![Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), entries: Arc::from([]), changes: None }], false));
}

#[test]
fn clipboard_preflight_runs_off_thread_and_does_not_follow_links() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("file");
    let link = directory.path().join("link");
    std::fs::write(&file, b"data").unwrap();
    std::os::unix::fs::symlink(&file, &link).unwrap();
    let sources: Arc<[Location]> = vec![crate::native_location::location_from_path(&file).unwrap(), crate::native_location::location_from_path(&link).unwrap()].into();
    let request = dual_pane_application::ClipboardRequestId::new(4);
    let generation = dual_pane_application::ClipboardGeneration::new(8);
    let (mut runner, probe) = start_probe("blocked");
    assert_eq!(runner.dispatch(WorkRequest::PreflightClipboard { request, generation, sources: Arc::clone(&sources) }), None);
    probe.wait_for_wake();
    let (events, _) = runner.take_events(8);
    let [Event::ClipboardPreflighted { request: found, generation: found_generation, result: Ok(roots) }] = events.as_slice() else { panic!("{events:?}") };
    assert_eq!((*found, *found_generation), (request, generation));
    assert_eq!(roots[0].kind, EntryKind::File);
    assert_eq!(roots[1].kind, EntryKind::Symlink { points_to_directory: false });
    assert_eq!(roots.iter().map(|root| &root.source).collect::<Vec<_>>(), sources.iter().collect::<Vec<_>>());
}

#[test]
fn the_worker_diffs_a_reload_against_the_previous_rows() {
    let file = |name: &str| Entry::new(EntryName::new(name).unwrap(), dual_pane_domain::EntryKind::File);
    let listed: Arc<[Entry]> = Arc::from(vec![file("a"), file("b"), file("c")]);
    let source_rows = Arc::clone(&listed);
    let (mut runner, wakes) = runtime(source_factory(move |_, _| Some(Ok(Arc::clone(&source_rows)))));
    let previous: Arc<[Entry]> = Arc::from(vec![file("a"), file("c")]);
    let reload = WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), location: location("folder"), sort: SortSpec::default(), visibility: Default::default(), previous: Some(previous) };
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
    let delivery = Delivery { events: mpsc::channel().0, wake_pending: Arc::clone(&wake_pending), wake: Arc::new(Mutex::new(Box::new(|| {}))), progress: Arc::default() };
    let mut runner = Runtime { left_jobs: jobs, right_jobs: job_queue().0, probes: mpsc::sync_channel(1).0, clipboard: mpsc::sync_channel(1).0, events, next_event: None, wake_pending, outstanding: HashMap::new(), preview_jobs: [mpsc::sync_channel(1).0, mpsc::sync_channel(1).0], preview_pending: [None, None], preview_outstanding: HashMap::new(), filter_lanes: None, read_capacity: SINGLE_LANE_CAPACITY, delivery, operations: None, watcher: WatcherHandle::unavailable() };

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
        jobs.send(Job { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), location: location("item"), sort: SortSpec::default(), visibility: Default::default(), previous: None, state }).unwrap();
    }
    drop(jobs);
    let source_factory = source_factory(|_, _| Some(Ok(Arc::from([]))));
    let (event_sender, events) = mpsc::channel();
    let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Arc::new(Mutex::new(Box::new(|| {}))), progress: Arc::default() };
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
            Box::new(move |_, _, _, _| {
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
            Box::new(|_, _, _, _| Some(Ok(Arc::from([]))))
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
            Box::new(move |_, _, _, _| {
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
        jobs.send(Job { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), location: location("item"), sort: SortSpec::default(), visibility: Default::default(), previous: None, state }).unwrap();
    }
    drop(jobs);
    let source_factory = source_factory(|_, _| Some(Ok(Arc::from([]))));
    let (event_sender, events) = mpsc::channel();
    let delivery = Delivery { events: event_sender, wake_pending: Arc::new(AtomicBool::new(false)), wake: Arc::new(Mutex::new(Box::new(|| {}))), progress: Arc::default() };
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
