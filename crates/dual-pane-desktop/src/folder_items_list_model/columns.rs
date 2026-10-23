//! Qt-facing cache of the visible ancestor columns.

use super::*;
use crate::browser_session::ColumnReadRequest;
use dual_pane_domain::{EntryName, ListingErrorKind, Location};

pub(super) struct ColumnData {
    parent: Location,
    selected: EntryName,
    names: Vec<EntryName>,
    next: Option<EntryName>,
    pending: Option<u64>,
    error: Option<ListingErrorKind>,
}

impl ColumnData {
    fn new(parent: Location, selected: EntryName) -> Self {
        Self { parent, selected, names: Vec::new(), next: None, pending: None, error: None }
    }
}

impl ffi::FolderItemsListModel {
    pub(super) fn set_column_revision(mut self: Pin<&mut Self>, revision: i32) {
        if self.rust().column_revision != revision {
            self.as_mut().rust_mut().get_mut().column_revision = revision;
            self.column_revision_changed();
        }
    }

    fn notify_columns(mut self: Pin<&mut Self>) {
        let next = self.rust().column_revision.wrapping_add(1);
        self.as_mut().set_column_revision(next);
    }

    pub(super) fn set_visible_columns(mut self: Pin<&mut Self>, first: i32, count: i32, generation: u64) {
        let Some(source) = self.rust().shown.location().cloned() else { return };
        let Some(first) = usize::try_from(first).ok() else { return };
        let Some(count) = usize::try_from(count).ok() else { return };
        let tab = self.active_tab_id();
        let visible_revision = self.visible_revision();
        if self.rust().column_generation != Some(generation) || self.rust().column_source.as_ref() != Some(&source) || self.rust().column_tab != tab || self.rust().column_visible_revision != visible_revision {
            self.as_mut().close_columns();
            let state = self.as_mut().rust_mut();
            let state = state.get_mut();
            state.column_generation = Some(generation);
            state.column_source = Some(source.clone());
            state.column_tab = tab;
            state.column_visible_revision = visible_revision;
            state.columns.resize_with(source.components().len(), || None);
        }
        let end = first.saturating_add(count).min(source.components().len());
        for column in first..end {
            if self.rust().columns[column].is_some() {
                continue;
            }
            let parent = source.breadcrumb_targets()[column].clone();
            let selected = source.components()[column].clone();
            let id = with_session(|session| session.request_folder_choices(ColumnReadRequest { browser: self.rust().browser, tab: TabId::new(tab), source: source.clone(), visible_revision, column, after: None, generation })).flatten();
            let mut data = ColumnData::new(parent, selected);
            data.pending = id;
            if id.is_none() {
                data.error = Some(ListingErrorKind::Internal);
            }
            self.as_mut().rust_mut().get_mut().columns[column] = Some(data);
        }
        self.as_mut().refresh_column_results();
        self.as_mut().notify_columns();
    }

    pub(super) fn close_columns(mut self: Pin<&mut Self>) {
        if let Some(generation) = self.rust().column_generation {
            let browser = self.rust().browser;
            with_session(|session| session.cancel_folder_choices(browser, generation));
        }
        let state = self.as_mut().rust_mut();
        let state = state.get_mut();
        state.column_generation = None;
        state.column_source = None;
        state.columns.clear();
        self.as_mut().notify_columns();
    }

    pub(super) fn refresh_column_results(mut self: Pin<&mut Self>) {
        let valid = self.rust().column_generation.is_some() && self.rust().column_source.as_ref() == self.rust().shown.location() && self.rust().shown.active_tab().map(TabId::value) == Some(self.rust().column_tab) && self.rust().shown.visible_revision() == self.rust().column_visible_revision;
        if !valid {
            if self.rust().column_generation.is_some() {
                self.as_mut().close_columns();
            }
            return;
        }
        let mut changed = false;
        for column in 0..self.rust().columns.len() {
            let Some(id) = self.rust().columns[column].as_ref().and_then(|data| data.pending) else { continue };
            let Some(result) = with_session(|session| session.take_folder_choice_result(id)).flatten() else { continue };
            let Some(data) = self.as_mut().rust_mut().get_mut().columns[column].as_mut() else { continue };
            data.pending = None;
            match result.outcome {
                Ok(page) => {
                    data.error = None;
                    data.next = page.next;
                    let first_page = data.names.is_empty();
                    for name in page.names.iter() {
                        if !data.names.contains(name) {
                            data.names.push(name.clone());
                        }
                    }
                    if first_page && page.selected_found && !data.names.contains(&data.selected) {
                        data.names.insert(0, data.selected.clone());
                    }
                }
                Err(error) => data.error = Some(error),
            }
            changed = true;
        }
        if changed {
            self.as_mut().notify_columns();
        }
    }

