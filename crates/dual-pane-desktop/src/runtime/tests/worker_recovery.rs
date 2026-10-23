use super::*;

#[test]
fn worker_start_failures_do_not_stop_later_work() {
    let (jobs, queue) = mpsc::channel();
    for index in 0..3 {
        let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
        jobs.send(Job { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), location: location("item"), sort: SortSpec::default(), resolve: false, visibility: Default::default(), previous: None, state }).unwrap();
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
        jobs.send(Job { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), location: location("item"), sort: SortSpec::default(), resolve: false, visibility: Default::default(), previous: None, state }).unwrap();
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
