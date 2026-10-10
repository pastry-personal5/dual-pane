//! The application contract the tabbed desktop interface renders and drives:
//! projections, tab-addressed gestures, typed Favorites rejections, Notices,
//! and confirmed settings reset.

use std::sync::Arc;

use dual_pane_application::{Command, Event, FavoriteEdit, FavoriteGroupRecord, FavoriteItemRecord, FavoriteRejection, FavoritesRecords, NoticeKind, Output, SettingsFailure, SettingsSnapshot, SettingsStatus, Transition, WindowFrame, WindowLayout, WindowState, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, BrowserTabs, Entry, EntryKind, EntryName, Location, RequestToken, SortDirection, SortField, SortSpec, TabId};

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}
fn location(value: &str) -> Location {
    Location::root().join(&name(value))
}
fn read(work: &[WorkRequest]) -> (TabId, RequestToken) {
    work.iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { tab, token, .. } => Some((*tab, *token)),
            _ => None,
        })
        .expect("a directory read")
}
fn load(workspace: &mut Workspace, browser: BrowserSide, work: &[WorkRequest], entries: &[&str]) -> TabId {
    let (tab, token) = read(work);
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries.iter().map(|entry| Entry::new(name(entry), EntryKind::File)).collect(), changes: None }.into());
    tab
}
fn show(workspace: &mut Workspace, browser: BrowserSide, at: &Location, entries: &[&str]) -> TabId {
    let work = workspace.handle(Command::Navigate { browser, location: at.clone() }.into()).work;
    load(workspace, browser, &work, entries)
}
fn ready() -> Workspace {
    let mut workspace = Workspace::new();
    let transition = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites: FavoritesRecords { initialized: true, ..FavoritesRecords::default() }, ..SettingsSnapshot::default() } }.into());
    for work in transition.work {
        if let WorkRequest::ReadDirectory { browser, tab, token, .. } = work {
            workspace.handle(Event::FolderItemsCancelled { browser, tab, token }.into());
        }
    }
    workspace
}

#[test]
fn settings_interaction_availability_tracks_storage_health_and_reset_reload() {
    let mut workspace = Workspace::new();
    assert!(!workspace.settings_interaction_available());
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    assert!(workspace.settings_interaction_available());

    let failed = workspace.handle(Event::SettingsSaveFailed { revision: 1, failure: SettingsFailure::Unavailable }.into());
    assert!(failed.outputs.iter().any(|output| matches!(output, Output::SettingsSaveFailed { .. })));
    assert!(!workspace.settings_interaction_available());
    workspace.handle(Event::SettingsSaved { revision: 1 }.into());
    assert!(!workspace.settings_interaction_available(), "an ordinary later save never reopens Settings");

    let reset = workspace.handle(Command::ResetSettings.into());
    assert_eq!(reset.work, vec![WorkRequest::ResetSettings]);
    assert!(!workspace.settings_interaction_available());
    workspace.handle(Event::SettingsReset { backup: None }.into());
    assert!(!workspace.settings_interaction_available());
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    assert!(workspace.settings_interaction_available());

    let mut failed_load = Workspace::new();
    failed_load.handle(Event::SettingsLoadFailed { failure: SettingsFailure::Corrupt }.into());
    assert!(!failed_load.settings_interaction_available());
    let mut timed_out = Workspace::new();
    timed_out.handle(Event::SettingsLoadTimedOut.into());
    assert!(!timed_out.settings_interaction_available());
}
fn rejection(transition: &Transition) -> Option<FavoriteRejection> {
    match transition.outputs.as_slice() {
        [Output::FavoriteEditRejected { reason, .. }] => Some(*reason),
        _ => None,
    }
}

#[test]
fn each_browser_holds_at_most_the_tab_limit() {
    let mut workspace = ready();
    show(&mut workspace, BrowserSide::Left, &location("start"), &[]);
    show(&mut workspace, BrowserSide::Right, &location("start"), &[]);
    for _ in 1..BrowserTabs::LIMIT {
        assert!(workspace.browser_chrome(BrowserSide::Left).can_open_tab);
        let work = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into()).work;
        load(&mut workspace, BrowserSide::Left, &work, &[]);
    }
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert_eq!(chrome.tabs.len(), BrowserTabs::LIMIT);
    assert!(!chrome.can_open_tab);
    assert_eq!(workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into()), Transition::default());
    assert!(workspace.browser_chrome(BrowserSide::Right).can_open_tab, "the limit is per Browser");
}

