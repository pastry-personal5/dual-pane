use std::sync::Arc;

use dual_pane_application::{Command, Event, Output, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, EntryName, Location, TabId};

fn name(bytes: &[u8]) -> EntryName {
    EntryName::new(bytes.to_vec()).unwrap()
}

fn show(workspace: &mut Workspace, browser: BrowserSide, location: Location) -> (TabId, u64) {
    let started = workspace.handle(Command::Navigate { browser, location }.into());
    let (tab, token) = started
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { tab, token, .. } => Some((*tab, *token)),
            _ => None,
        })
        .unwrap();
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from([]), changes: None }.into());
    let revision = loaded
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::FolderItemsLoaded { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    (tab, revision)
}

fn observed(browser: BrowserSide, tab: TabId, source: &Location, visible_revision: u64, target: Location) -> Command {
    Command::NavigateObservedPath { browser, tab, source: source.clone(), visible_revision, target }
}

#[test]
fn exact_breadcrumb_navigation_uses_the_observed_tab_and_normal_read() {
    let mut workspace = Workspace::new();
    let first = Location::root().join(&name(b"same"));
    let confirmed = first.join(&name(b"caf\xFF"));
    let (tab, revision) = show(&mut workspace, BrowserSide::Left, confirmed.clone());
    let read = workspace.handle(observed(BrowserSide::Left, tab, &confirmed, revision, first.clone()).into());
    assert!(read.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: id, location, .. } if *id == tab && location == &first)));
    assert_eq!(workspace.location(BrowserSide::Left), Some(&confirmed));
    assert!(workspace.handle(observed(BrowserSide::Left, tab, &confirmed, revision, confirmed.clone()).into()).work.is_empty());
}

#[test]
fn stale_path_gestures_cannot_retarget_a_tab_or_prior_listing() {
    let mut workspace = Workspace::new();
    let original = Location::root().join(&name(b"original"));
    let (tab, revision) = show(&mut workspace, BrowserSide::Left, original.clone());
    let new_tab = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    assert!(new_tab.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { .. })));
    assert!(workspace.handle(observed(BrowserSide::Left, tab, &original, revision, Location::root()).into()).work.is_empty());
    workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab }.into());
    let replacement = Location::root().join(&name(b"replacement"));
    show(&mut workspace, BrowserSide::Left, replacement);
    assert!(workspace.handle(observed(BrowserSide::Left, tab, &original, revision, Location::root()).into()).work.is_empty());
}

#[test]
fn late_visual_navigation_result_cannot_replace_the_newer_confirmed_path() {
    let mut workspace = Workspace::new();
    let source = Location::root().join(&name(b"source"));
    let first = Location::root().join(&name(b"first"));
    let second = Location::root().join(&name(b"second"));
    let (tab, revision) = show(&mut workspace, BrowserSide::Left, source.clone());
    let start = |workspace: &mut Workspace, target: Location| {
        workspace
            .handle(observed(BrowserSide::Left, tab, &source, revision, target).into())
            .work
            .into_iter()
            .find_map(|work| match work {
                WorkRequest::ReadDirectory { token, .. } => Some(token),
                _ => None,
            })
            .unwrap()
    };
    let old_token = start(&mut workspace, first);
    let new_token = start(&mut workspace, second.clone());
    workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token: new_token, entries: Arc::from([]), changes: None }.into());
    workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token: old_token, entries: Arc::from([]), changes: None }.into());
    assert_eq!(workspace.location(BrowserSide::Left), Some(&second));
}
