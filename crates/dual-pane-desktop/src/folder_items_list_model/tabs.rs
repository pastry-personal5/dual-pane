//! The tab strip.

use super::*;

impl ffi::FolderItemsListModel {
    pub(super) fn new_tab(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::NewTab);
    }
    pub(super) fn close_active_tab(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::CloseActiveTab);
    }
    pub(super) fn activate_tab(self: Pin<&mut Self>, tab: u64) {
        self.submit_ui(UiEvent::ActivateTab { tab: TabId::new(tab) });
    }
    pub(super) fn close_tab(self: Pin<&mut Self>, tab: u64) {
        self.submit_ui(UiEvent::CloseTab { tab: TabId::new(tab) });
    }
    pub(super) fn reorder_tab(self: Pin<&mut Self>, tab: u64, position: i32) {
        if let Ok(position) = usize::try_from(position) {
            self.submit_ui(UiEvent::ReorderTab { tab: TabId::new(tab), position });
        }
    }
    pub(super) fn tab(&self, index: i32) -> Option<&dual_pane_adapters::TabViewModel> {
        usize::try_from(index).ok().and_then(|index| self.rust().shown.tabs().get(index))
    }
    pub(super) fn tab_count(&self) -> i32 {
        qt_int(self.rust().shown.tabs().len())
    }
    pub(super) fn tab_id(&self, index: i32) -> u64 {
        self.tab(index).map_or(u64::MAX, |tab| tab.id.value())
    }
    pub(super) fn tab_label(&self, index: i32) -> QString {
        self.tab(index).map_or_else(QString::default, |tab| QString::from(tab.label.as_str()))
    }
    pub(super) fn tab_path(&self, index: i32) -> QString {
        self.tab(index).map_or_else(QString::default, |tab| QString::from(tab.path.as_str()))
    }
    pub(super) fn tab_is_active(&self, index: i32) -> bool {
        self.tab(index).is_some_and(|tab| tab.active)
    }
    pub(super) fn set_tabs_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().tabs_revision != value {
            self.as_mut().rust_mut().get_mut().tabs_revision = value;
            self.tabs_revision_changed();
        }
    }
}
