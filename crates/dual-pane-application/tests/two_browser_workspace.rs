use dual_pane_application::{BrowserSnapshot, Command, Event, FavoriteEdit, FavoriteProbeOutcome, FavoriteRejection, Notice, NoticeKind, Output, RowChange, SessionSnapshot, SettingsFailure, SettingsSnapshot, SettingsStatus, TabSnapshot, WindowFrame, WindowLayout, WindowState, WorkRequest, Workspace, WorkspaceSnapshot};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, ScrollAnchor, SortDirection, SortField, SortSpec, TabId};
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
/// A workspace whose stored settings loaded with an initialized, empty
/// Favorites collection, as at a normal launch.
fn ready() -> Workspace {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites: dual_pane_application::FavoritesRecords { initialized: true, ..Default::default() }, ..SettingsSnapshot::default() } }.into());
    workspace
}
fn load(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, token: dual_pane_domain::RequestToken, entries: Vec<Entry>) {
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(entries), changes: None }.into());
}

/// The reason a Favorites edit was rejected, if it was.
fn rejection(transition: &dual_pane_application::Transition) -> Option<FavoriteRejection> {
    match transition.outputs.as_slice() {
        [Output::FavoriteEditRejected { reason, .. }] if transition.work.is_empty() => Some(*reason),
        _ => None,
    }
}

/// Navigates `browser`'s active tab to `at` and confirms an empty listing.
fn show(workspace: &mut Workspace, browser: BrowserSide, at: &Location) -> TabId {
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser, location: at.clone() }.into()).work);
    load(workspace, browser, tab, token, vec![]);
    tab
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
    let (_, back) = request(&workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, back, vec![]);
    let (_, three) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("three") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, three, vec![]);
    assert!(workspace.handle(Command::GoForward { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work.is_empty());
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
    for command in [Command::SelectAll { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }, Command::ClearSelection { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }, Command::UpdateScrollHint { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), scroll: Some(ScrollAnchor::new(name("item"), 4)) }] {
        assert_eq!(workspace.handle(command.into()).outputs, vec![]);
    }
}

#[test]
fn selecting_the_sole_selected_item_reports_cursor_recovery() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 0, name: name("a") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 1, name: name("b") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 1, name: name("b") }.into());

    let transition = workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 0, name: name("a") }.into());
    assert!(matches!(transition.outputs.as_slice(), [dual_pane_application::Output::SelectionChanged { row: Some(0), .. }]));
}

#[test]
fn favorites_editing_preserves_order_ids_and_rejects_invalid_names() {
    let mut workspace = ready();
    let group = workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into());
    assert!(matches!(group.work.as_slice(), [WorkRequest::SaveSettings { .. }]));
    let id = workspace.favorites().groups[0].id;
    assert_eq!(rejection(&workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into())), Some(FavoriteRejection::DuplicateName));
    assert_eq!(rejection(&workspace.handle(Command::RenameFavoriteGroup { id, name: " \t".into() }.into())), Some(FavoriteRejection::InvalidName));
    assert_eq!(rejection(&workspace.handle(Command::RenameFavoriteGroup { id, name: "Places".into() }.into())), Some(FavoriteRejection::Unchanged));
    assert_eq!(rejection(&workspace.handle(Command::RenameFavoriteGroup { id: id + 40, name: "Elsewhere".into() }.into())), Some(FavoriteRejection::Stale));
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
fn favorites_moves_to_the_current_position_change_nothing() {
    let mut workspace = ready();
    workspace.handle(Command::CreateFavoriteGroup { name: "A".into() }.into());
    workspace.handle(Command::CreateFavoriteGroup { name: "B".into() }.into());
    let group = workspace.favorites().groups[0].id;
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "first".into(), target: location("first") }.into());
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "second".into(), target: location("second") }.into());
    let item = workspace.favorites().items[1].id;
    assert_eq!(rejection(&workspace.handle(Command::ReorderFavoriteGroup { id: group, position: 0 }.into())), Some(FavoriteRejection::Unchanged));
    assert_eq!(rejection(&workspace.handle(Command::MoveFavoriteItem { id: item, group_id: group, position: 1 }.into())), Some(FavoriteRejection::Unchanged));
    assert!(!workspace.handle(Command::MoveFavoriteItem { id: item, group_id: group, position: 0 }.into()).outputs.is_empty());
    assert_eq!(favorite_names(&workspace), ["second", "first"]);

    // Another Group's Item after this Group's last Item, and an Item alone in
    // its Group, both stay put when moved to their current position.
    let other_group = workspace.favorites().groups[1].id;
    workspace.handle(Command::CreateFavoriteItem { group_id: other_group, name: "alone".into(), target: location("alone") }.into());
    let last = workspace.favorites().items.iter().find(|record| record.name == "first").unwrap().id;
    let alone = workspace.favorites().items.iter().find(|record| record.name == "alone").unwrap().id;
    for (id, group_id, position) in [(last, group, 1), (last, group, 9), (alone, other_group, 0)] {
        assert_eq!(rejection(&workspace.handle(Command::MoveFavoriteItem { id, group_id, position }.into())), Some(FavoriteRejection::Unchanged));
    }
}

