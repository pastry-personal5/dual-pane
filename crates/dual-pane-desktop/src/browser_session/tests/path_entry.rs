use super::*;

fn loaded_session(base: &str) -> WorkspaceSession<FakeRunner> {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path(base));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha")]), changes: None });
    session
}

#[test]
fn cancelled_column_generation_rejects_late_result() {
    let mut session = loaded_session("base");
    let browser = BrowserSide::Left;
    let source = session.view(browser).location().unwrap().clone();
    let revision = session.view(browser).visible_revision();
    let id = session.request_folder_choices(ColumnReadRequest { browser, tab: TabId::new(0), source: source.clone(), visible_revision: revision, column: 0, after: None, generation: 17 }).unwrap();
    let late = session.take_folder_choice_result(id).unwrap();
    session.cancel_folder_choices(browser, 17);
    session.accept_column_result(late);
    assert!(session.take_folder_choice_result(id).is_none());
    assert!(session.request_folder_choices(ColumnReadRequest { browser, tab: TabId::new(0), source, visible_revision: revision.wrapping_add(1), column: 0, after: None, generation: 18 }).is_none());
}
fn resolve_token(session: &WorkspaceSession<FakeRunner>) -> RequestToken {
    session
        .runner
        .dispatched
        .iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ResolvePath { token, .. } => Some(*token),
            _ => None,
        })
        .expect("a resolve request")
}

#[test]
fn a_submitted_path_shows_loading_then_a_failure_stays_until_discarded() {
    let mut session = loaded_session("base");
    session.submit_path_observed(BrowserSide::Left, 0, Some("/missing".into()), None, true);
    assert_eq!(session.view(BrowserSide::Left).status_text(), "Loading…");
    assert!(session.view(BrowserSide::Left).path_entry().is_pending());
    let token = resolve_token(&session);
    session.submit(Event::PathResolved { browser: BrowserSide::Left, tab: TabId::new(0), token, result: Err(ListingErrorKind::ItemMissing) });
    let view = session.view(BrowserSide::Left);
    assert!(!view.path_entry().is_pending());
    assert_eq!(view.path_entry().error(), Some("“/missing” no longer exists."));
    assert_eq!(view.status_text(), "“/missing” no longer exists.");
    assert_eq!(view.location_text(), "/base", "the tab is untouched");
    session.discard_path_entry(BrowserSide::Left);
    assert_eq!(session.view(BrowserSide::Left).path_entry().error(), None);
    assert_eq!(session.view(BrowserSide::Left).status_text(), "/base");
}

#[test]
fn an_unedited_submission_rereads_the_location_the_editor_was_filled_with() {
    let mut session = loaded_session("base");
    let origin = session.view(BrowserSide::Left).location().cloned();
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("elsewhere") });
    session.submit_path_observed(BrowserSide::Left, 0, Some("/base".into()), origin, false);
    assert!(matches!(session.runner.dispatched.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })), Some(WorkRequest::ReadDirectory { location, .. }) if *location == path("base")));
    assert!(!session.runner.dispatched.iter().any(|work| matches!(work, WorkRequest::ResolvePath { .. })));
}

#[test]
fn text_that_is_not_valid_utf16_is_refused() {
    let mut session = loaded_session("base");
    session.submit_path_observed(BrowserSide::Left, 0, None, None, true);
    assert!(session.view(BrowserSide::Left).path_entry().error().is_some());
    assert!(!session.runner.dispatched.iter().any(|work| matches!(work, WorkRequest::ResolvePath { .. })));
}

#[test]
fn a_submission_from_a_tab_that_is_no_longer_shown_is_ignored() {
    let mut session = loaded_session("base");
    session.submit_path_observed(BrowserSide::Left, 7, Some("/x".into()), None, true);
    assert!(!session.runner.dispatched.iter().any(|work| matches!(work, WorkRequest::ResolvePath { .. })));
    assert!(!session.view(BrowserSide::Left).path_entry().is_pending());
}
