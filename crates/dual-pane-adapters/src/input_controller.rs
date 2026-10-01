use dual_pane_application::Command;
use dual_pane_domain::BrowserSide;

use crate::BrowserViewModel;

/// A user action in the browser, independent of the widget, pointer gesture,
/// menu item, or key that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    /// Select the entry shown in `row`.
    SelectRow {
        row: usize,
    },
    /// Open the entry shown in `row`.
    ActivateRow {
        row: usize,
    },
    GoToParent,
}

/// Maps UI events to application commands. It does not decide whether a
/// command is valid; the application does.
#[derive(Debug, Default)]
pub struct InputController;

impl InputController {
    pub fn new() -> Self {
        Self
    }

    /// The command for `event` given what the browser shows, or `None` when the
    /// event refers to a row that is not shown.
    pub fn command(&self, browser: BrowserSide, event: UiEvent, view: &BrowserViewModel) -> Option<Command> {
        match event {
            UiEvent::SelectRow { row } => view.entry(row).map(|entry| Command::SelectEntry { browser, row, name: entry.name().clone() }),
            UiEvent::ActivateRow { row } => view.entry(row).map(|entry| Command::OpenEntry { browser, row, name: entry.name().clone() }),
            UiEvent::GoToParent => Some(Command::GoToParent { browser }),
        }
    }
}