/// A workspace whose stored Favorites hold one Group and the given Items,
/// with the launch probes it requested.
fn stored_favorites(items: &[(i64, &str, Location)]) -> (Workspace, Vec<WorkRequest>) {
    let mut workspace = Workspace::new();
    let favorites = dual_pane_application::FavoritesRecords { initialized: true, groups: vec![dual_pane_application::FavoriteGroupRecord { id: 1, name: "Places".into(), position: 0 }], items: items.iter().enumerate().map(|(position, (id, name, target))| dual_pane_application::FavoriteItemRecord { id: *id, group_id: 1, name: (*name).into(), target: target.clone(), position: position as i64 }).collect() };
    let loaded = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites, ..SettingsSnapshot::default() } }.into());
    let probes = loaded.work.into_iter().filter(|work| matches!(work, WorkRequest::ProbeFavoriteTarget { .. })).collect();
    (workspace, probes)
}

#[test]
fn launch_probes_every_stored_item_and_only_unavailable_removes_the_matching_target() {
    let old = location("old");
    let (mut workspace, probes) = stored_favorites(&[(4, "Old", old.clone()), (5, "Kept", location("kept")), (6, "Unreadable", location("unreadable"))]);
    assert_eq!(probes, vec![WorkRequest::ProbeFavoriteTarget { item_id: 4, target: old.clone() }, WorkRequest::ProbeFavoriteTarget { item_id: 5, target: location("kept") }, WorkRequest::ProbeFavoriteTarget { item_id: 6, target: location("unreadable") }]);
    assert!(workspace.handle(Event::FavoriteTargetProbed { item_id: 4, target: location("different"), outcome: FavoriteProbeOutcome::Unavailable }.into()).outputs.is_empty());
    assert_eq!(workspace.favorites().items.len(), 3);
    assert!(workspace.handle(Event::FavoriteTargetProbed { item_id: 5, target: location("kept"), outcome: FavoriteProbeOutcome::Available }.into()).outputs.is_empty());
    let failed = workspace.handle(Event::FavoriteTargetProbed { item_id: 6, target: location("unreadable"), outcome: FavoriteProbeOutcome::Failed }.into());
    assert!(matches!(failed.outputs.as_slice(), [Output::NoticeAdded { notice: Notice { kind: NoticeKind::FavoriteProbeFailed { name, .. }, offers_reset: false, .. }, open: true }] if name == "Unreadable"));
    assert_eq!(workspace.favorites().items.len(), 3);

    // A probe answers once; only the first answer for an Item counts.
    let (mut workspace, _) = stored_favorites(&[(4, "Old", old.clone())]);
    let removed = workspace.handle(Event::FavoriteTargetProbed { item_id: 4, target: old.clone(), outcome: FavoriteProbeOutcome::Unavailable }.into());
    assert!(workspace.favorites().items.is_empty());
    assert!(removed.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { .. })));
    assert!(removed.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice: Notice { kind: NoticeKind::FavoriteRemoved { name, target }, .. }, open: true } if name == "Old" && *target == old)));
    assert_eq!(workspace.handle(Event::FavoriteTargetProbed { item_id: 4, target: old, outcome: FavoriteProbeOutcome::Unavailable }.into()), Default::default());
}

