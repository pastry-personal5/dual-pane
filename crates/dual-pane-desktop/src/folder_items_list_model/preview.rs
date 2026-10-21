//! The Preview Pane properties.

use super::*;

impl ffi::FolderItemsListModel {
    pub(super) fn set_preview_target(mut self: Pin<&mut Self>, width: i32, height: i32) {
        let browser = self.rust().browser;
        with_session(|session| {
            if let Some(tab) = session.view(browser).active_tab() {
                session.submit(dual_pane_application::Command::SetPreviewTarget { browser, tab, width: width.max(0) as u32, height: height.max(0) as u32 });
            }
        });
        self.as_mut().session_changed();
    }
    pub(super) fn preview_kind(&self) -> i32 {
        match self.rust().shown.preview() {
            PreviewView::Collapsed => 0,
            PreviewView::NoSelection => 1,
            PreviewView::MultipleSelection => 2,
            PreviewView::UnsupportedSelection { .. } => 3,
            PreviewView::Loading { .. } => 4,
            PreviewView::Unavailable { .. } => 5,
            PreviewView::DecodeFailed { .. } => 6,
            PreviewView::Image { .. } => 7,
        }
    }
    pub(super) fn preview_details(&self) -> QString {
        QString::from(self.rust().shown.preview_details_text())
    }
    pub(super) fn preview_image_width(&self) -> i32 {
        match self.rust().shown.preview() {
            PreviewView::Image { width, .. } => i32::try_from(*width).unwrap_or(0),
            _ => 0,
        }
    }
    pub(super) fn preview_image_height(&self) -> i32 {
        match self.rust().shown.preview() {
            PreviewView::Image { height, .. } => i32::try_from(*height).unwrap_or(0),
            _ => 0,
        }
    }
    pub(super) fn preview_image_pixels(&self) -> QByteArray {
        match self.rust().shown.preview() {
            PreviewView::Image { pixels, .. } => QByteArray::from(pixels.as_ref()),
            _ => QByteArray::default(),
        }
    }
    pub(super) fn set_preview_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().preview_revision != value {
            self.as_mut().rust_mut().get_mut().preview_revision = value;
            self.preview_revision_changed();
        }
    }
}
