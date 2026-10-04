use std::sync::Arc;

use dual_pane_application::{BrowserSnapshot, Command, Event, MonitoringStatus, Output, SessionSnapshot, SettingsSnapshot, TabSnapshot, WindowFrame, WindowLayout, WindowState, WorkRequest, Workspace, WorkspaceSnapshot};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingErrorKind, Location, RequestToken, ScrollAnchor, TabId};

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}

fn location(value: &str) -> Location {
    Location::root().join(&name(value))
}

fn entry(value: &str) -> Entry {
    Entry::new(name(value), EntryKind::File)
}

fn read(work: &[WorkRequest], browser: BrowserSide, location: &Location) -> (TabId, RequestToken) {
    work.iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser: owner, tab, token, location: target, .. } if *owner == browser && target == location => Some((*tab, *token)),
            _ => None,
        })
        .expect("matching read")
}

fn generation(work: &[WorkRequest], location: &Location) -> u64 {
    work.iter()
        .find_map(|work| match work {
            WorkRequest::WatchLocation { location: target, generation } if target == location => Some(*generation),
            _ => None,
        })
        .expect("matching watch")
}

fn load(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, token: RequestToken, rows: &[&str]) -> dual_pane_application::Transition {
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: rows.iter().map(|row| entry(row)).collect(), changes: None }.into())
}

#[test]
fn shared_locations_have_one_subscription_until_the_last_tab_leaves() {
    let mut workspace = Workspace::new();
    let shared = location("shared");
    let left = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: shared.clone() }.into());
    let shared_generation = generation(&left.work, &shared);
    let (left_tab, left_token) = read(&left.work, BrowserSide::Left, &shared);
    load(&mut workspace, BrowserSide::Left, left_tab, left_token, &[]);

    let right = workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: shared.clone() }.into());
    assert!(!right.work.iter().any(|work| matches!(work, WorkRequest::WatchLocation { location, .. } if *location == shared)));
    let (right_tab, right_token) = read(&right.work, BrowserSide::Right, &shared);
    load(&mut workspace, BrowserSide::Right, right_tab, right_token, &[]);

    let left_other = location("left-other");
    let moved = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: left_other.clone() }.into());
    let (tab, token) = read(&moved.work, BrowserSide::Left, &left_other);
    let settled = load(&mut workspace, BrowserSide::Left, tab, token, &[]);
    assert!(!settled.work.iter().any(|work| matches!(work, WorkRequest::UnwatchLocation { location, .. } if *location == shared)));

    let right_other = location("right-other");
    let moved = workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: right_other.clone() }.into());
    let (tab, token) = read(&moved.work, BrowserSide::Right, &right_other);
    let settled = load(&mut workspace, BrowserSide::Right, tab, token, &[]);
    assert!(settled.work.iter().any(|work| matches!(work, WorkRequest::UnwatchLocation { location, generation } if *location == shared && *generation == shared_generation)));
}

#[test]
fn initial_watch_race_and_invalidation_storm_finish_then_reread_once() {
    let mut workspace = Workspace::new();
    let folder = location("folder");
    let transition = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder.clone() }.into());
    let watch = generation(&transition.work, &folder);
    let (tab, token) = read(&transition.work, BrowserSide::Left, &folder);

    let ready = workspace.handle(Event::WatcherReady { location: folder.clone(), generation: watch, status: MonitoringStatus::Native }.into());
    assert!(!ready.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { .. } | WorkRequest::Cancel { .. })));
    for _ in 0..20 {
        let storm = workspace.handle(Event::WatchedLocationInvalidated { location: folder.clone(), generation: watch }.into());
        assert!(!storm.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { .. } | WorkRequest::Cancel { .. })));
    }
    let finished = load(&mut workspace, BrowserSide::Left, tab, token, &["a"]);
    assert_eq!(finished.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count(), 1);
    assert!(!finished.work.iter().any(|work| matches!(work, WorkRequest::Cancel { .. })));
}

#[test]
fn navigation_and_new_generation_win_over_stale_callbacks() {
    let mut workspace = Workspace::new();
    let first = location("first");
    let opened = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: first.clone() }.into());
    let old_generation = generation(&opened.work, &first);
    let (tab, token) = read(&opened.work, BrowserSide::Left, &first);
    load(&mut workspace, BrowserSide::Left, tab, token, &[]);

    let second = location("second");
    let moved = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: second.clone() }.into());
    let (tab, token) = read(&moved.work, BrowserSide::Left, &second);
    let settled = load(&mut workspace, BrowserSide::Left, tab, token, &[]);
    assert!(settled.work.iter().any(|work| matches!(work, WorkRequest::UnwatchLocation { location, generation } if *location == first && *generation == old_generation)));

    let returned = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: first.clone() }.into());
    let new_generation = generation(&returned.work, &first);
    assert_ne!(old_generation, new_generation);
    let (tab, token) = read(&returned.work, BrowserSide::Left, &first);
    load(&mut workspace, BrowserSide::Left, tab, token, &[]);
    assert!(workspace.handle(Event::WatchedLocationInvalidated { location: first.clone(), generation: old_generation }.into()).work.is_empty());
    assert_eq!(workspace.handle(Event::WatchedLocationInvalidated { location: first, generation: new_generation }.into()).work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count(), 1);
}