#[test]
fn edits_and_deletion_while_a_probe_is_in_flight_are_respected() {
    let target = location("gone");
    let (mut workspace, _) = stored_favorites(&[(4, "Renamed later", target.clone()), (5, "Deleted later", location("also-gone"))]);
    workspace.handle(Command::RenameFavoriteItem { id: 4, name: "New alias".into() }.into());
    workspace.handle(Command::DeleteFavoriteItem { id: 5 }.into());
    let deleted = workspace.handle(Event::FavoriteTargetProbed { item_id: 5, target: location("also-gone"), outcome: FavoriteProbeOutcome::Unavailable }.into());
    assert_eq!(deleted, Default::default(), "a deleted Item adds no Notice and nothing to save");
    workspace.handle(Event::FavoriteTargetProbed { item_id: 4, target, outcome: FavoriteProbeOutcome::Unavailable }.into());
    assert!(workspace.favorites().items.is_empty(), "a renamed Item keeps its target, so it is still removed");
}

#[test]
fn no_probe_runs_for_a_failed_load_or_an_intentionally_empty_collection() {
    let mut failed = Workspace::new();
    let transition = failed.handle(Event::SettingsLoadFailed { failure: SettingsFailure::Corrupt }.into());
    assert!(transition.work.is_empty());
    let (_, probes) = stored_favorites(&[]);
    assert!(probes.is_empty());
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
    let refresh = workspace.handle(Event::LocationInvalidated { location: shared }.into());
    let reads = refresh.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count();
    assert_eq!(reads, 3);
}

fn favorite_names(workspace: &Workspace) -> Vec<&str> {
    workspace.favorites().items.iter().map(|item| item.name.as_str()).collect()
}

/// Loads uninitialized settings and returns the Screenshots folder the
/// workspace asked a worker to probe.
fn load_fresh_profile(workspace: &mut Workspace) -> Location {
    let transition = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    assert!(workspace.favorites().items.is_empty(), "nothing is seeded before the probe completes");
    match transition.work.as_slice() {
        [WorkRequest::ProbeScreenshotsFolder { location }] => location.clone(),
        other => panic!("expected only a Screenshots probe, got {other:?}"),
    }
}

#[test]
fn uninitialized_settings_seed_favorites_once_after_the_screenshots_probe() {
    let home = location("home");
    let mut workspace = Workspace::with_home(home.clone());
    let screenshots = load_fresh_profile(&mut workspace);
    assert_eq!(screenshots, home.join(&name("Documents")).join(&name("Screenshots")));
    let seeded = workspace.handle(Event::ScreenshotsFolderProbed { location: screenshots.clone(), outcome: FavoriteProbeOutcome::Available }.into());
    assert!(matches!(seeded.work.first(), Some(WorkRequest::SaveSettings { .. })));
    assert_eq!(seeded.work.iter().filter(|work| matches!(work, WorkRequest::ProbeFavoriteTarget { .. })).count(), 5, "seeded Items are probed too");
    assert_eq!(favorite_names(&workspace), ["Applications", "Desktop", "Documents", "Screenshots", "Downloads"]);
    assert!(workspace.handle(Event::ScreenshotsFolderProbed { location: screenshots, outcome: FavoriteProbeOutcome::Available }.into()).outputs.is_empty(), "a repeated probe result seeds nothing");
    let count = workspace.favorites().items.len();
    workspace.handle(Event::SettingsLoaded { snapshot: dual_pane_application::SettingsSnapshot { favorites: workspace.favorites().clone(), ..SettingsSnapshot::default() } }.into());
    assert_eq!(workspace.favorites().items.len(), count);
}

#[test]
fn only_a_probe_that_found_screenshots_adds_it() {
    for outcome in [FavoriteProbeOutcome::Unavailable, FavoriteProbeOutcome::Failed, FavoriteProbeOutcome::Cancelled] {
        let mut workspace = Workspace::with_home(location("home"));
        let screenshots = load_fresh_profile(&mut workspace);
        workspace.handle(Event::ScreenshotsFolderProbed { location: screenshots, outcome }.into());
        assert_eq!(favorite_names(&workspace), ["Applications", "Desktop", "Documents", "Downloads"], "{outcome:?}");
    }
}

