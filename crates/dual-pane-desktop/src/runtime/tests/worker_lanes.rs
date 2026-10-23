use super::*;

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