#[test]
fn the_tab_projection_labels_pending_tabs_only_until_they_confirm_a_location() {
    let mut workspace = ready();
    show(&mut workspace, BrowserSide::Left, &location("shown"), &[]);
    workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("pending") }.into());
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert_eq!(chrome.tabs[0].location, Some(location("shown")));
    assert!(chrome.tabs[0].loading);
    assert_eq!(chrome.location, Some(location("shown")));

    let fresh = workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: location("first") }.into());
    let chrome = workspace.browser_chrome(BrowserSide::Right);
    assert_eq!(chrome.tabs[0].location, Some(location("first")), "a tab that never confirmed a location shows its target");
    assert_eq!(chrome.location, None);
    assert!(!chrome.can_open_tab && !chrome.can_go_up && chrome.can_refresh && chrome.sort.is_none());
    let (tab, token) = read(&fresh.work);
    workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Right, tab, token, kind: dual_pane_domain::ListingErrorKind::PermissionDenied }.into());
    assert!(workspace.browser_chrome(BrowserSide::Right).tabs[0].failed);
}

#[test]
fn history_and_parent_availability_follow_the_active_tab() {
    let mut workspace = ready();
    show(&mut workspace, BrowserSide::Left, &Location::root(), &[]);
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert!(!chrome.can_go_back && !chrome.can_go_forward && !chrome.can_go_up);
    show(&mut workspace, BrowserSide::Left, &location("child"), &[]);
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert!(chrome.can_go_back && !chrome.can_go_forward && chrome.can_go_up);
    let tab = workspace.active_tab(BrowserSide::Left);
    let back = workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab }.into()).work;
    assert!(workspace.browser_chrome(BrowserSide::Left).can_go_forward, "a pending Back counts as taken");
    load(&mut workspace, BrowserSide::Left, &back, &[]);
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert!(!chrome.can_go_back && chrome.can_go_forward);
    assert!(!workspace.browser_chrome(BrowserSide::Right).can_go_forward, "the other Browser is unaffected");
}

#[test]
fn gestures_observed_on_a_tab_that_is_no_longer_active_change_nothing() {
    let mut workspace = ready();
    let first = show(&mut workspace, BrowserSide::Left, &location("folder"), &["a", "b"]);
    let work = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into()).work;
    let second = load(&mut workspace, BrowserSide::Left, &work, &["a", "b"]);
    assert_ne!(first, second);
    let stale = [Command::SelectEntry { browser: BrowserSide::Left, tab: first, row: 0, name: name("a") }, Command::ToggleEntry { browser: BrowserSide::Left, tab: first, row: 0, name: name("a") }, Command::SelectAll { browser: BrowserSide::Left, tab: first }, Command::OpenEntry { browser: BrowserSide::Left, tab: first, row: 0, name: name("a") }, Command::Refresh { browser: BrowserSide::Left, tab: first }, Command::GoToParent { browser: BrowserSide::Left, tab: first }, Command::GoBack { browser: BrowserSide::Left, tab: first }];
    for command in stale {
        assert_eq!(workspace.handle(command.clone().into()), Transition::default(), "{command:?}");
    }
    assert!(workspace.selection(BrowserSide::Left).entries().is_empty());
    let refreshed = workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab: second }.into());
    assert_eq!(read(&refreshed.work).0, second);
}

#[test]
fn a_delayed_sort_click_cannot_change_another_locations_choice() {
    let mut workspace = ready();
    let tab = show(&mut workspace, BrowserSide::Left, &location("old"), &[]);
    show(&mut workspace, BrowserSide::Left, &location("new"), &[]);
    let size = SortSpec::new(SortField::Size, SortDirection::Descending);
    assert_eq!(workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab, location: location("old"), sort: size }.into()), Transition::default());
    assert_eq!(workspace.browser_chrome(BrowserSide::Left).sort, Some(SortSpec::default()));
    let applied = workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab, location: location("new"), sort: size }.into());
    assert!(applied.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { sort, .. } if *sort == size)));
    assert_eq!(workspace.browser_chrome(BrowserSide::Left).sort, Some(size));
}

#[test]
fn a_sort_updates_inactive_tabs_in_both_browsers_at_that_exact_location() {
    let mut workspace = ready();
    let shared = location("shared");
    let left = show(&mut workspace, BrowserSide::Left, &shared, &[]);
    let work = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into()).work;
    load(&mut workspace, BrowserSide::Left, &work, &[]);
    show(&mut workspace, BrowserSide::Left, &location("elsewhere"), &[]);
    show(&mut workspace, BrowserSide::Right, &shared, &[]);
    workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab: left }.into());
    let sort = SortSpec::new(SortField::Type, SortDirection::Ascending);
    let applied = workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab: left, location: shared.clone(), sort }.into());
    let reads = applied.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { location, sort: requested, .. } if *location == shared && *requested == sort)).count();
    assert_eq!(reads, 2, "the active Left tab and the Right tab reread; the tab elsewhere does not");
    assert_eq!(workspace.browser_chrome(BrowserSide::Right).sort, Some(sort));
}

