#![allow(dead_code)]

use std::sync::Arc;

use dual_pane_application::{Command, Event, Input, Transition, WorkRequest, Workspace};
use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, Location, RequestToken};

pub fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}

pub fn location(components: &[&str]) -> Location {
    Location::from_components(components.iter().map(|c| name(c)))
}

pub fn dir(text: &str) -> Entry {
    Entry::new(name(text), EntryKind::Directory)
}

pub fn file(text: &str) -> Entry {
    Entry::new(name(text), EntryKind::File)
}

pub fn link(text: &str, points_to_directory: bool) -> Entry {
    Entry::new(name(text), EntryKind::Symlink { points_to_directory })
}

pub fn entries(list: Vec<Entry>) -> Arc<[Entry]> {
    list.into()
}

/// The token of the single directory read a transition requests.
pub fn read_token(transition: &Transition) -> RequestToken {
    let reads: Vec<_> = transition
        .work
        .iter()
        .filter_map(|work| match work {
            WorkRequest::ReadDirectory { token, .. } => Some(*token),
            WorkRequest::Cancel { .. } => None,
        })
        .collect();
    assert_eq!(reads.len(), 1, "expected one directory read in {transition:?}");
    reads[0]
}

pub fn navigate(workspace: &mut Workspace, to: Location) -> RequestToken {
    read_token(&workspace.handle(Command::Navigate(to).into()))
}

pub fn loaded(token: RequestToken, list: Vec<Entry>) -> Input {
    Event::ListingLoaded { token, entries: entries(list) }.into()
}

pub fn failed(token: RequestToken, kind: ListingErrorKind) -> Input {
    Event::ListingFailed { token, kind }.into()
}

/// A workspace that shows `at` with `list`.
pub fn showing(at: Location, list: Vec<Entry>) -> Workspace {
    let mut workspace = Workspace::new();
    let token = navigate(&mut workspace, at.clone());
    workspace.handle(loaded(token, list));
    workspace
}
