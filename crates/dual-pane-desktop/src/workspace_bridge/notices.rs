//! Notices window invokables and properties.

use super::*;

impl ffi::WorkspaceBridge {
    pub(super) fn notice_count(&self) -> i32 {
        count(self.rust().shown.notices().len())
    }
    pub(super) fn notice_text(&self, notice: i32) -> QString {
        self.notice(notice).map_or_else(QString::default, |notice| QString::from(notice.text.as_str()))
    }
    pub(super) fn notice_offers_reset(&self, notice: i32) -> bool {
        self.notice(notice).is_some_and(|notice| notice.offers_reset)
    }
    pub(super) fn notice_offers_journal_retry(&self, notice: i32) -> bool {
        self.notice(notice).is_some_and(|notice| notice.offers_journal_retry)
    }
    pub(super) fn retry_journal(mut self: Pin<&mut Self>) {
        with_session(|session| session.submit(dual_pane_application::Command::RetryJournal));
        self.as_mut().session_changed();
    }
    pub(super) fn notice(&self, notice: i32) -> Option<&dual_pane_adapters::NoticeViewModel> {
        usize::try_from(notice).ok().and_then(|notice| self.rust().shown.notices().get(notice))
    }
    pub(super) fn set_notices_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().notices_revision != value {
            self.as_mut().rust_mut().get_mut().notices_revision = value;
            self.notices_revision_changed();
        }
    }
    pub(super) fn set_notices_open_requests(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().notices_open_requests != value {
            self.as_mut().rust_mut().get_mut().notices_open_requests = value;
            self.notices_open_requests_changed();
        }
    }
    pub(super) fn set_notices_startup_ready(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().notices_startup_ready != value {
            self.as_mut().rust_mut().get_mut().notices_startup_ready = value;
            self.notices_startup_ready_changed();
        }
    }
}