fn stored(groups: &[(i64, &str)], items: &[(i64, i64, &str)]) -> Workspace {
    let mut workspace = Workspace::new();
    let favorites = FavoritesRecords { initialized: true, groups: groups.iter().enumerate().map(|(position, (id, name))| FavoriteGroupRecord { id: *id, name: (*name).into(), position: position as i64 }).collect(), items: items.iter().enumerate().map(|(index, (id, group_id, name))| FavoriteItemRecord { id: *id, group_id: *group_id, name: (*name).into(), target: location(name), position: items[..index].iter().filter(|(_, other, _)| other == group_id).count() as i64 }).collect() };
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites, ..SettingsSnapshot::default() } }.into());
    workspace
}

#[test]
fn the_same_alias_may_exist_in_two_groups_but_not_twice_in_one() {
    let mut workspace = stored(&[(1, "Work"), (2, "Home")], &[(10, 1, "Docs"), (11, 2, "Docs"), (12, 2, "Music")]);
    assert_eq!(rejection(&workspace.handle(Command::CreateFavoriteItem { group_id: 1, name: "Docs".into(), target: location("other") }.into())), Some(FavoriteRejection::DuplicateName));
    assert_eq!(rejection(&workspace.handle(Command::MoveFavoriteItem { id: 11, group_id: 1, position: 0 }.into())), Some(FavoriteRejection::DuplicateName), "a cross-group move that would duplicate an alias is rejected");
    assert_eq!(workspace.favorites().items.iter().find(|item| item.id == 11).map(|item| (item.group_id, item.position)), Some((2, 0)), "the rejected Item keeps its position");
    assert_eq!(rejection(&workspace.handle(Command::RenameFavoriteItem { id: 12, name: "Docs".into() }.into())), Some(FavoriteRejection::DuplicateName));
    let moved = workspace.handle(Command::MoveFavoriteItem { id: 12, group_id: 1, position: 0 }.into());
    assert!(matches!(moved.outputs.as_slice(), [Output::FavoritesChanged { .. }]));
    assert_eq!(workspace.favorites().items.iter().filter(|item| item.group_id == 1).map(|item| item.name.as_str()).collect::<Vec<_>>(), ["Music", "Docs"]);
}

#[test]
fn deleting_a_group_removes_its_children_and_reports_a_stale_target_afterwards() {
    let mut workspace = stored(&[(1, "Work"), (2, "Home")], &[(10, 1, "Docs"), (11, 1, "Notes"), (12, 2, "Music")]);
    let deleted = workspace.handle(Command::DeleteFavoriteGroup { id: 1 }.into());
    assert!(deleted.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { .. })));
    assert_eq!(workspace.favorites().items.iter().map(|item| item.id).collect::<Vec<_>>(), [12]);
    assert_eq!(workspace.favorites().groups.iter().map(|group| (group.id, group.position)).collect::<Vec<_>>(), [(2, 0)]);
    for command in [Command::DeleteFavoriteGroup { id: 1 }, Command::RenameFavoriteItem { id: 10, name: "Gone".into() }, Command::CreateFavoriteItem { group_id: 1, name: "Late".into(), target: location("late") }] {
        assert_eq!(rejection(&workspace.handle(command.into())), Some(FavoriteRejection::Stale));
    }
}

#[test]
fn rejections_name_the_edit_they_refused() {
    let mut workspace = stored(&[(1, "Work")], &[(10, 1, "Docs")]);
    let cases = [(Command::CreateFavoriteGroup { name: "".into() }, FavoriteEdit::CreateGroup, FavoriteRejection::InvalidName), (Command::RenameFavoriteGroup { id: 1, name: "Work".into() }, FavoriteEdit::Group(1), FavoriteRejection::Unchanged), (Command::CreateFavoriteItem { group_id: 1, name: "Docs".into(), target: location("x") }, FavoriteEdit::CreateItem { group_id: 1 }, FavoriteRejection::DuplicateName), (Command::RenameFavoriteItem { id: 10, name: "  ".into() }, FavoriteEdit::Item(10), FavoriteRejection::InvalidName)];
    for (command, edit, reason) in cases {
        assert_eq!(workspace.handle(command.into()).outputs, vec![Output::FavoriteEditRejected { edit, reason }]);
    }
}