#[test]
fn favorites_edits_wait_for_the_screenshots_probe() {
    let mut workspace = Workspace::with_home(location("home"));
    let screenshots = load_fresh_profile(&mut workspace);
    assert_eq!(workspace.handle(Command::CreateFavoriteGroup { name: "Early".into() }.into()), dual_pane_application::Transition { outputs: vec![Output::FavoriteEditRejected { edit: FavoriteEdit::CreateGroup, reason: FavoriteRejection::NotReady }], work: vec![] });
    workspace.handle(Event::ScreenshotsFolderProbed { location: screenshots, outcome: FavoriteProbeOutcome::Unavailable }.into());
    assert!(!workspace.handle(Command::CreateFavoriteGroup { name: "Later".into() }.into()).outputs.is_empty());
    assert!(workspace.favorites().groups.iter().any(|group| group.name == "Later"));
}

#[test]
fn tab_local_selection_gestures_obey_anchor_and_secondary_rules() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("items") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("a"), entry("b"), entry("c")]);
    assert!(workspace.handle(Command::SelectRange { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 2, name: name("c") }.into()).outputs.is_empty());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 0, name: name("a") }.into());
    workspace.handle(Command::SelectRange { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 2, name: name("c") }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("a"), name("b"), name("c")]);
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 2, name: name("c") }.into());
    assert!(!workspace.selection(BrowserSide::Left).contains(&name("c")));
    assert!(workspace.handle(Command::SecondarySelect { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 1, name: name("b") }.into()).outputs.is_empty());
    workspace.handle(Command::SecondarySelect { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 2, name: name("c") }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("c")]);
    workspace.handle(Command::SelectAll { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into());
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
    let transition = workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), location: shared, sort: dual_pane_domain::SortSpec::new(dual_pane_domain::SortField::Size, dual_pane_domain::SortDirection::Descending) }.into());
    assert_eq!(transition.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count(), 2);
}

#[test]
fn sorting_a_pending_navigation_restarts_its_target_without_committing_the_old_result() {
    let mut workspace = ready();
    let (tab, first) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("shown") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, first, vec![entry("old")]);
    let target = location("target");
    let (right, right_token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: target.clone() }.into()).work);
    load(&mut workspace, BrowserSide::Right, right, right_token, vec![entry("right")]);
    let (_, stale) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: target.clone() }.into()).work);
    let sort = SortSpec::new(SortField::Size, SortDirection::Descending);
    let changed = workspace.handle(Command::SetSort { browser: BrowserSide::Right, tab: right, location: target.clone(), sort }.into());
    assert!(changed.work.contains(&WorkRequest::Cancel { browser: BrowserSide::Left, tab, token: stale }));
    let current = changed.work.iter().find_map(|work| match work {
        WorkRequest::ReadDirectory { browser: BrowserSide::Left, location, sort: requested, token, .. } if *location == target && *requested == sort => Some(*token),
        _ => None,
    });
    let current = current.expect("the pending target is reread with the new sort");
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
    workspace.handle(Command::SecondarySelect { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 0, name: name("a") }.into());
    assert!(workspace.handle(Command::SelectRange { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 1, name: name("b") }.into()).outputs.is_empty());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("a")]);
}

#[test]
fn recreating_a_favorite_does_not_reuse_an_id_from_an_in_flight_probe() {
    let mut workspace = ready();
    workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into());
    let group = workspace.favorites().groups[0].id;
    let target = location("same-target");
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "First".into(), target: target.clone() }.into());
    let old_id = workspace.favorites().items[0].id;
    workspace.handle(Command::DeleteFavoriteItem { id: old_id }.into());
    workspace.handle(Command::CreateFavoriteItem { group_id: group, name: "Second".into(), target: target.clone() }.into());
    let new_id = workspace.favorites().items[0].id;
    assert_ne!(new_id, old_id);
    workspace.handle(Event::FavoriteTargetProbed { item_id: old_id, target, outcome: FavoriteProbeOutcome::Unavailable }.into());
    assert_eq!(workspace.favorites().items[0].id, new_id);
}

#[test]
fn scroll_hint_is_exposed_when_history_and_tabs_are_restored() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("one") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![entry("anchor")]);
    workspace.handle(Command::UpdateScrollHint { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), scroll: Some(ScrollAnchor::new(name("anchor"), 7)) }.into());
    let (_, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("two") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![]);
    let (_, token) = request(&workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work);
    let restored = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, token, entries: Arc::from(vec![entry("anchor")]), changes: None }.into());
    assert!(restored.outputs.iter().any(|output| matches!(output, dual_pane_application::Output::FolderItemsLoaded { scroll_hint: Some(scroll), .. } if *scroll == ScrollAnchor::new(name("anchor"), 7))));

    workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let switched = workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab: first }.into());
    assert!(switched.outputs.iter().any(|output| matches!(output, dual_pane_application::Output::TabViewChanged { scroll_hint: Some(scroll), .. } if *scroll == ScrollAnchor::new(name("anchor"), 7))));
}

