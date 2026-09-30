mod support;

use dual_pane_application::{Command, Event, Output, Transition};
use dual_pane_domain::{ListingErrorKind, Selection};
use support::*;

fn selected(text: &str) -> Selection {
    let mut selection = Selection::default();
    assert!(selection.select(name(text)));
    selection
}

#[test]
fn matching_row_and_name_select_and_replace_by_exact_identity() {
    let mut workspace = showing(location(&["home"]), vec![file("first"), file("second")]);
    assert_eq!(workspace.handle(Command::SelectEntry { row: 0, name: name("first") }.into()).outputs, [Output::SelectionChanged { selection: selected("first"), row: Some(0) }]);
    assert_eq!(workspace.handle(Command::SelectEntry { row: 1, name: name("second") }.into()).outputs, [Output::SelectionChanged { selection: selected("second"), row: Some(1) }]);
    assert_eq!(workspace.selection(), &selected("second"));
}

#[test]
fn same_out_of_range_and_mismatched_selection_are_no_ops() {
    let mut workspace = showing(location(&["home"]), vec![file("first"), file("second")]);
    workspace.handle(Command::SelectEntry { row: 0, name: name("first") }.into());
    for command in [Command::SelectEntry { row: 0, name: name("first") }, Command::SelectEntry { row: 0, name: name("second") }, Command::SelectEntry { row: 9, name: name("first") }] {
        assert_eq!(workspace.handle(command.into()), Transition::default());
    }
    assert_eq!(workspace.selection(), &selected("first"));
}

#[test]
fn pending_failure_and_cancellation_preserve_selection() {
    let mut workspace = showing(location(&["home"]), vec![dir("child"), file("second")]);
    workspace.handle(Command::SelectEntry { row: 1, name: name("second") }.into());

    let failed_token = navigate(&mut workspace, location(&["home", "child"]));
    assert_eq!(workspace.handle(failed(failed_token, ListingErrorKind::PermissionDenied)).outputs.len(), 1);
    assert_eq!(workspace.selection(), &selected("second"));

    let cancelled_token = navigate(&mut workspace, location(&["home", "child"]));
    assert_eq!(workspace.handle(Event::ListingCancelled { token: cancelled_token }.into()).outputs, [Output::ListingCancelled]);
    assert_eq!(workspace.selection(), &selected("second"));
}

#[test]
fn selection_can_change_while_loading_and_success_clears_it_explicitly() {
    let mut workspace = showing(location(&["home"]), vec![dir("child"), file("second")]);
    let token = navigate(&mut workspace, location(&["home", "child"]));
    assert_eq!(workspace.handle(Command::SelectEntry { row: 1, name: name("second") }.into()).outputs, [Output::SelectionChanged { selection: selected("second"), row: Some(1) }]);

    let transition = workspace.handle(loaded(token, vec![file("inside")]));
    assert_eq!(transition.outputs, [Output::ListingReplaced { location: location(&["home", "child"]), entries: entries(vec![file("inside")]) }, Output::SelectionChanged { selection: Selection::default(), row: None }]);
    assert_eq!(workspace.selection(), &Selection::default());
}

#[test]
fn stale_results_never_clear_selection() {
    let mut workspace = showing(location(&["home"]), vec![file("first")]);
    workspace.handle(Command::SelectEntry { row: 0, name: name("first") }.into());
    let stale = navigate(&mut workspace, location(&["one"]));
    navigate(&mut workspace, location(&["two"]));
    assert_eq!(workspace.handle(loaded(stale, vec![])), Transition::default());
    assert_eq!(workspace.selection(), &selected("first"));
}
