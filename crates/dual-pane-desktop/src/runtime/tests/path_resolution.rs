use super::*;

fn resolve_request(index: usize, candidate: Location) -> WorkRequest {
    WorkRequest::ResolvePath { browser: BrowserSide::Left, tab: TabId::new(index as u64), token: token(index), candidate }
}

#[test]
fn a_path_check_runs_on_a_read_lane_and_delivers_its_result() {
    let folder = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(folder.path()).unwrap();
    std::fs::create_dir(root.join("inner")).unwrap();
    let candidate = crate::native_location::location_from_path(&root.join("inner")).unwrap();
    let (mut runner, wakes) = runtime(source_factory(|_, _| None));
    assert_eq!(runner.dispatch(resolve_request(0, candidate.clone())), None);
    let events = wait(&mut runner, &wakes);
    assert_eq!(events, vec![Event::PathResolved { browser: BrowserSide::Left, tab: TabId::new(0), token: token(0), result: Ok(dual_pane_application::ResolvedPath { location: candidate, target: dual_pane_application::PathTarget::Folder }) }]);
    assert!(runner.outstanding.is_empty(), "the finished check no longer holds read capacity");
}

#[test]
fn a_path_check_that_fails_reports_its_category() {
    let (mut runner, wakes) = runtime(source_factory(|_, _| None));
    assert_eq!(runner.dispatch(resolve_request(0, location("dual-pane-no-such-top-level-item"))), None);
    let events = wait(&mut runner, &wakes);
    assert!(matches!(events.as_slice(), [Event::PathResolved { result: Err(ListingErrorKind::ItemMissing), .. }]), "{events:?}");
}

#[test]
fn a_path_check_shares_the_read_admission_and_reports_busy() {
    let (mut runner, probe) = start_probe("blocked");
    runner.dispatch(read(0, "blocked"));
    assert_eq!(probe.next_call().0, location("blocked"));
    let mut index = 1;
    while runner.dispatch(read(index, "queued")).is_none() {
        index += 1;
        assert!(index < 100, "admission must be bounded");
    }
    let busy = runner.dispatch(resolve_request(index + 1, location("anything")));
    assert!(matches!(busy, Some(Event::PathResolved { result: Err(ListingErrorKind::Busy), .. })), "{busy:?}");
    probe.gate.send(()).unwrap();
}

#[test]
fn a_cancelled_path_check_never_delivers() {
    let (mut runner, probe) = start_probe("blocked");
    runner.dispatch(read(0, "blocked"));
    assert_eq!(probe.next_call().0, location("blocked"));
    assert_eq!(runner.dispatch(resolve_request(1, location("dual-pane-no-such-top-level-item"))), None);
    assert_eq!(runner.dispatch(WorkRequest::CancelPathResolve { browser: BrowserSide::Left, tab: TabId::new(9), token: token(1) }), None, "the wrong tab cannot cancel it");
    assert!(runner.outstanding.contains_key(&token(1)));
    assert_eq!(runner.dispatch(WorkRequest::CancelPathResolve { browser: BrowserSide::Left, tab: TabId::new(1), token: token(1) }), None);
    assert!(!runner.outstanding.contains_key(&token(1)));
    probe.gate.send(()).unwrap();
    probe.wait_for_wake();
    let (events, _) = runner.take_events(8);
    assert!(events.iter().all(|event| matches!(event, Event::FolderItemsLoaded { .. })), "{events:?}");
}