#[test]
fn settings_are_never_saved_before_a_successful_load() {
    let mut workspace = Workspace::new();
    let shared = location("shared");
    let sort = SortSpec::new(SortField::Size, SortDirection::Descending);
    let tab = show(&mut workspace, BrowserSide::Left, &shared);
    let early = workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab, location: shared.clone(), sort }.into());
    assert!(!early.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { .. })));
    assert_eq!(rejection(&workspace.handle(Command::CreateFavoriteGroup { name: "Early".into() }.into())), Some(FavoriteRejection::NotReady));
    assert_eq!(workspace.final_settings_save(), None);

    let stored = SettingsSnapshot { favorites: dual_pane_application::FavoritesRecords { initialized: true, groups: vec![dual_pane_application::FavoriteGroupRecord { id: 3, name: "Stored".into(), position: 0 }], items: vec![] }, ..SettingsSnapshot::default() };
    let loaded = workspace.handle(Event::SettingsLoaded { snapshot: stored }.into());
    let saved = loaded.work.iter().find_map(|work| match work {
        WorkRequest::SaveSettings { snapshot, .. } => Some(snapshot.clone()),
        _ => None,
    });
    let saved = saved.expect("the replayed sort is saved over the loaded snapshot");
    assert_eq!(saved.folder_sorts, vec![(shared, sort)]);
    assert_eq!(saved.favorites.groups[0].name, "Stored");
    assert_eq!(workspace.settings_status(), SettingsStatus::Loaded);
}

#[test]
fn a_failed_load_keeps_edits_in_memory_without_saving() {
    let mut workspace = Workspace::new();
    let failed = workspace.handle(Event::SettingsLoadFailed { failure: SettingsFailure::Corrupt }.into());
    assert_eq!(failed.outputs, vec![Output::SettingsLoadFailed { failure: SettingsFailure::Corrupt }, Output::NoticeAdded { notice: Notice { id: 1, kind: NoticeKind::SettingsLoadFailed { failure: SettingsFailure::Corrupt }, offers_reset: true }, open: true }]);
    let group = workspace.handle(Command::CreateFavoriteGroup { name: "Places".into() }.into());
    assert!(!group.outputs.is_empty());
    assert!(group.work.is_empty());
    let tab = show(&mut workspace, BrowserSide::Left, &location("any"));
    let sort = workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab, location: location("any"), sort: SortSpec::new(SortField::Name, SortDirection::Descending) }.into());
    assert!(!sort.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { .. })));
    assert_eq!(workspace.final_settings_save(), None);
}

#[test]
fn only_changes_after_a_load_need_a_final_save() {
    let mut workspace = ready();
    let tab = show(&mut workspace, BrowserSide::Left, &location("any"));
    assert_eq!(workspace.final_settings_save(), None);
    workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab, location: location("any"), sort: SortSpec::new(SortField::Name, SortDirection::Descending) }.into());
    assert!(matches!(workspace.final_settings_save(), Some(WorkRequest::SaveSettings { .. })));
}

#[test]
fn repeated_back_and_forward_while_loading_keep_moving() {
    let mut workspace = Workspace::new();
    for folder in ["a", "b", "c"] {
        let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location(folder) }.into()).work);
        load(&mut workspace, BrowserSide::Left, tab, token, vec![]);
    }
    let read_location = |work: &[WorkRequest]| match work.last() {
        Some(WorkRequest::ReadDirectory { location, .. }) => location.clone(),
        _ => panic!("read"),
    };
    assert_eq!(read_location(&workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work), location("b"));
    assert_eq!(read_location(&workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work), location("a"));
    assert!(!workspace.can_go_back(BrowserSide::Left));
    assert!(workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work.is_empty());
    assert!(workspace.can_go_forward(BrowserSide::Left));
    let (tab, token) = request(&workspace.handle(Command::GoForward { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![]);
    assert_eq!(workspace.location(BrowserSide::Left), Some(&location("b")));
    assert!(workspace.can_go_back(BrowserSide::Left) && workspace.can_go_forward(BrowserSide::Left));
}

#[test]
fn a_refresh_keeps_present_selected_items_in_order_and_drops_missing_ones() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b"), entry("c")]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 1, name: name("b") }.into());
    workspace.handle(Command::SelectAll { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into());
    let (_, token) = request(&workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work);
    let refreshed = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("a"), entry("c"), entry("d")]), changes: None }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("a"), name("c")]);
    assert!(refreshed.outputs.contains(&Output::SelectionChanged { browser: BrowserSide::Left, tab, selection: workspace.selection(BrowserSide::Left).clone(), row: None }), "the missing cursor is cleared rather than moved");
    assert!(workspace.handle(Command::SelectRange { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 2, name: name("d") }.into()).outputs.is_empty(), "the missing anchor is cleared");
}