#[test]
fn an_accepted_edit_stays_in_memory_when_its_save_fails_and_repeated_failures_add_one_notice() {
    let mut workspace = stored(&[(1, "Work")], &[]);
    let created = workspace.handle(Command::CreateFavoriteGroup { name: "Home".into() }.into());
    let Some(WorkRequest::SaveSettings { revision, .. }) = created.work.first().cloned() else { panic!("a save") };
    let failed = workspace.handle(Event::SettingsSaveFailed { revision, failure: SettingsFailure::Unavailable }.into());
    assert!(failed.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice, open: true } if notice.offers_reset && notice.kind == NoticeKind::SettingsSaveFailed { failure: SettingsFailure::Unavailable })));
    assert!(workspace.favorites().groups.iter().any(|group| group.name == "Home"));
    workspace.handle(Event::SettingsSaveFailed { revision, failure: SettingsFailure::Unavailable }.into());
    assert_eq!(workspace.notices().len(), 1);
    assert!(workspace.final_settings_save().is_some(), "the edit is not claimed as saved");
}

fn reset_ready() -> (Workspace, u64) {
    let mut workspace = stored(&[(1, "Work")], &[(10, 1, "Docs")]);
    let created = workspace.handle(Command::CreateFavoriteGroup { name: "Unsaved".into() }.into());
    let Some(WorkRequest::SaveSettings { revision, .. }) = created.work.first().cloned() else { panic!("a save") };
    (workspace, revision)
}

#[test]
fn a_confirmed_reset_reloads_fresh_settings_and_ignores_older_saves() {
    let (mut workspace, old_save) = reset_ready();
    let reset = workspace.handle(Command::ResetSettings.into());
    assert_eq!(reset.work, vec![WorkRequest::ResetSettings]);
    assert!(!workspace.favorites_ready(), "edits wait for the replacement settings");
    assert_eq!(rejection(&workspace.handle(Command::CreateFavoriteGroup { name: "During".into() }.into())), Some(FavoriteRejection::NotReady));
    assert_eq!(workspace.handle(Command::ResetSettings.into()), Transition::default(), "one reset at a time");
    assert_eq!(workspace.handle(Event::SettingsSaveFailed { revision: old_save, failure: SettingsFailure::Unavailable }.into()), Transition::default(), "a save from before the reset is stale");
    workspace.handle(Event::FavoriteTargetProbed { item_id: 10, target: location("Docs"), outcome: dual_pane_application::FavoriteProbeOutcome::Unavailable }.into());

    let backup = location("settings.failed");
    let done = workspace.handle(Event::SettingsReset { backup: Some(backup.clone()) }.into());
    assert!(done.work.iter().any(|work| matches!(work, WorkRequest::LoadSettings)));
    assert_eq!(done.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count(), 2);
    assert!(done.outputs.iter().any(|output| matches!(output, Output::LayoutReset)));
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
    assert_eq!(workspace.tabs(BrowserSide::Left).len(), 1);
    assert_eq!(workspace.tabs(BrowserSide::Right).len(), 1);
    assert!(done.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice, open: true } if notice.kind == NoticeKind::SettingsReset { backup: Some(backup.clone()) })));
    let reloaded = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    assert!(reloaded.work.iter().any(|work| matches!(work, WorkRequest::ProbeScreenshotsFolder { .. })), "a fresh database is seeded again");
    assert!(workspace.favorites().groups.is_empty(), "the current-session Favorites were replaced");
    assert_eq!(workspace.settings_status(), SettingsStatus::Loaded);
    assert!(!workspace.handle(Event::SettingsSaved { revision: old_save }.into()).outputs.iter().any(|_| true));
}

#[test]
fn a_second_reset_waits_until_the_reload_after_the_first_lands() {
    let (mut workspace, _) = reset_ready();
    assert_eq!(Workspace::new().handle(Command::ResetSettings.into()), Transition::default(), "no reset while the first load is pending");
    workspace.handle(Command::ResetSettings.into());
    workspace.handle(Event::SettingsReset { backup: None }.into());
    assert_eq!(workspace.handle(Command::ResetSettings.into()), Transition::default(), "the reload is still pending");
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites: FavoritesRecords { initialized: true, ..FavoritesRecords::default() }, ..SettingsSnapshot::default() } }.into());
    assert_eq!(workspace.settings_status(), SettingsStatus::Loaded);
    assert!(workspace.favorites().groups.is_empty(), "the reload applied");
    assert_eq!(workspace.handle(Command::ResetSettings.into()).work, vec![WorkRequest::ResetSettings]);
}

