//! The Path Edit Control's submission state and gestures.

use super::*;

impl ffi::FolderItemsListModel {
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
