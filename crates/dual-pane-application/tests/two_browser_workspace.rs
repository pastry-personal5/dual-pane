use dual_pane_application::{Command, Event, SettingsSnapshot, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, SortDirection, SortField, SortSpec, TabId};
use std::sync::Arc;

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}
fn location(value: &str) -> Location {
    Location::root().join(&name(value))
}
fn entry(value: &str) -> Entry {
    Entry::new(name(value), EntryKind::File)
}
fn request(work: &[WorkRequest]) -> (TabId, dual_pane_domain::RequestToken) {
    match work.last() {
        Some(WorkRequest::ReadDirectory { tab, token, .. }) => (*tab, *token),
        _ => panic!("read"),
    }
}
fn load(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, token: dual_pane_domain::RequestToken, entries: Vec<Entry>) {
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(entries) }.into());
}

#[test]
fn tabs_are_stable_and_close_selects_the_right_neighbor() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("one") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("a")]);
    let second = workspace.active_tab(BrowserSide::Left);
    let created = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let (third, token) = request(&created.work);
    assert_ne!(second, third);
    load(&mut workspace, BrowserSide::Left, third, token, vec![entry("b")]);
    workspace.handle(Command::CloseTab { browser: BrowserSide::Left, tab: second }.into());
    assert_eq!(workspace.active_tab(BrowserSide::Left), third);
    assert_eq!(workspace.tabs(BrowserSide::Left).collect::<Vec<_>>(), vec![third]);
}

#[test]
fn stale_tab_result_cannot_replace_a_newer_read() {
    let mut workspace = Workspace::new();
    let (tab, old) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("old") }.into()).work);
    let (_, new) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("new") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, old, vec![entry("wrong")]);
    assert!(workspace.entries(BrowserSide::Left).is_empty());
    load(&mut workspace, BrowserSide::Left, tab, new, vec![entry("right")]);
    assert_eq!(workspace.entries(BrowserSide::Left)[0].name(), &name("right"));
}

#[test]
fn navigation_after_back_truncates_forward_history() {
    let mut workspace = Workspace::new();
    let (tab, one) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("one") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, one, vec![]);
    let (_, two) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("two") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, two, vec![]);
    let (_, back) = request(&workspace.handle(Command::GoBack { browser: BrowserSide::Left }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, back, vec![]);
    let (_, three) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("three") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, three, vec![]);
    assert!(workspace.handle(Command::GoForward { browser: BrowserSide::Left }.into()).work.is_empty());
}

#[test]
fn closing_an_inactive_tab_preserves_the_active_tab_and_its_view() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("one") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("first")]);
    let created = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let (second, token) = request(&created.work);
    load(&mut workspace, BrowserSide::Left, second, token, vec![entry("second")]);

    let closed = workspace.handle(Command::CloseTab { browser: BrowserSide::Left, tab: first }.into());
    assert_eq!(workspace.active_tab(BrowserSide::Left), second);
    assert_eq!(workspace.entries(BrowserSide::Left)[0].name(), &name("second"));
    assert!(!closed.outputs.iter().any(|output| matches!(output, dual_pane_application::Output::TabViewChanged { .. })));
}

#[test]
fn selection_commands_before_the_first_listing_are_safe() {
    let mut workspace = Workspace::new();
    for command in [Command::SelectAll { browser: BrowserSide::Left }, Command::ClearSelection { browser: BrowserSide::Left }, Command::UpdateScrollHint { browser: BrowserSide::Left, anchor: Some(name("item")), offset: 4 }] {
        assert_eq!(workspace.handle(command.into()).outputs, vec![]);
    }
}

#[test]
fn selecting_the_sole_selected_item_reports_cursor_recovery() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, row: 0, name: name("a") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, row: 1, name: name("b") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, row: 1, name: name("b") }.into());

    let transition = workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, row: 0, name: name("a") }.into());
    assert!(matches!(transition.outputs.as_slice(), [dual_pane_application::Output::SelectionChanged { row: Some(0), .. }]));
}

