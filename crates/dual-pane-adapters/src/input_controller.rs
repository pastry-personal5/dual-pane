use dual_pane_application::Command;
use dual_pane_domain::BrowserSide;

use crate::BrowserViewModel;

/// A user action in the browser, independent of the widget, pointer gesture,
/// menu item, or key that produced it. Every event first activates its
/// Browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    /// Keyboard focus or a pointer press entered the Browser. Activation is
    /// the whole gesture, so no further command follows.
    FocusBrowser,
    /// Select the entry shown in `row`.
    SelectRow {
        row: usize,
    },
    /// Open the entry shown in `row`.
    ActivateRow {
        row: usize,
    },
    /// Select a row relative to the cursor row. Movement clamps at the first
    /// and last rows; without a cursor, `Last` and `PageDown` select the last
    /// row and every other movement selects the first.
    MoveSelection(SelectionMovement),
    /// Open the entry at the cursor row, if any.
    ActivateSelection,
    ClearSelection,
    GoToParent,
}

/// A selection movement. A page is the number of fully visible rows, which
/// only the view can measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMovement {
    Previous,
    Next,
    First,
    Last,
    PageUp { rows: usize },
    PageDown { rows: usize },
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
    /// event adds nothing to activation or refers to a row that is not shown.
    pub fn command(&self, browser: BrowserSide, event: UiEvent, view: &BrowserViewModel) -> Option<Command> {
        match event {
            UiEvent::FocusBrowser => None,
            UiEvent::SelectRow { row } => view.entry(row).map(|entry| Command::SelectEntry { browser, row, name: entry.name().clone() }),
            UiEvent::ActivateRow { row } => view.entry(row).map(|entry| Command::OpenEntry { browser, row, name: entry.name().clone() }),
            UiEvent::MoveSelection(movement) => {
                let last = view.row_count().checked_sub(1)?;
                let row = match (view.selected_row(), movement) {
                    (None, SelectionMovement::Last | SelectionMovement::PageDown { .. }) | (Some(_), SelectionMovement::Last) => last,
                    (None, _) | (Some(_), SelectionMovement::First) => 0,
                    (Some(row), SelectionMovement::Previous) => row.saturating_sub(1),
                    (Some(row), SelectionMovement::Next) => row.saturating_add(1).min(last),
                    (Some(row), SelectionMovement::PageUp { rows }) => row.saturating_sub(rows.max(1)),
                    (Some(row), SelectionMovement::PageDown { rows }) => row.saturating_add(rows.max(1)).min(last),
                };
                view.entry(row).map(|entry| Command::MoveSelection { browser, row, name: entry.name().clone() })
            }
            UiEvent::ActivateSelection => view.selected_row().and_then(|row| self.command(browser, UiEvent::ActivateRow { row }, view)),
            UiEvent::ClearSelection => Some(Command::ClearSelection { browser }),
            UiEvent::GoToParent => Some(Command::GoToParent { browser }),
        }
    }
}
