//! Row selection, cursor movement, and scroll hints.

use super::*;

impl ffi::FolderItemsListModel {
    pub(super) fn select_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::SelectRow { row });
    }
    pub(super) fn toggle_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ToggleRow { row });
    }
    pub(super) fn extend_to_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ExtendToRow { row });
    }
    pub(super) fn secondary_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::SecondaryRow { row });
    }
    pub(super) fn activate_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ActivateRow { row });
    }
    pub(super) fn row_event(self: Pin<&mut Self>, row: i32, event: impl FnOnce(usize) -> UiEvent) {
        if let Ok(row) = usize::try_from(row) {
            self.submit_ui(event(row));
        }
    }
    pub(super) fn move_cursor(self: Pin<&mut Self>, movement: i32, page: i32, extend: bool) {
        let rows = usize::try_from(page).unwrap_or(0);
        let movement = match movement {
            0 => SelectionMovement::Previous,
            1 => SelectionMovement::Next,
            2 => SelectionMovement::First,
            3 => SelectionMovement::Last,
            4 => SelectionMovement::PageUp { rows },
            5 => SelectionMovement::PageDown { rows },
            _ => return,
        };
        self.submit_ui(if extend { UiEvent::ExtendSelection(movement) } else { UiEvent::MoveSelection(movement) });
    }
    pub(super) fn select_all(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::SelectAll);
    }
    pub(super) fn activate_selected(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::ActivateSelection);
    }
    pub(super) fn clear_selection(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::ClearSelection);
    }
    /// Scrolling changes no view model and does not activate the Browser,
    /// so nothing is refreshed.
    pub(super) fn update_scroll(self: Pin<&mut Self>, row: i32, offset: i32) {
        let browser = self.rust().browser;
        let shown = self.rust().shown.clone();
        with_session(|session| session.submit_observed_view_state(browser, UiEvent::Scrolled { row: usize::try_from(row).ok(), offset }, shown));
    }
    pub(super) fn selected_ranges(&self) -> QList<i32> {
        let mut ranges = QList::default();
        let rows = self.rust().shown.selected_rows();
        let mut rows = rows.into_iter().filter_map(|row| i32::try_from(row).ok()).peekable();
        while let Some(first) = rows.next() {
            let mut last = first;
            while rows.peek() == Some(&(last + 1)) {
                last += 1;
                rows.next();
            }
            ranges.append(first);
            ranges.append(last);
        }
        ranges
    }
    pub(super) fn scroll_hint_row(&self) -> i32 {
        self.rust().shown.scroll_hint_row().and_then(|(row, _)| i32::try_from(row).ok()).unwrap_or(-1)
    }
    pub(super) fn scroll_hint_offset(&self) -> i32 {
        self.rust().shown.scroll_hint_row().map_or(0, |(_, offset)| offset)
    }
    pub(super) fn set_cursor_row(mut self: Pin<&mut Self>, row: i32) {
        if self.rust().cursor_row != row {
            self.as_mut().rust_mut().get_mut().cursor_row = row;
            self.cursor_row_changed();
        }
    }
    pub(super) fn set_selection_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().selection_revision != value {
            self.as_mut().rust_mut().get_mut().selection_revision = value;
            self.selection_revision_changed();
        }
    }
}
