use dual_pane_application::Command;
use dual_pane_domain::{BrowserSide, ScrollAnchor, SortSpec, TabId};

use crate::{BrowserViewModel, WorkspaceViewModel};

/// A user action in a Browser, independent of the widget, pointer gesture,
/// menu item, or key that produced it. Every event first activates its
/// Browser. Gestures on the shown Folder Items name the tab the view showed,
/// so the application can reject them after a tab switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    /// Keyboard focus or a pointer press entered the Browser. Activation is
    /// the whole gesture, so no further command follows.
    FocusBrowser,
    /// A plain click: select only the entry shown in `row`.
    SelectRow {
        row: usize,
    },
    /// A Command-click: toggle the entry in `row` and move the cursor there.
    ToggleRow {
        row: usize,
    },
    /// A Shift-click: select from the range anchor to `row`.
    ExtendToRow {
        row: usize,
    },
    /// A right-click on `row`.
    SecondaryRow {
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
    /// Shift-movement: extend the range from the anchor to the row a plain
    /// movement would select.
    ExtendSelection(SelectionMovement),
    SelectAll,
    /// Open the only selected entry; nothing happens unless exactly one Item
    /// is selected, wherever the cursor is.
    ActivateSelection,
    ClearSelection,
    GoToParent,
    GoBack,
    GoForward,
    Refresh,
    NewTab,
    /// Close the active tab.
    CloseActiveTab,
    ActivateTab {
        tab: TabId,
    },
    CloseTab {
        tab: TabId,
    },
    /// A tab dragged to `position` in the strip.
    ReorderTab {
        tab: TabId,
        position: usize,
    },
    /// Sort the active tab's folder.
    Sort(SortSpec),
    /// The view scrolled so `row` is the first visible row, `offset` pixels
    /// above the viewport's top edge; `None` when no row is visible.
    Scrolled {
        row: Option<usize>,
        offset: i32,
    },
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

/// A Sidebar Favorites gesture. Groups and Items are named by stable ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoritesEvent {
    CreateGroup {
        name: String,
    },
    RenameGroup {
        id: i64,
        name: String,
    },
    MoveGroup {
        id: i64,
        motion: GroupMotion,
    },
    DeleteGroup {
        id: i64,
    },
    /// Add the active Browser Tab's current folder to the Group.
    AddItem {
        group_id: i64,
    },
    RenameItem {
        id: i64,
        name: String,
    },
    RemoveItem {
        id: i64,
    },
    /// An Item dropped into a Group before the Item shown at `slot`, or at
    /// the end when `slot` is the Group's Item count.
    DropItem {
        id: i64,
        group_id: i64,
        slot: usize,
    },
    OpenItem {
        id: i64,
    },
}

