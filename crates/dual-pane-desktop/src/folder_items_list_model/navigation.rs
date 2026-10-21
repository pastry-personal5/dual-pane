//! Folder navigation, sorting, and toolbar state.

use super::*;

pub(super) const SORT_FIELDS: [SortField; 4] = [SortField::Name, SortField::Type, SortField::Modified, SortField::Size];

/// The `sortChoice` code for `sort`.
pub(super) fn sort_choice(sort: Option<SortSpec>) -> i32 {
    sort.and_then(|sort| SORT_FIELDS.iter().position(|field| *field == sort.field()).map(|field| field as i32 * 2 + i32::from(sort.direction() == SortDirection::Descending))).unwrap_or(-1)
}

/// The sort a `sortChoice` code names.
pub(super) fn sort_from_choice(choice: i32) -> Option<SortSpec> {
    if choice < 0 {
        return None;
    }
    let field = SORT_FIELDS.get(usize::try_from(choice / 2).ok()?)?;
    Some(SortSpec::new(*field, if choice % 2 == 0 { SortDirection::Ascending } else { SortDirection::Descending }))
}

impl ffi::FolderItemsListModel {
    pub(super) fn go_to_parent(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::GoToParent);
    }
    pub(super) fn go_back(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::GoBack);
    }
    pub(super) fn go_forward(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::GoForward);
    }
    pub(super) fn refresh_folder(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::Refresh);
    }
    pub(super) fn set_sort(self: Pin<&mut Self>, choice: i32) {
        if let Some(sort) = sort_from_choice(choice) {
            self.submit_ui(UiEvent::Sort(sort));
        }
    }
    pub(super) fn set_toolbar_state(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().toolbar_state != value {
            self.as_mut().rust_mut().get_mut().toolbar_state = value;
            self.toolbar_state_changed();
        }
    }
    pub(super) fn set_sort_choice(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().sort_choice != value {
            self.as_mut().rust_mut().get_mut().sort_choice = value;
            self.sort_choice_changed();
        }
    }
}
