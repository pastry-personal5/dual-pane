//! File operations, clipboard commands, and quit confirmation.

use super::*;

impl<R: WorkRunner> WorkspaceSession<R> {
    /// Reveals and hides Operation Panels whose delay passed.
    pub fn tick_operations(&mut self) {
        self.operations.tick();
        let dismissals = self.operations.take_dismissals();
        for id in dismissals {
            self.submit(Command::DismissOperation { id });
        }
    }
    pub fn operations_view(&self) -> &OperationsViewModel {
        self.operations.view()
    }
    /// How long until the Operation Panels next need a tick.
    pub fn next_operations_deadline(&self) -> Option<Duration> {
        self.operations.next_deadline()
    }
    /// Whether `command` could start from `browser`'s active tab now, for
    /// enabling a menu item when the menu opens.
    pub fn file_command_available(&self, browser: BrowserSide, command: OperationCommand) -> bool {
        self.workspace.command_availability(browser, self.workspace.active_tab(browser), command).is_ok()
    }
    pub fn clipboard_command_available(&self, browser: BrowserSide, action: Option<ClipboardAction>) -> bool {
        let tab = self.workspace.active_tab(browser);
        match action {
            Some(_) => self.workspace.clipboard_capture_availability(browser, tab).is_ok(),
            None => self.workspace.paste_availability(browser, tab).is_ok(),
        }
    }
    pub fn submit_clipboard_observed(&mut self, browser: BrowserSide, action: Option<ClipboardAction>, tab: TabId, visible_revision: u64) {
        self.submit(Command::ActivateBrowser { browser });
        match action {
            Some(action) => self.submit(Input::ObservedCommand { command: Command::CaptureClipboard { browser, tab, action }, browser, tab, visible_revision }),
            // Paste targets the confirmed folder, not visible rows, so it has no observation to validate.
            None => self.submit(Command::PasteClipboard { browser, tab }),
        };
    }
    pub fn observe_clipboard(&mut self, generation: dual_pane_application::ClipboardGeneration, offer: dual_pane_application::ClipboardOffer) {
        self.submit(Event::ClipboardChanged { generation, offer });
    }
    /// Starts Rename or New Folder with the editor's `text`. A retry first
    /// closes the job that refused the previous name. Returns the new job,
    /// `Ok(None)` when the editor should simply close, as for an unchanged
    /// name, or the inline error for a name refused before any job.
    pub fn commit_name(&mut self, browser: BrowserSide, command: OperationCommand, text: &str, previous: Option<OperationId>) -> Result<Option<OperationId>, &'static str> {
        let name = if text.trim().is_empty() { return Err(name_rejection_text(OperationRejection::InvalidName).unwrap_or_default()) } else { EntryName::new(text).map_err(|_| UNREPRESENTABLE_NAME_TEXT)? };
        self.submit(Command::ActivateBrowser { browser });
        // A retry keeps the refused job's frozen target; only a first commit
        // reads what the tab shows now.
        let command = match previous {
            Some(id) => Command::RetryName { id, name },
            None => match self.controller.name_command(browser, command, name, self.view(browser)) {
                Some(command) => command,
                None => return Ok(None),
            },
        };
        let input = if let Some((_, tab)) = command.visible_target() { Input::ObservedCommand { command, browser, tab, visible_revision: self.view(browser).visible_revision() } } else { command.into() };
        let outputs = self.handle_all(input);
        for output in &outputs {
            match output {
                Output::OperationChanged { job } if Some(job.id()) != previous => return Ok(Some(job.id())),
                Output::OperationRejected { reason, .. } => return name_rejection_text(*reason).map_or(Ok(None), Err),
                _ => {}
            }
        }
        Ok(None)
    }
    /// What the inline editor of a committed name shows now.
    pub fn editor_outcome(&self, id: OperationId) -> EditorOutcome {
        self.operations.editor_outcome(id)
    }
    /// Applies a choice on the Operation Decision Card that showed `token`.
    /// A card replaced since then ignores the click.
    pub fn decide(&mut self, id: OperationId, token: DecisionToken, choice: OperationChoice, apply_to_all: bool) {
        let Some(card) = self.operations.view().panel(id).and_then(|panel| panel.decision.clone()).filter(|card| card.token == token) else { return };
        self.submit(Command::DecideOperation { id, token, item: card.item, choice, apply_to_all });
    }
    /// The person closed a panel; a finished job is dismissed.
    pub fn close_panel(&mut self, id: OperationId) {
        if self.operations.close(id) {
            self.tick_operations();
        }
    }
    /// Asks to quit and reports whether quitting may proceed now. With
    /// running operations the quit prompt opens instead.
    pub fn request_quit(&mut self, confirmed: bool) -> bool {
        self.submit(Command::Quit { confirmed });
        self.operations.view().quit_accepted()
    }
}
