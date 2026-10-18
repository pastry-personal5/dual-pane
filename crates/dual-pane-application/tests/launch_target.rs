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
fn read_for(work: &[WorkRequest], folder: &Location) -> Option<(TabId, RequestToken)> {
    work.iter().find_map(|work| match work {
        WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, token, location, .. } if location == folder => Some((*tab, *token)),
        _ => None,
    })
}
fn left_locations(workspace: &Workspace) -> Vec<Option<Location>> {
    workspace.browser_chrome(BrowserSide::Left).tabs.into_iter().map(|tab| tab.location).collect()
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
fn a_missing_tab_opens_beside_the_others_in_the_left_browser_and_reads_the_folder() {
    let mut workspace = Workspace::with_home(location("home"));
    restore(&mut workspace, saved(&["first"], 0));
    let transition = workspace.handle(open("target", None).into());
    assert_eq!(left_locations(&workspace), vec![Some(location("first")), Some(location("target"))]);
    let chrome = workspace.browser_chrome(BrowserSide::Left);
    assert_eq!(chrome.active_tab, chrome.tabs[1].id);
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
    assert!(read_for(&transition.work, &location("target")).is_some());
}

#[test]
fn a_file_target_is_selected_once_its_folder_loads() {
    let mut workspace = Workspace::with_home(location("home"));
    restore(&mut workspace, saved(&["first"], 0));
    let transition = workspace.handle(open("target", Some("wanted")).into());
    let (tab, token) = read_for(&transition.work, &location("target")).expect("read");
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("other"), entry("wanted")]), changes: None }.into());
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
    let (tab, token) = read_for(&transition.work, &location("target")).expect("reread");
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![entry("wanted")]), changes: None }.into());
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::SelectionChanged { row: Some(0), .. })));
}

#[test]
fn a_target_that_arrives_before_the_session_restores_waits_for_it() {
    let mut workspace = Workspace::with_home(location("home"));
    let early = workspace.handle(open("target", None).into());
    assert!(early.outputs.is_empty() && early.work.is_empty(), "nothing happens while settings load");
    let restored = restore(&mut workspace, saved(&["first"], 0));
    assert!(read_for(&restored.work, &location("target")).is_some());
    assert_eq!(left_locations(&workspace), vec![Some(location("first")), Some(location("target"))]);
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
}

#[test]
fn a_target_also_applies_after_a_fresh_profile_and_after_a_settings_timeout() {
    let mut fresh = Workspace::with_home(location("home"));
    fresh.handle(open("target", None).into());
    restore(&mut fresh, SessionSnapshot::Absent);
    assert_eq!(left_locations(&fresh).last(), Some(&Some(location("target"))));

    let mut timed_out = Workspace::with_home(location("home"));
    timed_out.handle(open("target", None).into());
    timed_out.handle(Event::SettingsLoadTimedOut.into());
    assert_eq!(left_locations(&timed_out).last(), Some(&Some(location("target"))));
}