#[test]
fn a_failed_reset_keeps_the_error_visible_and_restores_the_previous_state() {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoadFailed { failure: SettingsFailure::Corrupt }.into());
    workspace.handle(Command::ResetSettings.into());
    let failed = workspace.handle(Event::SettingsResetFailed { failure: SettingsFailure::Unavailable }.into());
    assert!(failed.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice, open: true } if notice.offers_reset && notice.kind == NoticeKind::SettingsResetFailed { failure: SettingsFailure::Unavailable })));
    assert_eq!(workspace.settings_status(), SettingsStatus::LoadFailed(SettingsFailure::Corrupt));
    assert!(workspace.notices().iter().any(|notice| notice.kind == NoticeKind::SettingsLoadFailed { failure: SettingsFailure::Corrupt }), "the original error stays visible");
    assert_eq!(workspace.handle(Event::SettingsReset { backup: None }.into()), Transition::default(), "a late success for no pending reset claims nothing");
    assert_eq!(workspace.handle(Command::ResetSettings.into()).work, vec![WorkRequest::ResetSettings], "reset stays available after a failed open");
}

#[test]
fn without_a_settings_worker_nothing_offers_or_performs_a_reset() {
    let mut workspace = Workspace::new();
    let failed = workspace.handle(Event::SettingsLoadFailed { failure: SettingsFailure::WorkerUnavailable }.into());
    assert!(failed.outputs.iter().any(|output| matches!(output, Output::NoticeAdded { notice, open: true } if !notice.offers_reset)));
    assert_eq!(workspace.handle(Command::ResetSettings.into()), Transition::default());
}

#[test]
fn notices_startup_preference_waits_for_the_first_settings_answer_and_saves_promptly() {
    let mut workspace = Workspace::new();
    assert!(!workspace.workspace_chrome().notices_startup_ready);
    assert_eq!(workspace.handle(Command::SetHideNoticesAtStartup { hide: true }.into()), Transition::default());

    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot { hide_notices_at_startup: true, ..SettingsSnapshot::default() } }.into());
    let chrome = workspace.workspace_chrome();
    assert!(chrome.notices_startup_ready);
    assert!(chrome.hide_notices_at_startup);

    let changed = workspace.handle(Command::SetHideNoticesAtStartup { hide: false }.into());
    assert!(matches!(changed.work.as_slice(), [WorkRequest::SaveSettings { snapshot, .. }] if !snapshot.hide_notices_at_startup));
}

#[test]
fn a_startup_notice_waits_for_the_preference_before_opening_notices() {
    let mut workspace = Workspace::new();
    let notice = workspace.handle(Event::JournalStatus { available: false }.into());
    assert!(matches!(notice.outputs.as_slice(), [Output::NoticeAdded { open: false, .. }]));

    let loaded = workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::OpenNotices)));
}

#[test]
fn session_snapshot_keeps_requested_locations_for_unconfirmed_tabs() {
    let mut workspace = Workspace::new();
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 900, height: 700 }, state: WindowState::Normal, sidebar_splitter: 220, browser_splitter: 440 };
    workspace.handle(Command::UpdateWindowLayout { layout }.into());
    workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location("left-pending") }.into());
    workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: location("right-pending") }.into());
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    let snapshot = workspace.session_snapshot().expect("reported layout and both requested folders");
    assert_eq!(snapshot.left.tabs[0].location, location("left-pending"));
    assert_eq!(snapshot.right.tabs[0].location, location("right-pending"));
    assert_eq!(snapshot.layout, layout);
}

#[test]
fn the_workspace_projection_carries_bindings_readiness_and_notices() {
    let mut workspace = Workspace::new();
    let chrome = workspace.workspace_chrome();
    assert!(!chrome.favorites_ready);
    assert!(chrome.bindings.iter().any(|binding| binding.action == dual_pane_application::ActionId::CloseTab && binding.shortcut.is_some()));
    workspace.handle(Event::SettingsLoadFailed { failure: SettingsFailure::UnsupportedSchema }.into());
    let chrome = workspace.workspace_chrome();
    assert!(chrome.favorites_ready, "a failed load leaves Favorites editable in memory");
    assert_eq!(chrome.notices.len(), 1);
    assert_eq!(Arc::strong_count(&chrome.notices), 2, "the projection shares the Notices");
}