#[test]
fn reordering_a_tab_to_its_current_position_reports_nothing() {
    let mut workspace = Workspace::new();
    let (first, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("one") }.into()).work);
    load(&mut workspace, BrowserSide::Left, first, token, vec![]);
    let (second, _) = request(&workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into()).work);
    assert_eq!(workspace.handle(Command::ReorderTab { browser: BrowserSide::Left, tab: second, position: 1 }.into()), Default::default());
    assert_eq!(workspace.handle(Command::ReorderTab { browser: BrowserSide::Left, tab: second, position: 9 }.into()), Default::default());
    let moved = workspace.handle(Command::ReorderTab { browser: BrowserSide::Left, tab: second, position: 0 }.into());
    assert_eq!(moved.outputs, vec![Output::TabsChanged { browser: BrowserSide::Left, active_tab: second }]);
    assert_eq!(workspace.tabs(BrowserSide::Left).collect::<Vec<_>>(), vec![second, first]);
}

fn read_previous(work: &[WorkRequest]) -> Option<Arc<[Entry]>> {
    match work.last() {
        Some(WorkRequest::ReadDirectory { previous, .. }) => previous.clone(),
        _ => panic!("read"),
    }
}

#[test]
fn a_reload_asks_the_gateway_to_diff_against_the_shown_rows_and_forwards_its_change() {
    let mut workspace = Workspace::new();
    let first = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into());
    assert_eq!(read_previous(&first.work), None, "a new folder has nothing to diff against");
    let (tab, token) = request(&first.work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    let refresh = workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into());
    assert_eq!(read_previous(&refresh.work).as_deref(), Some(&[entry("a"), entry("b")][..]));
    let (_, token) = request(&refresh.work);
    let change = RowChange { row: 1, removed: 0, inserted: 1 };
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("a"), entry("ab"), entry("b")]), changes: Some(vec![change.clone()]) }.into());
    assert!(matches!(&loaded.outputs[0], Output::FolderItemsLoaded { tab: loaded_tab, changes: Some(changes), .. } if *loaded_tab == tab && *changes == vec![change]));
    let other = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("other") }.into());
    assert_eq!(read_previous(&other.work), None);
}

#[test]
fn a_gateway_change_that_does_not_fit_the_shown_rows_replaces_every_row() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    for wrong in [RowChange { row: 2, removed: 1, inserted: 1 }, RowChange { row: 0, removed: 0, inserted: 5 }] {
        let (_, token) = request(&workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work);
        let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("a"), entry("c")]), changes: Some(vec![wrong]) }.into());
        assert!(matches!(&loaded.outputs[0], Output::FolderItemsLoaded { changes: None, .. }));
    }
}

#[test]
fn another_folders_listing_replaces_every_row_even_with_a_fitting_change() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    let (_, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("other") }.into()).work);
    let fitting = RowChange { row: 0, removed: 2, inserted: 2 };
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("x"), entry("y")]), changes: Some(vec![fitting]) }.into());
    assert!(matches!(&loaded.outputs[0], Output::FolderItemsLoaded { changes: None, .. }));
}

