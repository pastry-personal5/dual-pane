mod support;

use dual_pane_application::{Command, Event, Input, Output, Transition, WorkRequest, Workspace};
use dual_pane_domain::{Entry, ListingErrorKind, Location, RequestToken};
use proptest::prelude::*;
use support::*;

#[test]
fn a_newer_navigation_cancels_the_pending_read() {
    let mut workspace = Workspace::new();
    let first = navigate(&mut workspace, location(&["a"]));
    let transition = workspace.handle(Command::Navigate(location(&["b"])).into());
    let second = read_token(&transition);
    assert_ne!(first, second);
    assert_eq!(transition.work, [WorkRequest::Cancel { token: first }, WorkRequest::ReadDirectory { token: second, location: location(&["b"]) }]);
}

#[test]
fn a_superseded_result_changes_nothing() {
    let mut workspace = Workspace::new();
    let first = navigate(&mut workspace, location(&["a"]));
    navigate(&mut workspace, location(&["b"]));
    assert_eq!(workspace.handle(loaded(first, location(&["a"]), vec![file("x")])), Transition::default());
    assert_eq!(workspace.handle(failed(first, location(&["a"]), ListingErrorKind::Unknown)), Transition::default());
    assert_eq!(workspace.location(), None);
    assert_eq!(workspace.loading_location(), Some(&location(&["b"])));
}

#[test]
fn a_result_that_arrives_twice_applies_once() {
    let mut workspace = Workspace::new();
    let token = navigate(&mut workspace, location(&["a"]));
    workspace.handle(loaded(token, location(&["a"]), vec![file("x")]));
    assert_eq!(workspace.handle(loaded(token, location(&["a"]), vec![file("y")])), Transition::default());
    assert_eq!(workspace.entries(), [file("x")]);
}

#[test]
fn an_unknown_token_changes_nothing() {
    let mut workspace = showing(location(&["a"]), vec![file("x")]);
    let unknown = RequestToken::first().next().next().next();
    assert_eq!(workspace.handle(loaded(unknown, location(&["b"]), vec![])), Transition::default());
    assert_eq!(workspace.handle(failed(unknown, location(&["b"]), ListingErrorKind::ItemMissing)), Transition::default());
    assert_eq!(workspace.entries(), [file("x")]);
}

#[test]
fn a_result_for_a_different_location_changes_nothing() {
    let mut workspace = Workspace::new();
    let token = navigate(&mut workspace, location(&["a"]));
    assert_eq!(workspace.handle(loaded(token, location(&["b"]), vec![])), Transition::default());
    assert_eq!(workspace.loading_location(), Some(&location(&["a"])));
}

/// One step of a generated session. Result steps refer to an earlier
/// navigation by index, so they can be current, superseded, or repeated.
#[derive(Debug, Clone)]
enum Step {
    Navigate(usize),
    Load { navigation: usize, entries: usize },
    Fail { navigation: usize },
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![(0..4usize).prop_map(Step::Navigate), (0..8usize, 0..3usize).prop_map(|(navigation, entries)| Step::Load { navigation, entries }), (0..8usize).prop_map(|navigation| Step::Fail { navigation }),]
}

fn place(index: usize) -> Location {
    location(&[["a", "b", "c", "d"][index]])
}

fn listing(size: usize) -> Vec<Entry> {
    (0..size).map(|i| file(&format!("f{i}"))).collect()
}

proptest! {
    #[test]
    fn only_the_latest_navigation_commits_a_listing(steps in proptest::collection::vec(step(), 0..40)) {
        let mut workspace = Workspace::new();
        let mut issued: Vec<(RequestToken, Location)> = Vec::new();
        let mut expected: Option<(Location, Vec<Entry>)> = None;
        let mut pending: Option<usize> = None;

        for step in steps {
            match step {
                Step::Navigate(target) => {
                    let token = navigate(&mut workspace, place(target));
                    prop_assert!(issued.iter().all(|(earlier, _)| *earlier != token), "tokens are never reused");
                    issued.push((token, place(target)));
                    pending = Some(issued.len() - 1);
                }
                Step::Load { navigation, entries } => {
                    let Some((token, location)) = issued.get(navigation).cloned() else { continue };
                    let transition = workspace.handle(Input::Event(Event::ListingLoaded { token, location: location.clone(), entries: listing(entries).into() }));
                    if pending == Some(navigation) {
                        prop_assert_eq!(transition.outputs.len(), 1);
                        expected = Some((location, listing(entries)));
                        pending = None;
                    } else {
                        prop_assert_eq!(transition, Transition::default());
                    }
                }
                Step::Fail { navigation } => {
                    let Some((token, location)) = issued.get(navigation).cloned() else { continue };
                    let transition = workspace.handle(failed(token, location, ListingErrorKind::PermissionDenied));
                    if pending == Some(navigation) {
                        let reported_failure = matches!(transition.outputs.as_slice(), [Output::ListingFailed { .. }]);
                        prop_assert!(reported_failure);
                        pending = None;
                    } else {
                        prop_assert_eq!(transition, Transition::default());
                    }
                }
            }

            prop_assert_eq!(workspace.location(), expected.as_ref().map(|(location, _)| location));
            prop_assert_eq!(workspace.entries(), expected.as_ref().map_or(&[][..], |(_, entries)| entries.as_slice()));
            prop_assert_eq!(workspace.loading_location(), pending.map(|index| &issued[index].1));
        }
    }
}
