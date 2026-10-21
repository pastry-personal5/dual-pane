//! File commands, clipboard commands, and the inline name editor.

use super::*;

/// The Qt code for `command`.
pub(super) fn command_code(command: OperationCommand) -> i32 {
    OperationCommand::ALL.iter().position(|candidate| *candidate == command).map_or(-1, qt_int)
}

pub(super) fn job_code(id: OperationId) -> i64 {
    i64::try_from(id.get()).unwrap_or(-1)
}

impl ffi::FolderItemsListModel {
    pub(super) fn start_file_command_observed(mut self: Pin<&mut Self>, command: i32, tab: u64, visible_revision: u64) {
        if let Some(command) = operation_command(command) {
            let browser = self.rust().browser;
            with_session(|session| session.submit_ui_at_observation(browser, UiEvent::FileCommand(command), tab, visible_revision));
            self.as_mut().session_changed();
        }
    }
    pub(super) fn show_package_contents_observed(mut self: Pin<&mut Self>, row: i32, tab: u64, visible_revision: u64) {
        if let Ok(row) = usize::try_from(row) {
            let browser = self.rust().browser;
            with_session(|session| session.submit_ui_at_observation(browser, UiEvent::ShowPackageContents { row }, tab, visible_revision));
            self.as_mut().session_changed();
        }
    }
    pub(super) fn file_command_available(&self, command: i32) -> bool {
        let browser = self.rust().browser;
        operation_command(command).and_then(|command| with_session(|session| session.file_command_available(browser, command))).unwrap_or(false)
    }
    pub(super) fn start_file_command(self: Pin<&mut Self>, command: i32) {
        if let Some(command) = operation_command(command) {
            self.submit_ui(UiEvent::FileCommand(command));
        }
    }
    pub(super) fn clipboard_command_available(&self, command: i32) -> bool {
        let browser = self.rust().browser;
        let action = match command {
            0 => Some(ClipboardAction::Copy),
            1 => Some(ClipboardAction::Cut),
            2 => None,
            _ => return false,
        };
        with_session(|session| session.clipboard_command_available(browser, action)).unwrap_or(false)
    }
    pub(super) fn start_clipboard_command_observed(mut self: Pin<&mut Self>, command: i32, tab: u64, visible_revision: u64) {
        let action = match command {
            0 => Some(ClipboardAction::Copy),
            1 => Some(ClipboardAction::Cut),
            2 => None,
            _ => return,
        };
        let browser = self.rust().browser;
        let (generation, offer) = crate::clipboard_native::generation_and_offer();
        with_session(|session| session.observe_clipboard(generation, offer));
        with_session(|session| session.submit_clipboard_observed(browser, action, TabId::new(tab), visible_revision));
        self.as_mut().session_changed();
    }
    pub(super) fn observe_clipboard(mut self: Pin<&mut Self>) {
        let (generation, offer) = crate::clipboard_native::generation_and_offer();
        with_session(|session| session.observe_clipboard(generation, offer));
        self.as_mut().session_changed();
    }
    pub(super) fn expire_status(mut self: Pin<&mut Self>, token: i32) {
        let browser = self.rust().browser;
        // The token is the view model's counter truncated like every revision.
        let current = self.rust().shown.status_token();
        if revision(current) == token {
            with_session(|session| session.expire_status(browser, current));
            self.as_mut().session_changed();
        }
    }
    pub(super) fn editor_command(&self) -> i32 {
        self.rust().shown.editor().map_or(-1, |editor| command_code(editor.command))
    }
    pub(super) fn editor_row(&self) -> i32 {
        self.rust().shown.editor().and_then(|editor| editor.row).map_or(-1, qt_int)
    }
    pub(super) fn editor_text(&self) -> QString {
        self.rust().shown.editor().map_or_else(QString::default, |editor| QString::from(editor.text.as_str()))
    }
    pub(super) fn commit_name(mut self: Pin<&mut Self>, command: i32, text: &QString, previous: i64) -> i64 {
        let browser = self.rust().browser;
        let Some(command) = operation_command(command) else { return -1 };
        let text = text.to_string();
        let result = with_session(|session| session.commit_name(browser, command, &text, operation_id(previous))).unwrap_or(Ok(None));
        self.as_mut().session_changed();
        match result {
            Ok(Some(job)) => job_code(job),
            Ok(None) => -1,
            Err(error) => {
                self.as_mut().rust_mut().get_mut().name_error = QString::from(error);
                -2
            }
        }
    }
    pub(super) fn name_error(&self) -> QString {
        self.rust().name_error.clone()
    }
    pub(super) fn editor_outcome(&self, job: i64) -> i32 {
        match operation_id(job).and_then(|job| with_session(|session| session.editor_outcome(job))) {
            Some(EditorOutcome::Pending) => 0,
            Some(EditorOutcome::Rejected(_)) => 1,
            Some(EditorOutcome::Succeeded) => 2,
            Some(EditorOutcome::Closed) | None => 3,
        }
    }
    pub(super) fn editor_outcome_text(&self, job: i64) -> QString {
        match operation_id(job).and_then(|job| with_session(|session| session.editor_outcome(job))) {
            Some(EditorOutcome::Rejected(text)) => QString::from(text),
            _ => QString::default(),
        }
    }
    pub(super) fn cancel_operation(mut self: Pin<&mut Self>, job: i64) {
        if let Some(id) = operation_id(job) {
            with_session(|session| session.submit(dual_pane_application::Command::CancelOperation { id }));
            self.as_mut().session_changed();
        }
    }
    pub(super) fn row_is_package(&self, row: i32) -> bool {
        usize::try_from(row).is_ok_and(|row| self.rust().shown.is_package(row))
    }
    pub(super) fn show_package_contents(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ShowPackageContents { row });
    }
    pub(super) fn set_status_token(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().status_token != value {
            self.as_mut().rust_mut().get_mut().status_token = value;
            self.status_token_changed();
        }
    }
    pub(super) fn set_editor_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().editor_revision != value {
            self.as_mut().rust_mut().get_mut().editor_revision = value;
            self.editor_revision_changed();
        }
    }
}
