use super::*;

#[test]
fn delayed_native_gestures_keep_the_revision_of_the_displayed_rows() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha"), entry("beta")]), changes: None });
    let old_view = session.view(BrowserSide::Left).clone();
    session.submit(Command::SetFilterText { browser: BrowserSide::Left, tab: TabId::new(0), text: "alpha".into() });
    let request = session
        .runner
        .dispatched
        .iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .expect("filter projection");
    session.submit(Event::FilterProjected { result: Ok(project_filter(&request)), request });

    session.submit_observed_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 }, old_view.clone());
    assert!(session.view(BrowserSide::Left).selection().entries().is_empty(), "an old row gesture cannot select after projection");
    session.submit_ui_at_observation(BrowserSide::Left, UiEvent::FileCommand(OperationCommand::Copy), old_view.active_tab().unwrap().value(), old_view.visible_revision());
    assert!(session.operations.view().panels().is_empty(), "an old selection command cannot start after projection");
}

#[test]
fn delayed_filter_edits_cannot_retarget_a_different_active_tab() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    let first = session.view(BrowserSide::Left).active_tab().unwrap();
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, token: RequestToken::first(), entries: Arc::from(vec![entry("first")]), changes: None });
    session.submit(Command::NewTab { browser: BrowserSide::Left });
    let second = session.view(BrowserSide::Left).active_tab().unwrap();
    assert_ne!(first, second);
    let token = session
        .runner
        .dispatched
        .iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, token, .. } if *tab == second => Some(*token),
            _ => None,
        })
        .expect("new tab read");
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: second, token, entries: Arc::from(vec![entry("alpha")]), changes: None });

    session.set_filter_text_observed(BrowserSide::Left, first.value(), "wrong tab".into());
    assert_eq!(session.view(BrowserSide::Left).active_tab(), Some(second));
    assert_eq!(session.view(BrowserSide::Left).filter_text(), "");

    session.set_filter_text_observed(BrowserSide::Left, second.value(), "right tab".into());
    assert_eq!(session.view(BrowserSide::Left).filter_text(), "right tab");
    session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: first });
    assert_eq!(session.view(BrowserSide::Left).filter_text(), "");
}

#[test]
fn loading_failure_cancellation_and_late_results_preserve_the_other_browser() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 0));
    assert_view(&session, BrowserSide::Right, ("", &[], None, "Loading…", true, 0));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]), changes: None });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]), changes: None });
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("older") });
    let older = RequestToken::first().next().next();
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("missing") });
    let missing = older.next();
    assert!(session.runner.dispatched.contains(&WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: older }));
    session.submit(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: older });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: older, entries: Arc::from(vec![entry("late")]), changes: None });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: missing, entries: Arc::from(vec![entry("wrong")]), changes: None });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "Loading…", true, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: missing, kind: ListingErrorKind::ItemMissing });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "“/missing” no longer exists.", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("cancelled") });
    let cancelled = missing.next();
    session.submit(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: cancelled });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("final") });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: cancelled.next(), entries: Arc::from(vec![entry("fresh")]), changes: None });
    assert_view(&session, BrowserSide::Left, ("/final", &["fresh"], None, "/final", false, 2));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
}

#[test]
fn bounded_drain_applies_one_browsers_result_without_touching_the_other() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), 1, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.runner.delivered.push_back(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]), changes: None });
    session.runner.delivered.push_back(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]), changes: None });
    assert!(session.drain());
    assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 0));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], None, "/base", false, 1));
    assert!(!session.drain());
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], None, "/base", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], None, "/base", false, 1));
}

#[test]
fn start_dispatches_distinct_reads_for_both_browsers() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(Location::root());
    let reads = session.runner.dispatched.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).cloned().collect::<Vec<_>>();
    assert_eq!(reads, vec![WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), location: Location::root(), sort: SortSpec::default(), visibility: Default::default(), previous: None }, WorkRequest::ReadDirectory { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), location: Location::root(), sort: SortSpec::default(), visibility: Default::default(), previous: None }]);
}

#[test]
fn clipboard_menu_availability_tracks_selection_destination_and_the_observed_offer() {
    use dual_pane_application::{ClipboardAction, ClipboardGeneration, ClipboardOffer};

    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha"), entry("beta")]), changes: None });
    let copy = Some(ClipboardAction::Copy);
    let cut = Some(ClipboardAction::Cut);
    assert!(!session.clipboard_command_available(BrowserSide::Left, copy) && !session.clipboard_command_available(BrowserSide::Left, cut), "Copy and Cut need a selection");
    assert!(!session.clipboard_command_available(BrowserSide::Left, None), "Paste needs a supported clipboard offer");

    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    assert!(session.clipboard_command_available(BrowserSide::Left, copy) && session.clipboard_command_available(BrowserSide::Left, cut));

    session.observe_clipboard(ClipboardGeneration::new(1), ClipboardOffer::Files);
    assert!(session.clipboard_command_available(BrowserSide::Left, None), "a ready destination and a file offer enable Paste");
    assert!(!session.clipboard_command_available(BrowserSide::Right, None), "a loading destination disables Paste");

    session.observe_clipboard(ClipboardGeneration::new(2), ClipboardOffer::Unsupported);
    assert!(!session.clipboard_command_available(BrowserSide::Left, None), "replacement by unsupported content disables Paste");
}

#[test]
fn an_observed_paste_reaches_the_clipboard_read() {
    use dual_pane_application::{ClipboardGeneration, ClipboardOffer};

    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha")]), changes: None });
    session.observe_clipboard(ClipboardGeneration::new(1), ClipboardOffer::Files);
    session.submit_clipboard_observed(BrowserSide::Left, None, TabId::new(0), 0);
    assert!(session.runner.dispatched.iter().any(|request| matches!(request, WorkRequest::ReadClipboard { .. })), "Paste must start a clipboard read");
}
