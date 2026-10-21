//! The Folder Items filter row.

use super::*;

impl ffi::FolderItemsListModel {
    pub(super) fn edit_filter_text(mut self: Pin<&mut Self>, observed_tab: u64, text: &QString) {
        let browser = self.rust().browser;
        with_session(|session| session.set_filter_text_observed(browser, observed_tab, text.to_string()));
        self.as_mut().session_changed();
    }
    pub(super) fn set_filter_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().filter_text != text {
            self.as_mut().rust_mut().get_mut().filter_text = text;
            self.filter_text_changed();
        }
    }
    pub(super) fn set_filter_status(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().filter_status != value {
            self.as_mut().rust_mut().get_mut().filter_status = value;
            self.filter_status_changed();
        }
    }
    pub(super) fn set_filter_result_count(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().filter_result_count != value {
            self.as_mut().rust_mut().get_mut().filter_result_count = value;
            self.filter_result_count_changed();
        }
    }
    pub(super) fn set_no_filter_matches(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().no_filter_matches != value {
            self.as_mut().rust_mut().get_mut().no_filter_matches = value;
            self.no_filter_matches_changed();
        }
    }
}
