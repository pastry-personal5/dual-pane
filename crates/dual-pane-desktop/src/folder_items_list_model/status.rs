//! The status line, path, and summary texts.

use super::*;

impl ffi::FolderItemsListModel {
    pub(super) fn set_missing_folder(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().missing_folder != value {
            self.as_mut().rust_mut().get_mut().missing_folder = value;
            self.missing_folder_changed();
        }
    }
    pub(super) fn set_status_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().status_text != text {
            self.as_mut().rust_mut().get_mut().status_text = text;
            self.status_text_changed();
        }
    }
    pub(super) fn set_path_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().path_text != text {
            self.as_mut().rust_mut().get_mut().path_text = text;
            self.path_text_changed();
        }
    }
    pub(super) fn set_folder_name(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().folder_name != text {
            self.as_mut().rust_mut().get_mut().folder_name = text;
            self.folder_name_changed();
        }
    }
    pub(super) fn set_summary_count_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().summary_count_text != text {
            self.as_mut().rust_mut().get_mut().summary_count_text = text;
            self.summary_count_text_changed();
        }
    }
    pub(super) fn set_summary_selected_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().summary_selected_text != text {
            self.as_mut().rust_mut().get_mut().summary_selected_text = text;
            self.summary_selected_text_changed();
        }
    }
    pub(super) fn set_summary_size_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().summary_size_text != text {
            self.as_mut().rust_mut().get_mut().summary_size_text = text;
            self.summary_size_text_changed();
        }
    }
}
