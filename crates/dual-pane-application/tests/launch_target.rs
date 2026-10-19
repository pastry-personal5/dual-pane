use std::sync::Arc;

use dual_pane_application::{BrowserSnapshot, Command, Event, Output, SessionSnapshot, SettingsSnapshot, TabSnapshot, Transition, WindowFrame, WindowLayout, WindowState, WorkRequest, Workspace, WorkspaceSnapshot};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, RequestToken, TabId};

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}
fn location(value: &str) -> Location {
    Location::root().join(&name(value))
}
fn entry(value: &str) -> Entry {
    Entry::new(name(value), EntryKind::File)
}
fn saved(left: &[&str], left_active: usize) -> SessionSnapshot {
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 };
    let tabs = left.iter().map(|folder| TabSnapshot { location: location(folder) }).collect();
    SessionSnapshot::Saved(WorkspaceSnapshot { left: BrowserSnapshot { tabs, active_tab: Some(left_active) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("right") }], active_tab: Some(0) }, active_browser: BrowserSide::Right, layout })
}
fn restore(workspace: &mut Workspace, session: SessionSnapshot) -> Transition {
    workspace.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session }.into())
}
fn open(folder: &str, select: Option<&str>) -> Command {
    Command::OpenLaunchTarget { folder: location(folder), select: select.map(name) }
}
fn read_for(work: &[WorkRequest], folder: &Location) -> Option<(BrowserSide, TabId, RequestToken)> {
    work.iter().find_map(|work| match work {
        WorkRequest::ReadDirectory { browser, tab, token, location, .. } if location == folder => Some((*browser, *tab, *token)),
        _ => None,
    })
}
fn locations(workspace: &Workspace, browser: BrowserSide) -> Vec<Option<Location>> {
    workspace.browser_chrome(browser).tabs.into_iter().map(|tab| tab.location).collect()
}

#[test]
fn an_open_tab_on_the_folder_becomes_active_instead_of_a_duplicate() {
    let mut workspace = Workspace::with_home(location("home"));
    restore(&mut workspace, saved(&["first", "target", "last"], 0));
    let transition = workspace.handle(open("target", None).into());
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert_eq!(chrome.tabs.len(), 3, "no tab is added");
    assert_eq!(chrome.tabs.iter().position(|tab| tab.id == chrome.active_tab), Some(1));
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
    assert!(transition.outputs.iter().any(|output| matches!(output, Output::ActiveBrowserChanged { browser: BrowserSide::Left })));
}

#[test]
fn a_missing_tab_opens_in_the_active_browser_and_reads_the_folder() {
    let mut workspace = Workspace::with_home(location("home"));
    restore(&mut workspace, saved(&["first"], 0));
    let transition = workspace.handle(open("target", None).into());
    assert_eq!(locations(&workspace, BrowserSide::Right), vec![Some(location("right")), Some(location("target"))]);
    let chrome = workspace.browser_chrome(BrowserSide::Right);
    assert_eq!(chrome.active_tab, chrome.tabs[1].id);
    assert_eq!(workspace.active_browser(), BrowserSide::Right);
    assert!(read_for(&transition.work, &location("target")).is_some());
    assert!(transition.outputs.iter().any(|output| matches!(output, Output::ActivateWindow { focus_browser: Some(BrowserSide::Right) })));
}

#[test]
fn a_file_target_is_selected_once_its_folder_loads() {
    let mut workspace = Workspace::with_home(location("home"));
    restore(&mut workspace, saved(&["first"], 0));
    let transition = workspace.handle(open("target", Some("wanted")).into());
    let (browser, tab, token) = read_for(&transition.work, &location("target")).expect("read");
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(vec![entry("other"), entry("wanted")]), changes: None }.into());
    let selected = loaded.outputs.iter().find_map(|output| match output {
        Output::SelectionChanged { selection, row, .. } => Some((selection.selected().cloned(), *row)),
        _ => None,
    });
    assert_eq!(selected, Some((Some(name("wanted")), Some(1))));
}

#[test]
fn an_existing_tab_rereads_so_its_file_target_gets_selected() {
    let mut workspace = Workspace::with_home(location("home"));
    restore(&mut workspace, saved(&["target"], 0));
    let transition = workspace.handle(open("target", Some("wanted")).into());
    assert_eq!(workspace.browser_chrome(BrowserSide::Left).tabs.len(), 1);
    let (browser, tab, token) = read_for(&transition.work, &location("target")).expect("reread");
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(vec![entry("wanted")]), changes: None }.into());
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::SelectionChanged { row: Some(0), .. })));
}

#[test]
fn a_target_that_arrives_before_the_session_restores_waits_for_it() {
    let mut workspace = Workspace::with_home(location("home"));
    let early = workspace.handle(open("target", None).into());
    assert!(early.outputs.is_empty() && early.work.is_empty(), "nothing happens while settings load");
    let restored = restore(&mut workspace, saved(&["first"], 0));
    assert!(read_for(&restored.work, &location("target")).is_some());
    assert_eq!(locations(&workspace, BrowserSide::Right), vec![Some(location("right")), Some(location("target"))]);
    assert_eq!(workspace.active_browser(), BrowserSide::Right);
}

