//! The Path Edit Control's submission state and gestures.

use super::*;

impl ffi::FolderItemsListModel {
    pub(super) fn set_path_view_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().path_view_revision != value {
            self.as_mut().rust_mut().get_mut().path_view_revision = value;
            self.path_view_revision_changed();
        }
    }
    pub(super) fn breadcrumb_count(&self) -> i32 {
        self.rust().shown.location().map_or(0, |location| i32::try_from(location.components().len() + 1).unwrap_or(i32::MAX))
    }
    pub(super) fn breadcrumb_label(&self, index: i32) -> QString {
        let Some(location) = self.rust().shown.location() else { return QString::default() };
        if index == 0 {
            return QString::from("/");
        }
        usize::try_from(index - 1).ok().and_then(|index| location.components().get(index)).map_or_else(QString::default, |name| QString::from(name.to_text_lossy().as_ref()))
    }
    pub(super) fn breadcrumb_path(&self, index: i32) -> QString {
        let Some(location) = self.rust().shown.location() else { return QString::default() };
        let Some(index) = usize::try_from(index).ok() else { return QString::default() };
        location.breadcrumb_targets().get(index).map_or_else(QString::default, |target| QString::from(dual_pane_adapters::location_text(target).as_str()))
    }
    pub(super) fn navigate_breadcrumb(mut self: Pin<&mut Self>, index: i32, observed_tab: u64, visible_revision: u64) {
        let browser = self.rust().browser;
        let shown = &self.rust().shown;
        if shown.active_tab().map(dual_pane_domain::TabId::value) != Some(observed_tab) || shown.visible_revision() != visible_revision {
            return;
        }
        let Some(source) = shown.location().cloned() else { return };
        let Some(target) = usize::try_from(index).ok().and_then(|index| source.breadcrumb_targets().get(index).cloned()) else { return };
        if source == target {
            return;
        }
        with_session(|session| {
            session.submit(dual_pane_application::Command::ActivateBrowser { browser });
            session.submit(dual_pane_application::Command::NavigateObservedPath { browser, tab: dual_pane_domain::TabId::new(observed_tab), source, visible_revision, target });
        });
        self.as_mut().session_changed();
    }
    pub(super) fn begin_path_entry(mut self: Pin<&mut Self>) {
        let origin = self.rust().shown.location().cloned();
        self.as_mut().rust_mut().get_mut().path_origin = origin;
    }
    pub(super) fn submit_path(mut self: Pin<&mut Self>, observed_tab: u64, text: &QString, edited: bool) {
        let browser = self.rust().browser;
        let origin = self.rust().path_origin.clone();
        let text = String::from_utf16(text.as_slice()).ok();
        with_session(|session| session.submit_path_observed(browser, observed_tab, text, origin, edited));
        self.as_mut().session_changed();
    }
    pub(super) fn cancel_path_entry(mut self: Pin<&mut Self>, observed_tab: u64) {
        let browser = self.rust().browser;
        with_session(|session| session.cancel_path_entry(browser, observed_tab));
        self.as_mut().session_changed();
    }
    pub(super) fn discard_path_entry(mut self: Pin<&mut Self>) {
        let browser = self.rust().browser;
        with_session(|session| session.discard_path_entry(browser));
        self.as_mut().session_changed();
    }
    pub(super) fn set_path_entry_state(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().path_entry_state != value {
            self.as_mut().rust_mut().get_mut().path_entry_state = value;
            self.path_entry_state_changed();
        }
    }
    pub(super) fn set_path_error(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().path_error != text {
            self.as_mut().rust_mut().get_mut().path_error = text;
            self.path_error_changed();
        }
    }
    pub(super) fn set_path_entry_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().path_entry_revision != value {
            self.as_mut().rust_mut().get_mut().path_entry_revision = value;
            self.path_entry_revision_changed();
        }
    }
    pub(super) fn set_path_accepted_count(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().path_accepted_count != value {
            self.as_mut().rust_mut().get_mut().path_accepted_count = value;
            self.path_accepted_count_changed();
        }
    }
}