#[test]
fn favorites_editing_preserves_order_ids_and_rejects_invalid_names() {
    let mut workspace = Workspace::new();
    let group = workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into());
    assert!(matches!(group.work.as_slice(), [WorkRequest::SaveSettings { .. }]));
    let id = workspace.favorites().groups[0].id;
    assert!(workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into()).outputs.is_empty());
    assert!(workspace.handle(Command::RenameFavoriteGroup { id, name: " \t".into() }.into()).outputs.is_empty());
    workspace.handle(Command::CreateFavoriteItem { group_id: id, name: "Root".into(), target: Location::root() }.into());
    let item = workspace.favorites().items[0].id;
    assert_eq!(workspace.favorites().items[0].target, Location::root());
    workspace.handle(Command::RenameFavoriteItem { id: item, name: "System".into() }.into());
    assert_eq!(workspace.favorites().items[0].name, "System");
    workspace.handle(Command::DeleteFavoriteGroup { id }.into());
    assert!(workspace.favorites().groups.is_empty());
    assert!(workspace.favorites().items.is_empty());
    assert!(workspace.favorites().initialized);
}

#[test]
fn unavailable_probe_only_removes_the_matching_favorite_target() {
    let mut workspace = Workspace::new();
    workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into());
    let group = workspace.favorites().groups[0].id;
    let old = location("old");
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "Old".into(), target: old.clone() }.into());
    let item = workspace.favorites().items[0].id;
    assert!(workspace.handle(Event::FavoriteTargetProbed { item_id: item, target: location("different"), available: false }.into()).outputs.is_empty());
    assert_eq!(workspace.favorites().items.len(), 1);
    workspace.handle(Event::FavoriteTargetProbed { item_id: item, target: old, available: false }.into());
    assert!(workspace.favorites().items.is_empty());
}

#[test]
fn invalidation_refreshes_matching_inactive_tabs_in_both_browsers() {
    let mut workspace = Workspace::new();
    let shared = location("shared");
    let (left, left_token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: shared.clone() }.into()).work);
    load(&mut workspace, BrowserSide::Left, left, left_token, vec![entry("left")]);
    let (right, right_token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: shared.clone() }.into()).work);
    load(&mut workspace, BrowserSide::Right, right, right_token, vec![entry("right")]);
    let created = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let (inactive, token) = request(&created.work);
    load(&mut workspace, BrowserSide::Left, inactive, token, vec![entry("inactive")]);
    workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab: left }.into());
    let refresh = workspace.handle(Command::InvalidateLocation { location: shared }.into());
    let reads = refresh.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count();
    assert_eq!(reads, 3);
}

#[test]
fn uninitialized_settings_seed_favorites_once_with_startup_context() {
    let home = location("home");
    let mut workspace = Workspace::with_context(home, true);
    let transition = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    assert!(matches!(transition.work.as_slice(), [WorkRequest::SaveSettings { .. }]));
    assert_eq!(workspace.favorites().items.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), ["Applications", "Desktop", "Documents", "Screenshots", "Downloads"]);
    let count = workspace.favorites().items.len();
    workspace.handle(Event::SettingsLoaded { snapshot: dual_pane_application::SettingsSnapshot { favorites: workspace.favorites().clone(), ..SettingsSnapshot::default() } }.into());
    assert_eq!(workspace.favorites().items.len(), count);
}

#[test]
fn tab_local_selection_gestures_obey_anchor_and_secondary_rules() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("items") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("a"), entry("b"), entry("c")]);
    assert!(workspace.handle(Command::SelectRange { browser: BrowserSide::Left, row: 2, name: name("c") }.into()).outputs.is_empty());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, row: 0, name: name("a") }.into());
    workspace.handle(Command::SelectRange { browser: BrowserSide::Left, row: 2, name: name("c") }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("a"), name("b"), name("c")]);
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, row: 2, name: name("c") }.into());
    assert!(!workspace.selection(BrowserSide::Left).contains(&name("c")));
    assert!(workspace.handle(Command::SecondarySelect { browser: BrowserSide::Left, row: 1, name: name("b") }.into()).outputs.is_empty());
    workspace.handle(Command::SecondarySelect { browser: BrowserSide::Left, row: 2, name: name("c") }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("c")]);
    workspace.handle(Command::SelectAll { browser: BrowserSide::Left }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries().len(), 3);
}

#[test]
fn failed_navigation_retains_history_and_location_shared_sort_refreshes_every_tab() {
    let mut workspace = Workspace::new();
    let shared = location("shared");
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: shared.clone() }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("a")]);
    let created = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let (second, token) = request(&created.work);
    load(&mut workspace, BrowserSide::Left, second, token, vec![entry("b")]);
    let pending = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("missing") }.into());
    let (_, token) = request(&pending.work);
    workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: second, token, kind: dual_pane_domain::ListingErrorKind::ItemMissing }.into());
    assert_eq!(workspace.location(BrowserSide::Left), Some(&shared));
    let transition = workspace.handle(Command::SetSort { location: shared, sort: dual_pane_domain::SortSpec::new(dual_pane_domain::SortField::Size, dual_pane_domain::SortDirection::Descending) }.into());
    assert_eq!(transition.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count(), 2);
}