/// A Favorite Group Menu movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMotion {
    Up,
    Down,
    Top,
    Bottom,
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
        let tab = view.active_tab();
        let at = |row: usize| Some((tab?, row, view.entry(row)?.name().clone()));
        match event {
            UiEvent::FocusBrowser => None,
            UiEvent::SelectRow { row } => at(row).map(|(tab, row, name)| Command::SelectEntry { browser, tab, row, name }),
            UiEvent::ToggleRow { row } => at(row).map(|(tab, row, name)| Command::ToggleEntry { browser, tab, row, name }),
            UiEvent::ExtendToRow { row } => at(row).map(|(tab, row, name)| Command::SelectRange { browser, tab, row, name }),
            UiEvent::SecondaryRow { row } => at(row).map(|(tab, row, name)| Command::SecondarySelect { browser, tab, row, name }),
            UiEvent::ActivateRow { row } => at(row).map(|(tab, row, name)| Command::OpenEntry { browser, tab, row, name }),
            UiEvent::MoveSelection(movement) => at(moved_row(view, movement)?).map(|(tab, row, name)| Command::MoveSelection { browser, tab, row, name }),
            UiEvent::ExtendSelection(movement) => at(moved_row(view, movement)?).map(|(tab, row, name)| Command::SelectRange { browser, tab, row, name }),
            UiEvent::ActivateSelection => at(view.single_selected_row()?).map(|(tab, row, name)| Command::OpenEntry { browser, tab, row, name }),
            UiEvent::SelectAll => Some(Command::SelectAll { browser, tab: tab? }),
            UiEvent::ClearSelection => Some(Command::ClearSelection { browser, tab: tab? }),
            UiEvent::GoToParent => Some(Command::GoToParent { browser, tab: tab? }),
            UiEvent::GoBack => Some(Command::GoBack { browser, tab: tab? }),
            UiEvent::GoForward => Some(Command::GoForward { browser, tab: tab? }),
            UiEvent::Refresh => Some(Command::Refresh { browser, tab: tab? }),
            UiEvent::NewTab => Some(Command::NewTab { browser }),
            UiEvent::CloseActiveTab => Some(Command::CloseTab { browser, tab: tab? }),
            UiEvent::ActivateTab { tab } => Some(Command::ActivateTab { browser, tab }),
            UiEvent::CloseTab { tab } => Some(Command::CloseTab { browser, tab }),
            UiEvent::ReorderTab { tab, position } => Some(Command::ReorderTab { browser, tab, position }),
            UiEvent::Sort(sort) => Some(Command::SetSort { browser, tab: tab?, location: view.location()?.clone(), sort }),
            UiEvent::Scrolled { row, offset } => Some(Command::UpdateScrollHint { browser, tab: tab?, scroll: row.and_then(|row| view.entry(row)).map(|entry| ScrollAnchor::new(entry.name().clone(), offset)) }),
        }
    }

    /// The command for a Sidebar gesture. `browser` and `active` are the
    /// active Browser and what it shows, for adding and opening Items.
    pub fn favorites_command(&self, event: FavoritesEvent, favorites: &WorkspaceViewModel, browser: BrowserSide, active: &BrowserViewModel) -> Option<Command> {
        Some(match event {
            FavoritesEvent::CreateGroup { name } => Command::CreateFavoriteGroup { name },
            FavoritesEvent::RenameGroup { id, name } => Command::RenameFavoriteGroup { id, name },
            FavoritesEvent::MoveGroup { id, motion } => {
                let (index, _) = favorites.group(id)?;
                let last = favorites.groups().len().saturating_sub(1);
                let position = match motion {
                    GroupMotion::Up => index.saturating_sub(1),
                    GroupMotion::Down => (index + 1).min(last),
                    GroupMotion::Top => 0,
                    GroupMotion::Bottom => last,
                };
                Command::ReorderFavoriteGroup { id, position }
            }
            FavoritesEvent::DeleteGroup { id } => Command::DeleteFavoriteGroup { id },
            FavoritesEvent::AddItem { group_id } => Command::CreateFavoriteItem { group_id, name: active.folder_name().to_owned(), target: active.location()?.clone() },
            FavoritesEvent::RenameItem { id, name } => Command::RenameFavoriteItem { id, name },
            FavoritesEvent::RemoveItem { id } => Command::DeleteFavoriteItem { id },
            FavoritesEvent::DropItem { id, group_id, slot } => {
                // The application positions an Item among the Group's other
                // Items, so a slot below its own row moves up by one.
                let (group, index) = favorites.item(id)?;
                let position = if group.id == group_id && slot > index { slot - 1 } else { slot };
                Command::MoveFavoriteItem { id, group_id, position }
            }
            FavoritesEvent::OpenItem { id } => Command::OpenFavoriteItem { browser, id },
        })
    }
}

/// The row a plain movement selects, starting from the cursor.
fn moved_row(view: &BrowserViewModel, movement: SelectionMovement) -> Option<usize> {
    let last = view.row_count().checked_sub(1)?;
    Some(match (view.cursor_row(), movement) {
        (None, SelectionMovement::Last | SelectionMovement::PageDown { .. }) | (Some(_), SelectionMovement::Last) => last,
        (None, _) | (Some(_), SelectionMovement::First) => 0,
        (Some(row), SelectionMovement::Previous) => row.saturating_sub(1),
        (Some(row), SelectionMovement::Next) => row.saturating_add(1).min(last),
        (Some(row), SelectionMovement::PageUp { rows }) => row.saturating_sub(rows.max(1)),
        (Some(row), SelectionMovement::PageDown { rows }) => row.saturating_add(rows.max(1)).min(last),
    })
}
