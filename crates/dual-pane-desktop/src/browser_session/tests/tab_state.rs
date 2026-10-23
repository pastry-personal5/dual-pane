use super::*;

#[test]
fn a_sort_in_one_browser_rereads_inactive_tabs_at_that_location_in_both() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("shared"));
    deliver(&mut session, BrowserSide::Left, &["a"]);
    deliver(&mut session, BrowserSide::Right, &["a"]);
    session.submit_ui(BrowserSide::Left, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Left, &["a"]);
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("other") });
    deliver(&mut session, BrowserSide::Left, &[]);
    session.runner.dispatched.clear();
    let size = SortSpec::new(dual_pane_domain::SortField::Size, dual_pane_domain::SortDirection::Descending);
    session.submit_ui(BrowserSide::Right, UiEvent::Sort(size));
    let reads = session.runner.dispatched.iter().filter_map(|work| match work {
        WorkRequest::ReadDirectory { browser, location, sort, .. } if *sort == size => Some((*browser, location.clone())),
        _ => None,
    });
    assert_eq!(reads.collect::<Vec<_>>(), vec![(BrowserSide::Left, path("shared")), (BrowserSide::Right, path("shared"))], "the inactive Left tab rereads; the active one elsewhere does not");
    assert_eq!(session.view(BrowserSide::Right).toolbar().sort, Some(size));
    assert_eq!(session.view(BrowserSide::Left).toolbar().sort, Some(SortSpec::default()), "the Left active tab shows another folder");
}

#[test]
fn selection_cursor_and_scroll_return_with_their_tab() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    deliver(&mut session, BrowserSide::Left, &["a", "b", "c", "d"]);
    let first = session.view(BrowserSide::Left).active_tab().unwrap();
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 1 });
    session.submit_ui(BrowserSide::Left, UiEvent::ExtendToRow { row: 2 });
    session.submit_ui(BrowserSide::Left, UiEvent::Scrolled { row: Some(2), offset: 4 });
    session.submit_ui(BrowserSide::Left, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Left, &["a", "b", "c", "d"]);
    assert!(session.view(BrowserSide::Left).selected_rows().is_empty(), "a new tab starts clean");
    session.submit_ui(BrowserSide::Left, UiEvent::ActivateTab { tab: first });
    let view = session.view(BrowserSide::Left);
    assert_eq!((view.selected_rows(), view.cursor_row(), view.scroll_hint_row()), (vec![1, 2], Some(2), Some((2, 4))));
    assert!(session.view(BrowserSide::Right).selected_rows().is_empty(), "the other Browser is untouched");
}

#[test]
fn a_scroll_report_does_not_activate_its_browser() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    deliver(&mut session, BrowserSide::Left, &["a", "b"]);
    deliver(&mut session, BrowserSide::Right, &["a", "b"]);
    session.submit_ui(BrowserSide::Left, UiEvent::FocusBrowser);
    session.submit_view_state(BrowserSide::Right, UiEvent::Scrolled { row: Some(1), offset: 3 });
    assert_eq!(session.workspace.active_browser(), BrowserSide::Left);
    assert_eq!(session.workspace_view().active_browser(), BrowserSide::Left);
    let right = session.view(BrowserSide::Right).active_tab().unwrap();
    session.submit_ui(BrowserSide::Right, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Right, &[]);
    session.submit_ui(BrowserSide::Right, UiEvent::ActivateTab { tab: right });
    assert_eq!(session.view(BrowserSide::Right).scroll_hint_row(), Some((1, 3)), "the scroll is still recorded");
}

#[test]
fn a_closed_tab_cannot_receive_a_delayed_gesture() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    deliver(&mut session, BrowserSide::Left, &["a", "b"]);
    let stale = session.controller.command(BrowserSide::Left, UiEvent::SelectRow { row: 0 }, session.view(BrowserSide::Left)).unwrap();
    session.submit_ui(BrowserSide::Left, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Left, &["a", "b"]);
    session.submit(stale);
    assert!(session.view(BrowserSide::Left).selected_rows().is_empty(), "a gesture observed on the first tab does not select in the second");
}

#[test]
fn stored_favorites_are_probed_on_the_runner_and_an_unavailable_item_adds_a_notice() {
    use dual_pane_application::{FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoritesRecords, SettingsSnapshot};
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    let favorites = FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "Places".into(), position: 0 }], items: vec![FavoriteItemRecord { id: 3, group_id: 1, name: "Gone".into(), target: path("gone"), position: 0 }] };
    // A session without a worker settled as failed; this load stands in
    // for the worker's answer so the probe path can be followed.
    session.workspace = Workspace::with_home(Location::root());
    session.submit(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites, ..SettingsSnapshot::default() } });
    assert!(session.runner.dispatched.contains(&WorkRequest::ProbeFavoriteTarget { item_id: 3, target: path("gone") }));
    session.runner.delivered.push_back(Event::FavoriteTargetProbed { item_id: 3, target: path("gone"), outcome: FavoriteProbeOutcome::Unavailable });
    session.drain();
    assert!(session.workspace_view().groups()[0].items.is_empty());
    assert!(session.workspace_view().notices().iter().any(|notice| notice.text == "Removed Favorite “Gone” because “/gone” is unavailable."));
}