#[test]
fn sorting_a_pending_navigation_restarts_its_target_without_committing_the_old_result() {
    let mut workspace = Workspace::new();
    let (tab, first) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("shown") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, first, vec![entry("old")]);
    let target = location("target");
    let (_, stale) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: target.clone() }.into()).work);
    let sort = SortSpec::new(SortField::Size, SortDirection::Descending);
    let changed = workspace.handle(Command::SetSort { location: target.clone(), sort }.into());
    assert!(changed.work.contains(&WorkRequest::Cancel { browser: BrowserSide::Left, tab, token: stale }));
    assert!(matches!(changed.work.iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })), Some(WorkRequest::ReadDirectory { location, sort: requested, .. }) if *location == target && *requested == sort));
    let (_, current) = request(&changed.work[..changed.work.len() - 1]);
    load(&mut workspace, BrowserSide::Left, tab, stale, vec![entry("stale")]);
    assert_eq!(workspace.location(BrowserSide::Left), Some(&location("shown")));
    load(&mut workspace, BrowserSide::Left, tab, current, vec![entry("new")]);
    assert_eq!(workspace.location(BrowserSide::Left), Some(&target));
    assert_eq!(workspace.entries(BrowserSide::Left)[0].name(), &name("new"));
}

#[test]
fn settings_load_refreshes_each_tab_once_and_restarts_initial_pending_reads() {
    let mut workspace = Workspace::new();
    let shared = location("shared");
    let (left, first) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: shared.clone() }.into()).work);
    load(&mut workspace, BrowserSide::Left, left, first, vec![]);
    let (right, second) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: shared.clone() }.into()).work);
    load(&mut workspace, BrowserSide::Right, right, second, vec![]);
    let (third, third_token) = request(&workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into()).work);
    let sort = SortSpec::new(SortField::Modified, SortDirection::Descending);
    let snapshot = SettingsSnapshot { folder_sorts: vec![(shared.clone(), sort)], ..SettingsSnapshot::default() };
    let transition = workspace.handle(Event::SettingsLoaded { snapshot }.into());
    let reads = transition.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { location, sort: requested, .. } if *location == shared && *requested == sort)).count();
    assert_eq!(reads, 3);
    assert!(transition.work.contains(&WorkRequest::Cancel { browser: BrowserSide::Left, tab: third, token: third_token }));
}

#[test]
fn right_click_does_not_establish_a_missing_range_anchor() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("items") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    workspace.handle(Command::SecondarySelect { browser: BrowserSide::Left, row: 0, name: name("a") }.into());
    assert!(workspace.handle(Command::SelectRange { browser: BrowserSide::Left, row: 1, name: name("b") }.into()).outputs.is_empty());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("a")]);
}

#[test]
fn recreating_a_favorite_does_not_reuse_an_id_from_an_in_flight_probe() {
    let mut workspace = Workspace::new();
    workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into());
    let group = workspace.favorites().groups[0].id;
    let target = location("same-target");
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "First".into(), target: target.clone() }.into());
    let old_id = workspace.favorites().items[0].id;
    workspace.handle(Command::DeleteFavoriteItem { id: old_id }.into());
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "Second".into(), target: target.clone() }.into());
    let new_id = workspace.favorites().items[0].id;
    assert_ne!(new_id, old_id);
    workspace.handle(Event::FavoriteTargetProbed { item_id: old_id, target, available: false }.into());
    assert_eq!(workspace.favorites().items[0].id, new_id);
}

#[test]
fn scroll_hint_is_exposed_when_history_and_tabs_are_restored() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("one") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("anchor")]);
    workspace.handle(Command::UpdateScrollHint { browser: BrowserSide::Left, anchor: Some(name("anchor")), offset: 7 }.into());
    let (_, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("two") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![]);
    let (_, token) = request(&workspace.handle(Command::GoBack { browser: BrowserSide::Left }.into()).work);
    let restored = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, token, entries: Arc::from(vec![entry("anchor")]) }.into());
    assert!(restored.outputs.iter().any(|output| matches!(output, dual_pane_application::Output::FolderItemsReplaced { scroll_hint: Some((anchor, 7)), .. } if anchor == &name("anchor"))));

    workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let switched = workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab: first }.into());
    assert!(switched.outputs.iter().any(|output| matches!(output, dual_pane_application::Output::TabViewChanged { scroll_hint: Some((anchor, 7)), .. } if anchor == &name("anchor"))));
}
