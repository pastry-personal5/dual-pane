mod support;

use dual_pane_application::{Command, Output, Transition, WorkRequest, Workspace};
use dual_pane_domain::{ListingError, ListingErrorKind};
use support::*;

#[test]
fn navigating_requests_a_directory_read_and_reports_loading() {
    let mut workspace = Workspace::new();
    let transition = workspace.handle(Command::Navigate(location(&["home"])).into());
    let token = read_token(&transition);
    assert_eq!(transition.outputs, [Output::LoadingStarted { location: location(&["home"]) }]);
    assert_eq!(transition.work, [WorkRequest::ReadDirectory { token, location: location(&["home"]) }]);
    assert_eq!(workspace.loading_location(), Some(&location(&["home"])));
    assert_eq!(workspace.location(), None);
}

#[test]
fn a_matching_result_replaces_the_listing() {
    let mut workspace = Workspace::new();
    let token = navigate(&mut workspace, location(&["home"]));
    let transition = workspace.handle(loaded(token, vec![dir("docs"), file("a.txt")]));
    assert_eq!(transition.outputs, [Output::ListingReplaced { location: location(&["home"]), entries: entries(vec![dir("docs"), file("a.txt")]) }]);
    assert!(transition.work.is_empty());
    assert_eq!(workspace.location(), Some(&location(&["home"])));
    assert_eq!(workspace.entries(), [dir("docs"), file("a.txt")]);
    assert_eq!(workspace.loading_location(), None);
}

#[test]
fn results_are_kept_in_the_order_they_arrive() {
    // Sorting happens on a worker before the result arrives; `handle` never sorts.
    let unsorted = vec![file("z"), dir("b"), file("a")];
    let workspace = showing(location(&["home"]), unsorted.clone());
    assert_eq!(workspace.entries(), unsorted);
}

#[test]
fn a_failed_read_keeps_the_previous_listing_and_reports_the_error() {
    let mut workspace = showing(location(&["home"]), vec![dir("secret")]);
    let token = read_token(&workspace.handle(Command::OpenEntry { row: 0, name: name("secret") }.into()));
    let transition = workspace.handle(failed(token, ListingErrorKind::PermissionDenied));
    assert_eq!(transition.outputs, [Output::ListingFailed { error: ListingError::new(location(&["home", "secret"]), ListingErrorKind::PermissionDenied) }]);
    assert_eq!(workspace.location(), Some(&location(&["home"])));
    assert_eq!(workspace.entries(), [dir("secret")]);
    assert_eq!(workspace.loading_location(), None);
}

#[test]
fn opening_a_folder_navigates_into_it() {
    let mut workspace = showing(location(&["home"]), vec![dir("docs")]);
    let transition = workspace.handle(Command::OpenEntry { row: 0, name: name("docs") }.into());
    assert_eq!(transition.outputs, [Output::LoadingStarted { location: location(&["home", "docs"]) }]);
}

#[test]
fn opening_a_link_to_a_folder_navigates_through_the_link() {
    let mut workspace = showing(location(&["home"]), vec![link("shortcut", true)]);
    let transition = workspace.handle(Command::OpenEntry { row: 0, name: name("shortcut") }.into());
    assert_eq!(transition.outputs, [Output::LoadingStarted { location: location(&["home", "shortcut"]) }]);
}

#[test]
fn opening_anything_else_changes_nothing() {
    let mut workspace = showing(location(&["home"]), vec![file("a.txt"), link("file-link", false), dir("docs")]);
    for (row, target) in [(0, "a.txt"), (1, "file-link"), (2, "missing")] {
        assert_eq!(workspace.handle(Command::OpenEntry { row, name: name(target) }.into()), Transition::default());
    }
    assert_eq!(workspace.loading_location(), None);
}

#[test]
fn opening_before_any_listing_changes_nothing() {
    let mut workspace = Workspace::new();
    assert_eq!(workspace.handle(Command::OpenEntry { row: 0, name: name("docs") }.into()), Transition::default());
}

#[test]
fn opening_requires_the_row_and_exact_name_to_match() {
    let mut workspace = showing(location(&["home"]), vec![dir("first"), dir("second")]);
    assert_eq!(workspace.handle(Command::OpenEntry { row: 0, name: name("second") }.into()), Transition::default());
    assert_eq!(workspace.handle(Command::OpenEntry { row: 2, name: name("second") }.into()), Transition::default());
    assert_eq!(workspace.loading_location(), None);
}

#[test]
fn going_to_the_parent_navigates_up_one_level() {
    let mut workspace = showing(location(&["home", "docs"]), vec![]);
    let transition = workspace.handle(Command::GoToParent.into());
    assert_eq!(transition.outputs, [Output::LoadingStarted { location: location(&["home"]) }]);
}

#[test]
fn going_to_the_parent_of_the_root_changes_nothing() {
    let mut workspace = showing(location(&[]), vec![dir("home")]);
    assert_eq!(workspace.handle(Command::GoToParent.into()), Transition::default());
}

#[test]
fn going_to_the_parent_before_any_listing_changes_nothing() {
    let mut workspace = Workspace::new();
    assert_eq!(workspace.handle(Command::GoToParent.into()), Transition::default());
}