#[test]
fn a_cleared_selection_stays_without_a_cursor_row_after_a_reload() {
    let mut workspace = Workspace::new();
    let (tab, token) = request(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("folder") }.into()).work);
    load(&mut workspace, BrowserSide::Left, tab, token, vec![entry("a"), entry("b")]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left), row: 1, name: name("b") }.into());
    let cleared = workspace.handle(Command::ClearSelection { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into());
    assert!(matches!(&cleared.outputs[..], [Output::SelectionChanged { row: None, .. }]));
    assert_eq!(workspace.handle(Command::ClearSelection { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).outputs, vec![]);
    let (_, token) = request(&workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab: workspace.active_tab(BrowserSide::Left) }.into()).work);
    let reloaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("a"), entry("b")]), changes: None }.into());
    assert!(reloaded.outputs.iter().any(|output| matches!(output, Output::SelectionChanged { row: None, .. })));
}

#[test]
fn restoration_discards_only_conclusive_first_read_failures() {
    let left = location("restored-left");
    let right = location("restored-right");
    let layout = WindowLayout { frame: WindowFrame { x: 1, y: 2, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400 };
    let session = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: left.clone() }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: right }], active_tab: Some(0) }, active_browser: BrowserSide::Left, layout };
    let mut workspace = Workspace::with_home(location("home"));
    let restored = workspace.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session: SessionSnapshot::Saved(session) }.into());
    let (tab, token) = restored
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, token, location, .. } if *location == left => Some((*tab, *token)),
            _ => None,
        })
        .expect("left restoration read");
    let busy = workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token, kind: dual_pane_domain::ListingErrorKind::Busy }.into());
    assert!(!busy.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice, .. } if matches!(notice.kind, NoticeKind::TabDiscarded { .. }))));
    let retry = workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into());
    let retry_token = retry
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: read_tab, token, .. } if *read_tab == tab => Some(*token),
            _ => None,
        })
        .expect("retry read");
    let discarded = workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token: retry_token, kind: dual_pane_domain::ListingErrorKind::PermissionDenied }.into());
    assert!(discarded.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice, .. } if matches!(notice.kind, NoticeKind::TabDiscarded { ref location, reason: dual_pane_domain::ListingErrorKind::PermissionDenied } if *location == left))));
    assert!(discarded.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { browser: BrowserSide::Left, location: folder, .. } if *folder == location("home"))));
}

#[test]
fn restoration_dispatches_active_tabs_before_remaining_tabs_in_strip_order() {
    let location = |name| location(name);
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400 };
    let saved = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("left-first") }, TabSnapshot { location: location("left-active") }], active_tab: Some(1) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("right-active") }, TabSnapshot { location: location("right-second") }], active_tab: Some(0) }, active_browser: BrowserSide::Right, layout };
    let mut workspace = Workspace::with_home(location("home"));
    let restored = workspace.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session: SessionSnapshot::Saved(saved) }.into());
    let order = restored
        .work
        .iter()
        .filter_map(|work| match work {
            WorkRequest::ReadDirectory { location, .. } => Some(location.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(order, vec![location("right-active"), location("left-active"), location("left-first"), location("right-second")]);
}

#[test]
fn a_late_settings_answer_after_timeout_never_restores_or_saves_the_prior_session() {
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400 };
    let saved = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("saved-left") }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("saved-right") }], active_tab: Some(0) }, active_browser: BrowserSide::Right, layout };
    let mut workspace = Workspace::with_home(location("home"));
    let timed_out = workspace.handle(Event::SettingsLoadTimedOut.into());
    assert!(timed_out.outputs.iter().any(|output| matches!(output, Output::SessionRestored { layout: None, .. })));
    let late = workspace.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session: SessionSnapshot::Saved(saved) }.into());
    assert!(!late.outputs.iter().any(|output| matches!(output, Output::SessionRestored { .. })));
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
    assert_eq!(workspace.location(BrowserSide::Left), None, "the Home fallback is still loading");
    assert!(workspace.final_session_save().is_none());
}

#[test]
fn first_settings_answer_saves_a_layout_reported_while_loading() {
    let layout = WindowLayout { frame: WindowFrame { x: 40, y: 50, width: 900, height: 700 }, state: WindowState::Normal, sidebar_splitter: 240, browser_splitter: 430 };
    let mut workspace = Workspace::with_home(location("home"));
    assert!(workspace.handle(Command::UpdateWindowLayout { layout }.into()).work.is_empty());

    let loaded = workspace.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session: SessionSnapshot::Absent }.into());
    assert!(matches!(loaded.work.as_slice(), work if work.iter().any(|request| matches!(request, WorkRequest::SaveSession { session, .. } if session.layout == layout))));
}
