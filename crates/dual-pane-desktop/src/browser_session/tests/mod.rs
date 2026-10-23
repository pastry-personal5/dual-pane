use std::sync::Arc;

use dual_pane_adapters::{FavoritesEvent, SelectionMovement};
use dual_pane_application::{Command, Event, WorkRequest, project_filter};
use dual_pane_domain::OperationCommand;
use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, RequestToken, Selection, SortSpec, TabId};

use super::*;

#[derive(Default)]
struct FakeRunner {
    dispatched: Vec<WorkRequest>,
    delivered: VecDeque<Event>,
}

impl WorkRunner for FakeRunner {
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
        self.dispatched.push(request);
        None
    }

    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
        let count = max.min(self.delivered.len());
        (self.delivered.drain(..count).collect(), !self.delivered.is_empty())
    }
}

fn entry(name: &str) -> Entry {
    Entry::new(EntryName::new(name).unwrap(), EntryKind::Directory)
}

fn path(name: &str) -> Location {
    Location::root().join(&EntryName::new(name).unwrap())
}

fn assert_view(session: &WorkspaceSession<FakeRunner>, browser: BrowserSide, expected: (&str, &[&str], Option<&str>, &str, bool, u64)) {
    let (path, rows, selected, status, loading, revision) = expected;
    let view = session.view(browser);
    assert_eq!(view.location_text(), path);
    assert_eq!(view.row_count(), rows.len());
    for (index, name) in rows.iter().enumerate() {
        assert_eq!(view.row(index).map(|row| row.name), Some((*name).to_owned()));
    }
    let expected_row = selected.and_then(|name| rows.iter().position(|row| *row == name));
    assert_eq!(view.cursor_row(), expected_row);
    let mut selection = Selection::default();
    if let Some(name) = selected {
        selection.select(EntryName::new(name).unwrap());
    }
    assert_eq!(view.selection(), &selection);
    assert_eq!(view.status_text(), status);
    assert_eq!(view.is_loading(), loading);
    assert_eq!(view.folder_items_revision(), revision);
}

/// Completes the latest read `browser` requested with `names` as folders.
fn deliver(session: &mut WorkspaceSession<FakeRunner>, browser: BrowserSide, names: &[&str]) {
    let Some(WorkRequest::ReadDirectory { tab, token, .. }) = session.runner.dispatched.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { browser: requested, .. } if *requested == browser)).cloned() else { panic!("a read") };
    session.submit(Event::FolderItemsLoaded { browser, tab, token, entries: names.iter().map(|name| entry(name)).collect(), changes: None });
}

mod event_routing;
mod navigation;
mod path_entry;
mod stored_settings;
mod tab_state;