#[test]
fn a_target_also_applies_after_a_fresh_profile_and_after_a_settings_timeout() {
    let mut fresh = Workspace::with_home(location("home"));
    fresh.handle(open("target", None).into());
    restore(&mut fresh, SessionSnapshot::Absent);
    assert_eq!(locations(&fresh, BrowserSide::Left).last(), Some(&Some(location("target"))));

    let mut timed_out = Workspace::with_home(location("home"));
    timed_out.handle(open("target", None).into());
    timed_out.handle(Event::SettingsLoadTimedOut.into());
    assert_eq!(locations(&timed_out, BrowserSide::Left).last(), Some(&Some(location("target"))));
}

#[test]
fn the_active_browser_wins_when_both_browsers_have_the_target() {
    let mut workspace = Workspace::with_home(location("home"));
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 };
    let session = SessionSnapshot::Saved(WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("target") }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("target") }], active_tab: Some(0) }, active_browser: BrowserSide::Right, layout });
    restore(&mut workspace, session);
    let transition = workspace.handle(open("target", None).into());
    assert_eq!(workspace.active_browser(), BrowserSide::Right);
    assert!(transition.outputs.iter().any(|output| matches!(output, Output::ActivateWindow { focus_browser: Some(BrowserSide::Right) })));
}

#[test]
fn only_the_last_early_target_applies_and_raise_changes_no_tabs() {
    let mut workspace = Workspace::with_home(location("home"));
    workspace.handle(open("first-target", None).into());
    workspace.handle(open("last-target", None).into());
    let raised = workspace.handle(Command::RaiseWindow.into());
    assert_eq!(raised.outputs, vec![Output::ActivateWindow { focus_browser: None }]);
    let restored = restore(&mut workspace, saved(&["first"], 0));
    assert!(read_for(&restored.work, &location("first-target")).is_none());
    assert!(read_for(&restored.work, &location("last-target")).is_some());
}

#[test]
fn the_active_tab_wins_duplicate_matches_and_an_in_flight_match_is_reused() {
    let mut workspace = Workspace::with_home(location("home"));
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 };
    let session = SessionSnapshot::Saved(WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("left") }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("target") }, TabSnapshot { location: location("target") }], active_tab: Some(1) }, active_browser: BrowserSide::Right, layout });
    restore(&mut workspace, session);
    let active = workspace.browser_chrome(BrowserSide::Right).active_tab;
    workspace.handle(open("target", None).into());
    assert_eq!(workspace.browser_chrome(BrowserSide::Right).active_tab, active);

    let navigate = workspace.handle(Command::Navigate { browser: BrowserSide::Right, location: location("pending") }.into());
    assert!(read_for(&navigate.work, &location("pending")).is_some());
    let before = workspace.browser_chrome(BrowserSide::Right).tabs.len();
    workspace.handle(open("pending", None).into());
    assert_eq!(workspace.browser_chrome(BrowserSide::Right).tabs.len(), before, "the loading tab is reused");
}

#[test]
fn a_launch_back_to_the_displayed_folder_cancels_navigation_away() {
    let mut workspace = Workspace::with_home(location("home"));
    let restored = restore(&mut workspace, saved(&["target"], 0));
    let (browser, tab, token) = read_for(&restored.work, &location("target")).expect("restored read");
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(vec![entry("wanted")]), changes: None }.into());

    let away = workspace.handle(Command::Navigate { browser, location: location("elsewhere") }.into());
    assert!(read_for(&away.work, &location("elsewhere")).is_some());
    let launched = workspace.handle(open("target", Some("wanted")).into());
    assert_eq!(workspace.browser_chrome(browser).tabs.len(), 1);
    let (read_browser, read_tab, _) = read_for(&launched.work, &location("target")).expect("launch read");
    assert_eq!((read_browser, read_tab), (browser, tab));
    assert_eq!(launched.work.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).count(), 1);
    assert!(launched.work.iter().any(|work| matches!(work, WorkRequest::Cancel { .. })));
}

#[test]
fn the_active_tab_navigates_at_the_tab_limit() {
    let mut workspace = Workspace::with_home(location("home"));
    let layout = WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 };
    let session = SessionSnapshot::Saved(WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location("left") }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: (0..8).map(|index| TabSnapshot { location: location(&format!("tab-{index}")) }).collect(), active_tab: Some(3) }, active_browser: BrowserSide::Right, layout });
    restore(&mut workspace, session);
    let active = workspace.browser_chrome(BrowserSide::Right).active_tab;
    let transition = workspace.handle(open("target", None).into());
    let chrome = workspace.browser_chrome(BrowserSide::Right);
    assert_eq!(chrome.tabs.len(), 8);
    assert_eq!(chrome.active_tab, active);
    assert!(read_for(&transition.work, &location("target")).is_some());
}