#[test]
fn watcher_reads_pause_in_background_and_foreground_return_reconciles_all() {
    let mut workspace = Workspace::new();
    let folder = location("folder");
    let opened = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder.clone() }.into());
    let watch = generation(&opened.work, &folder);
    let (tab, token) = read(&opened.work, BrowserSide::Left, &folder);
    load(&mut workspace, BrowserSide::Left, tab, token, &[]);

    let hidden = workspace.handle(Event::ApplicationActivityChanged { active: false }.into());
    assert!(hidden.work.contains(&WorkRequest::SetWatchActivity { active: false }));
    for _ in 0..5 {
        assert!(!workspace.handle(Event::WatchedLocationInvalidated { location: folder.clone(), generation: watch }.into()).work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { .. })));
    }
    let foreground = workspace.handle(Event::ApplicationActivityChanged { active: true }.into());
    assert!(foreground.work.contains(&WorkRequest::SetWatchActivity { active: true }));
    assert_eq!(foreground.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { location, .. } if *location == folder)).count(), 1);
}

#[test]
fn missing_location_can_return_without_losing_matching_visit_state() {
    let mut workspace = Workspace::new();
    let folder = location("folder");
    let opened = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder.clone() }.into());
    let watch = generation(&opened.work, &folder);
    let (tab, token) = read(&opened.work, BrowserSide::Left, &folder);
    load(&mut workspace, BrowserSide::Left, tab, token, &["a", "b"]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 1, name: name("b") }.into());
    workspace.handle(Command::UpdateScrollHint { browser: BrowserSide::Left, tab, scroll: Some(ScrollAnchor::new(name("b"), 6)) }.into());

    let invalidated = workspace.handle(Event::WatchedLocationInvalidated { location: folder.clone(), generation: watch }.into());
    let (_, failed_token) = read(&invalidated.work, BrowserSide::Left, &folder);
    let failed = workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token: failed_token, kind: ListingErrorKind::ItemMissing }.into());
    assert!(failed.outputs.iter().any(|output| matches!(output, Output::FolderItemsFailed { .. })));
    assert_eq!(workspace.entries(BrowserSide::Left).len(), 2, "the last successful rows stay visible");

    let returned = workspace.handle(Event::WatchedLocationInvalidated { location: folder.clone(), generation: watch }.into());
    let (_, returned_token) = read(&returned.work, BrowserSide::Left, &folder);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token: returned_token, entries: Arc::from([entry("b"), entry("c")]), changes: None }.into());
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("b")]);
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::SelectionChanged { row: Some(0), .. })));
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::FolderItemsLoaded { scroll_hint: Some(scroll), .. } if *scroll == ScrollAnchor::new(name("b"), 6))));
}

#[test]
fn restoration_shares_subscriptions_and_reset_replaces_them_without_leaks() {
    let shared = location("shared");
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400 };
    let saved = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: shared.clone() }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: shared.clone() }], active_tab: Some(0) }, active_browser: BrowserSide::Left, layout };
    let home = location("home");
    let mut workspace = Workspace::with_home(home.clone());
    let restored = workspace.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session: SessionSnapshot::Saved(saved) }.into());
    assert_eq!(restored.work.iter().filter(|work| matches!(work, WorkRequest::WatchLocation { location, .. } if *location == shared)).count(), 1);
    assert_eq!(restored.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { location, .. } if *location == shared)).count(), 2);
    let first_read = restored.work.iter().position(|work| matches!(work, WorkRequest::ReadDirectory { .. })).unwrap();
    let watch = restored.work.iter().position(|work| matches!(work, WorkRequest::WatchLocation { .. })).unwrap();
    assert!(watch < first_read, "monitoring starts before restoration listings");

    let reset = workspace.handle(Command::ResetSettings.into());
    assert!(reset.work.iter().any(|work| matches!(work, WorkRequest::ResetSettings)));
    let replaced = workspace.handle(Event::SettingsReset { backup: None }.into());
    assert!(replaced.work.iter().any(|work| matches!(work, WorkRequest::UnwatchLocation { location, .. } if *location == shared)));
    assert_eq!(replaced.work.iter().filter(|work| matches!(work, WorkRequest::WatchLocation { location, .. } if *location == home)).count(), 1);
    assert_eq!(replaced.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { location, .. } if *location == home)).count(), 2);
}
