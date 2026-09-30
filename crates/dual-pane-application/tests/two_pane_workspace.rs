use std::sync::Arc;

use dual_pane_application::{Command, Event, Output, WorkRequest, Workspace};
use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, Location, PaneSide, Selection};

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
fn navigate(workspace: &mut Workspace, pane: PaneSide, location: Location) -> dual_pane_domain::RequestToken {
    read_token(&workspace.handle(Command::Navigate { pane, location }.into()).work)
}
fn load(workspace: &mut Workspace, pane: PaneSide, token: dual_pane_domain::RequestToken, entries: Vec<Entry>) {
    workspace.handle(Event::ListingLoaded { pane, token, entries: Arc::from(entries) }.into());
}

#[test]
fn activation_starts_left_and_changes_only_when_side_changes() {
    let mut workspace = Workspace::new();
    assert_eq!(workspace.active_pane(), PaneSide::Left);
    assert_eq!(workspace.handle(Command::ActivatePane { pane: PaneSide::Left }.into()).outputs, []);
    assert_eq!(workspace.handle(Command::ActivatePane { pane: PaneSide::Right }.into()).outputs, [Output::ActivePaneChanged { pane: PaneSide::Right }]);
    assert_eq!(workspace.handle(Command::ActivatePane { pane: PaneSide::Right }.into()).outputs, []);
}

#[test]
fn listings_selections_and_navigation_are_independent() {
    let mut workspace = Workspace::new();
    let left = navigate(&mut workspace, PaneSide::Left, location(&["left"]));
    let right = navigate(&mut workspace, PaneSide::Right, location(&["right"]));
    assert_ne!(left, right);
    load(&mut workspace, PaneSide::Left, left, vec![directory("child"), file("left-file")]);
    load(&mut workspace, PaneSide::Right, right, vec![file("right-file")]);
    workspace.handle(Command::SelectEntry { pane: PaneSide::Left, row: 1, name: name("left-file") }.into());
    assert_eq!(workspace.selection(PaneSide::Left), &selected("left-file"));
    assert_eq!(workspace.selection(PaneSide::Right), &Selection::default());
    let child = workspace.handle(Command::OpenEntry { pane: PaneSide::Left, row: 0, name: name("child") }.into());
    assert_eq!(child.outputs, [Output::LoadingStarted { pane: PaneSide::Left, location: location(&["left", "child"]) }]);
    assert_eq!(workspace.location(PaneSide::Right), Some(&location(&["right"])));
    assert_eq!(workspace.active_pane(), PaneSide::Left);
}

#[test]
fn clear_selection_is_targeted_and_replacement_clears_only_target() {
    let mut workspace = Workspace::new();
    let left = navigate(&mut workspace, PaneSide::Left, location(&["left"]));
    let right = navigate(&mut workspace, PaneSide::Right, location(&["right"]));
    load(&mut workspace, PaneSide::Left, left, vec![file("a")]);
    load(&mut workspace, PaneSide::Right, right, vec![file("b")]);
    workspace.handle(Command::SelectEntry { pane: PaneSide::Left, row: 0, name: name("a") }.into());
    workspace.handle(Command::SelectEntry { pane: PaneSide::Right, row: 0, name: name("b") }.into());
    assert_eq!(workspace.handle(Command::ClearSelection { pane: PaneSide::Left }.into()).outputs, [Output::SelectionChanged { pane: PaneSide::Left, selection: Selection::default(), row: None }]);
    assert_eq!(workspace.selection(PaneSide::Right), &selected("b"));
    let replacement = navigate(&mut workspace, PaneSide::Right, location(&["next"]));
    assert_eq!(workspace.handle(Event::ListingLoaded { pane: PaneSide::Right, token: replacement, entries: Arc::from(vec![file("c")]) }.into()).outputs.len(), 2);
    assert_eq!(workspace.selection(PaneSide::Right), &Selection::default());
}

#[test]
fn cancellation_and_terminal_events_are_pane_local() {
    let mut workspace = Workspace::new();
    let old_left = navigate(&mut workspace, PaneSide::Left, location(&["old"]));
    let right = navigate(&mut workspace, PaneSide::Right, location(&["right"]));
    let replacement = workspace.handle(Command::Navigate { pane: PaneSide::Left, location: location(&["new"]) }.into());
    let new_left = read_token(&replacement.work);
    assert_eq!(replacement.work[0], WorkRequest::Cancel { pane: PaneSide::Left, token: old_left });
    assert_eq!(workspace.loading_location(PaneSide::Right), Some(&location(&["right"])));
    assert_eq!(workspace.handle(Event::ListingLoaded { pane: PaneSide::Right, token: new_left, entries: Arc::from(vec![file("wrong")]) }.into()).outputs, []);
    load(&mut workspace, PaneSide::Right, right, vec![file("right")]);
    assert_eq!(workspace.handle(Event::ListingCancelled { pane: PaneSide::Left, token: old_left }.into()).outputs, []);
    assert_eq!(workspace.handle(Event::ListingCancelled { pane: PaneSide::Left, token: new_left }.into()).outputs, [Output::ListingCancelled { pane: PaneSide::Left }]);
    assert_eq!(workspace.handle(Event::ListingCancelled { pane: PaneSide::Left, token: new_left }.into()).outputs, []);
}

#[test]
fn failure_preserves_only_the_target_panes_committed_listing() {
    let mut workspace = Workspace::new();
    let left = navigate(&mut workspace, PaneSide::Left, location(&["left"]));
    let right = navigate(&mut workspace, PaneSide::Right, location(&["right"]));
    load(&mut workspace, PaneSide::Left, left, vec![file("a")]);
    load(&mut workspace, PaneSide::Right, right, vec![file("b")]);
    let failed = navigate(&mut workspace, PaneSide::Left, location(&["missing"]));
    workspace.handle(Event::ListingFailed { pane: PaneSide::Left, token: failed, kind: ListingErrorKind::ItemMissing }.into());
    assert_eq!(workspace.location(PaneSide::Left), Some(&location(&["left"])));
    assert_eq!(workspace.location(PaneSide::Right), Some(&location(&["right"])));
}
