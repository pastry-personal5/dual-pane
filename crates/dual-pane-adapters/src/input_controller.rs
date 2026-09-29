use dual_pane_application::Command;

use crate::PaneViewModel;

/// A user action in the pane, independent of the widget, pointer gesture,
/// menu item, or key that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
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

    /// The command for `event` given what the pane shows, or `None` when the
    /// event refers to a row that is not shown.
    pub fn command(&self, event: UiEvent, view: &PaneViewModel) -> Option<Command> {
        match event {
            UiEvent::ActivateRow { row } => view.entry(row).map(|entry| Command::OpenEntry(entry.name().clone())),
            UiEvent::GoToParent => Some(Command::GoToParent),
        }
    }
}
