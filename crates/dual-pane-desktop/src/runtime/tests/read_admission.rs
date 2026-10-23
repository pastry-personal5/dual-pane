use super::*;

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
fn column_choices_finish_while_the_ordinary_listing_lane_is_blocked() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::create_dir(temporary.path().join("child")).unwrap();
    let parent = crate::native_location::location_from_path(temporary.path()).unwrap();
    let selected = EntryName::new("child").unwrap();
    let (started_sender, started) = mpsc::channel();
    let source = source_factory(move |_, cancelled| {
        started_sender.send(()).ok();
        while !cancelled.load(Ordering::Relaxed) {
            thread::park_timeout(Duration::from_millis(1));
        }
        None
    });
    let (mut runner, wakes) = runtime(source);
    assert!(runner.dispatch(read(0, "blocked")).is_none());
    started.recv_timeout(TIMEOUT).unwrap();
    let request = FolderChoiceRequest { id: 1, browser: BrowserSide::Left, tab: TabId::new(0), source: parent.join(&selected), visible_revision: 1, parent, selected: selected.clone(), after: None, visibility: Default::default(), generation: 1 };
    assert!(runner.request_folder_choices(request).is_none());
    wakes.recv_timeout(TIMEOUT).unwrap();
    let results = runner.take_folder_choices();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].outcome.as_ref().unwrap().names.as_ref(), &[selected]);
    runner.dispatch(WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0) });
}

#[test]
fn failed_column_read_can_be_retried_after_the_parent_returns() {
    let temporary = tempfile::tempdir().unwrap();
    let parent_path = temporary.path().join("parent");
    let parent = crate::native_location::location_from_path(&parent_path).unwrap();
    let selected = EntryName::new("child").unwrap();
    let (mut runner, wakes) = runtime(source_factory(|_, _| None));
    let request = |id, generation| FolderChoiceRequest { id, browser: BrowserSide::Left, tab: TabId::new(0), source: parent.join(&selected), visible_revision: 1, parent: parent.clone(), selected: selected.clone(), after: None, visibility: Default::default(), generation };

    assert!(runner.request_folder_choices(request(1, 1)).is_none());
    wakes.recv_timeout(TIMEOUT).unwrap();
    assert!(runner.take_folder_choices()[0].outcome.is_err());
    runner.take_events(0);

    std::fs::create_dir(&parent_path).unwrap();
    std::fs::create_dir(parent_path.join("child")).unwrap();
    assert!(runner.request_folder_choices(request(2, 2)).is_none());
    wakes.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(runner.take_folder_choices()[0].outcome.as_ref().unwrap().names.as_ref(), &[selected]);
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
    let mut runner = Runtime { left_jobs: jobs, right_jobs: job_queue().0, probes: mpsc::sync_channel(1).0, clipboard: mpsc::sync_channel(1).0, associations: mpsc::sync_channel(1).0, events, next_event: None, wake_pending, outstanding: HashMap::new(), preview_jobs: [mpsc::sync_channel(1).0, mpsc::sync_channel(1).0], preview_pending: [None, None], preview_outstanding: HashMap::new(), column_senders: [mpsc::sync_channel(1).0, mpsc::sync_channel(1).0], column_results: mpsc::channel().1, column_outstanding: HashMap::new(), filter_lanes: None, read_capacity: SINGLE_LANE_CAPACITY, delivery, operations: None, watcher: WatcherHandle::unavailable() };

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
