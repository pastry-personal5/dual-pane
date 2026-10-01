use std::sync::Arc;

use dual_pane_application::{Command, Event, Output, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingErrorKind, Location, Selection};

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}
fn location(parts: &[&str]) -> Location {
    parts.iter().fold(Location::root(), |location, part| location.join(&name(part)))
}
fn file(value: &str) -> Entry {
    Entry::new(name(value), EntryKind::File)
}
fn directory(value: &str) -> Entry {
    Entry::new(name(value), EntryKind::Directory)
}
fn selected(value: &str) -> Selection {
    let mut selection = Selection::default();
    selection.select(name(value));
    selection
}
fn read_token(work: &[WorkRequest]) -> dual_pane_domain::RequestToken {
    match work.last() {
        Some(WorkRequest::ReadDirectory { token, .. }) => *token,
        _ => panic!("expected a read request"),
    }
}
fn navigate(workspace: &mut Workspace, browser: BrowserSide, location: Location) -> dual_pane_domain::RequestToken {
    read_token(&workspace.handle(Command::Navigate { browser, location }.into()).work)
}
fn load(workspace: &mut Workspace, browser: BrowserSide, token: dual_pane_domain::RequestToken, entries: Vec<Entry>) {
    workspace.handle(Event::FolderItemsLoaded { browser, token, entries: Arc::from(entries) }.into());
}

#[test]
fn activation_starts_left_and_changes_only_when_side_changes() {
    let mut workspace = Workspace::new();
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
    assert_eq!(workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Left }.into()).outputs, []);
    assert_eq!(workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into()).outputs, [Output::ActiveBrowserChanged { browser: BrowserSide::Right }]);
    assert_eq!(workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into()).outputs, []);
}

#[test]
fn listings_selections_and_navigation_are_independent() {
    let mut workspace = Workspace::new();
    let left = navigate(&mut workspace, BrowserSide::Left, location(&["left"]));
    let right = navigate(&mut workspace, BrowserSide::Right, location(&["right"]));
    assert_ne!(left, right);
    load(&mut workspace, BrowserSide::Left, left, vec![directory("child"), file("left-file")]);
    load(&mut workspace, BrowserSide::Right, right, vec![file("right-file")]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, row: 1, name: name("left-file") }.into());
    assert_eq!(workspace.selection(BrowserSide::Left), &selected("left-file"));
    assert_eq!(workspace.selection(BrowserSide::Right), &Selection::default());
    let child = workspace.handle(Command::OpenEntry { browser: BrowserSide::Left, row: 0, name: name("child") }.into());
    assert_eq!(child.outputs, [Output::LoadingStarted { browser: BrowserSide::Left, location: location(&["left", "child"]) }]);
    assert_eq!(workspace.location(BrowserSide::Right), Some(&location(&["right"])));
    assert_eq!(workspace.active_browser(), BrowserSide::Left);
}

#[test]
fn clear_selection_is_targeted_and_replacement_clears_only_target() {
    let mut workspace = Workspace::new();
    let left = navigate(&mut workspace, BrowserSide::Left, location(&["left"]));
    let right = navigate(&mut workspace, BrowserSide::Right, location(&["right"]));
    load(&mut workspace, BrowserSide::Left, left, vec![file("a")]);
    load(&mut workspace, BrowserSide::Right, right, vec![file("b")]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, row: 0, name: name("a") }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Right, row: 0, name: name("b") }.into());
    assert_eq!(workspace.handle(Command::ClearSelection { browser: BrowserSide::Left }.into()).outputs, [Output::SelectionChanged { browser: BrowserSide::Left, selection: Selection::default(), row: None }]);
    assert_eq!(workspace.selection(BrowserSide::Right), &selected("b"));
    let replacement = navigate(&mut workspace, BrowserSide::Right, location(&["next"]));
    assert_eq!(workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Right, token: replacement, entries: Arc::from(vec![file("c")]) }.into()).outputs.len(), 2);
    assert_eq!(workspace.selection(BrowserSide::Right), &Selection::default());
}

#[test]
fn cancellation_and_terminal_events_are_browser_local() {
    let mut workspace = Workspace::new();
    let old_left = navigate(&mut workspace, BrowserSide::Left, location(&["old"]));
    let right = navigate(&mut workspace, BrowserSide::Right, location(&["right"]));
    let replacement = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: location(&["new"]) }.into());
    let new_left = read_token(&replacement.work);
    assert_eq!(replacement.work[0], WorkRequest::Cancel { browser: BrowserSide::Left, token: old_left });
    assert_eq!(workspace.loading_location(BrowserSide::Right), Some(&location(&["right"])));
    assert_eq!(workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Right, token: new_left, entries: Arc::from(vec![file("wrong")]) }.into()).outputs, []);
    load(&mut workspace, BrowserSide::Right, right, vec![file("right")]);
    assert_eq!(workspace.handle(Event::FolderItemsCancelled { browser: BrowserSide::Left, token: old_left }.into()).outputs, []);
    assert_eq!(workspace.handle(Event::FolderItemsCancelled { browser: BrowserSide::Left, token: new_left }.into()).outputs, [Output::FolderItemsCancelled { browser: BrowserSide::Left }]);
    assert_eq!(workspace.handle(Event::FolderItemsCancelled { browser: BrowserSide::Left, token: new_left }.into()).outputs, []);
}

#[test]
fn failure_preserves_only_the_target_browsers_committed_listing() {
    let mut workspace = Workspace::new();
    let left = navigate(&mut workspace, BrowserSide::Left, location(&["left"]));
    let right = navigate(&mut workspace, BrowserSide::Right, location(&["right"]));
    load(&mut workspace, BrowserSide::Left, left, vec![file("a")]);
    load(&mut workspace, BrowserSide::Right, right, vec![file("b")]);
    let failed = navigate(&mut workspace, BrowserSide::Left, location(&["missing"]));
    workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, token: failed, kind: ListingErrorKind::ItemMissing }.into());
    assert_eq!(workspace.location(BrowserSide::Left), Some(&location(&["left"])));
    assert_eq!(workspace.location(BrowserSide::Right), Some(&location(&["right"])));
}
