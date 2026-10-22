//! Input from a Browser view, observed against what was on screen.

use super::*;

impl<R: WorkRunner> WorkspaceSession<R> {
    /// Clears `browser`'s Status Bar reason when its display time ends.
    pub fn expire_status(&mut self, browser: BrowserSide, token: u64) {
        match browser {
            BrowserSide::Left => self.left.expire_status(token),
            BrowserSide::Right => self.right.expire_status(token),
        }
    }
    #[cfg(test)]
    pub fn submit_ui(&mut self, browser: BrowserSide, event: UiEvent) {
        self.submit(Command::ActivateBrowser { browser });
        if let Some(command) = self.controller.command(browser, event, self.view(browser)) {
            self.submit_visible_command(browser, command);
        }
    }
    /// Submits a gesture using the view the native model had displayed when
    /// the gesture arrived. The application validates this observation after
    /// any newer projection or tab activation.
    pub fn submit_observed_ui(&mut self, browser: BrowserSide, event: UiEvent, shown: BrowserViewModel) {
        self.submit(Command::ActivateBrowser { browser });
        let Some(command) = self.controller.command(browser, event, &shown) else { return };
        if let Some((_, tab)) = command.visible_target() {
            self.submit(Input::ObservedCommand { command, browser, tab, visible_revision: shown.visible_revision() });
        } else {
            self.submit(command);
        }
    }
    /// Records a scroll hint against the rows the native model displayed.
    pub fn submit_observed_view_state(&mut self, browser: BrowserSide, event: UiEvent, shown: BrowserViewModel) {
        let Some(command) = self.controller.command(browser, event, &shown) else { return };
        if let Some((_, tab)) = command.visible_target() {
            self.submit(Input::ObservedCommand { command, browser, tab, visible_revision: shown.visible_revision() });
        }
    }
    /// Uses a menu gesture's original observation even if its action arrives
    /// after the view has changed.
    pub fn submit_ui_at_observation(&mut self, browser: BrowserSide, event: UiEvent, tab: u64, visible_revision: u64) {
        let Some(command) = self.controller.command(browser, event, self.view(browser)) else { return };
        if command.visible_target().is_some() {
            self.submit(Input::ObservedCommand { command, browser, tab: dual_pane_domain::TabId::new(tab), visible_revision });
        } else {
            self.submit(command);
        }
    }
    /// Applies a filter edit only to the tab whose row produced the edit.
    /// A delayed Qt signal cannot retarget a newly active tab.
    pub fn set_filter_text_observed(&mut self, browser: BrowserSide, observed_tab: u64, text: String) {
        let tab = dual_pane_domain::TabId::new(observed_tab);
        if self.view(browser).active_tab() != Some(tab) {
            return;
        }
        self.submit(Command::SetFilterText { browser, tab, text });
    }
    /// Submits entered path text for the tab the editor observed. `origin` is
    /// the exact location the editor was filled with and is used only when the
    /// text was not edited. Text that is not valid UTF-16 is refused.
    pub fn submit_path_observed(&mut self, browser: BrowserSide, observed_tab: u64, text: Option<String>, origin: Option<Location>, edited: bool) {
        let tab = TabId::new(observed_tab);
        if self.view(browser).active_tab() != Some(tab) {
            return;
        }
        let shown_text = text.clone().unwrap_or_default();
        self.presenter_mut(browser).note_entered_path(&shown_text);
        let submission = match (text, origin) {
            (None, _) => PathSubmission::Undecodable,
            (Some(_), Some(location)) if !edited => PathSubmission::Unchanged(location),
            (Some(text), _) => PathSubmission::Typed(text),
        };
        self.submit(Command::SubmitPath { browser, tab, submission });
    }
    /// Escape in the Path Edit Control while a check is pending.
    pub fn cancel_path_entry(&mut self, browser: BrowserSide, observed_tab: u64) {
        let tab = TabId::new(observed_tab);
        if self.view(browser).active_tab() == Some(tab) {
            self.submit(Command::CancelPathEntry { browser, tab });
        }
    }
    /// Forgets failed or pending path text after an edit, Escape, or focus loss.
    pub fn discard_path_entry(&mut self, browser: BrowserSide) {
        self.presenter_mut(browser).discard_path_entry();
    }
    fn presenter_mut(&mut self, browser: BrowserSide) -> &mut BrowserPresenter {
        match browser {
            BrowserSide::Left => &mut self.left,
            BrowserSide::Right => &mut self.right,
        }
    }
    /// Records view state, such as a scroll position, that a Browser reports
    /// without the person acting on it, so it never activates that Browser.
    #[cfg(test)]
    pub fn submit_view_state(&mut self, browser: BrowserSide, event: UiEvent) {
        if let Some(command) = self.controller.command(browser, event, self.view(browser)) {
            self.submit_visible_command(browser, command);
        }
    }
    #[cfg(test)]
    pub(super) fn submit_visible_command(&mut self, browser: BrowserSide, command: Command) {
        if let Some((_, tab)) = command.visible_target() {
            let visible_revision = self.view(browser).visible_revision();
            self.submit(Input::ObservedCommand { command, browser, tab, visible_revision });
        } else {
            self.submit(command);
        }
    }
}