    fn column(&self, index: i32) -> Option<&ColumnData> {
        usize::try_from(index).ok().and_then(|index| self.rust().columns.get(index)).and_then(Option::as_ref)
    }
    pub(super) fn column_state(&self, column: i32) -> i32 {
        self.column(column).map_or(-1, |data| {
            if data.error.is_some() {
                2
            } else if data.pending.is_some() && data.names.is_empty() {
                0
            } else {
                1
            }
        })
    }
    pub(super) fn column_choice_count(&self, column: i32) -> i32 {
        self.column(column).map_or(0, |data| qt_int(data.names.len()))
    }
    pub(super) fn column_choice_label(&self, column: i32, row: i32) -> QString {
        self.column(column).and_then(|data| usize::try_from(row).ok().and_then(|row| data.names.get(row))).map_or_else(QString::default, |name| QString::from(name.to_text_lossy().as_ref()))
    }
    pub(super) fn column_choice_path(&self, column: i32, row: i32) -> QString {
        self.column(column).and_then(|data| usize::try_from(row).ok().and_then(|row| data.names.get(row).map(|name| data.parent.join(name)))).map_or_else(QString::default, |location| QString::from(dual_pane_adapters::location_text(&location).as_str()))
    }
    pub(super) fn column_choice_selected(&self, column: i32, row: i32) -> bool {
        self.column(column).and_then(|data| usize::try_from(row).ok().and_then(|row| data.names.get(row).map(|name| name == &data.selected))).unwrap_or(false)
    }
    pub(super) fn column_has_more(&self, column: i32) -> bool {
        self.column(column).is_some_and(|data| data.next.is_some() && data.pending.is_none())
    }
    pub(super) fn column_load_more(mut self: Pin<&mut Self>, column: i32) {
        let Some(column) = usize::try_from(column).ok() else { return };
        let Some(generation) = self.rust().column_generation else { return };
        let Some(source) = self.rust().column_source.as_ref() else { return };
        let Some(after) = self.rust().columns.get(column).and_then(Option::as_ref).and_then(|data| data.next.clone()) else { return };
        let id = with_session(|session| session.request_folder_choices(ColumnReadRequest { browser: self.rust().browser, tab: TabId::new(self.rust().column_tab), source: source.clone(), visible_revision: self.rust().column_visible_revision, column, after: Some(after), generation })).flatten();
        if let Some(data) = self.as_mut().rust_mut().get_mut().columns[column].as_mut() {
            data.pending = id;
            data.next = None;
            data.error = id.is_none().then_some(ListingErrorKind::Internal);
        }
        self.as_mut().refresh_column_results();
        self.as_mut().notify_columns();
    }
    pub(super) fn navigate_column_choice(mut self: Pin<&mut Self>, column: i32, row: i32, observed_tab: u64, visible_revision: u64, generation: u64) {
        if self.rust().column_generation != Some(generation) || self.rust().column_tab != observed_tab || self.rust().column_visible_revision != visible_revision {
            return;
        }
        let Some(source) = self.rust().column_source.clone() else { return };
        let Some(target) = self.column(column).and_then(|data| usize::try_from(row).ok().and_then(|row| data.names.get(row).map(|name| data.parent.join(name)))) else { return };
        let browser = self.rust().browser;
        with_session(|session| {
            session.submit(dual_pane_application::Command::ActivateBrowser { browser });
            session.submit(dual_pane_application::Command::NavigateObservedPath { browser, tab: TabId::new(observed_tab), source, visible_revision, target });
        });
        self.as_mut().session_changed();
    }
}
