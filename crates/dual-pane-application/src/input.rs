use std::sync::Arc;

use crate::SettingsSnapshot;
use dual_pane_domain::{BrowserSide, Entry, EntryName, ListingErrorKind, Location, RequestToken, SortSpec, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    Event(Event),
}

/// UI-neutral workspace requests. Commands without a tab address the active
/// tab, retaining the compatibility boundary used by the current shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ActivateBrowser { browser: BrowserSide },
    ActivateTab { browser: BrowserSide, tab: TabId },
    NewTab { browser: BrowserSide },
    CloseTab { browser: BrowserSide, tab: TabId },
    ReorderTab { browser: BrowserSide, tab: TabId, position: usize },
    Navigate { browser: BrowserSide, location: Location },
    GoBack { browser: BrowserSide },
    GoForward { browser: BrowserSide },
    Refresh { browser: BrowserSide },
    InvalidateLocation { location: Location },
    SetSort { location: Location, sort: SortSpec },
    CreateFavoriteGroup { name: String },
    RenameFavoriteGroup { id: i64, name: String },
    ReorderFavoriteGroup { id: i64, position: usize },
    DeleteFavoriteGroup { id: i64 },
    CreateFavoriteItem { group_id: i64, name: String, target: Location },
    RenameFavoriteItem { id: i64, name: String },
    MoveFavoriteItem { id: i64, group_id: i64, position: usize },
    DeleteFavoriteItem { id: i64 },
    OpenFavoriteItem { browser: BrowserSide, id: i64 },
    SelectEntry { browser: BrowserSide, row: usize, name: EntryName },
    ToggleEntry { browser: BrowserSide, row: usize, name: EntryName },
    SelectAll { browser: BrowserSide },
    ClearSelection { browser: BrowserSide },
    OpenEntry { browser: BrowserSide, row: usize, name: EntryName },
    GoToParent { browser: BrowserSide },
    UpdateScrollHint { browser: BrowserSide, anchor: Option<EntryName>, offset: i32 },
    SelectRange { browser: BrowserSide, row: usize, name: EntryName },
    MoveSelection { browser: BrowserSide, row: usize, name: EntryName },
    SecondarySelect { browser: BrowserSide, row: usize, name: EntryName },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    FolderItemsLoaded { browser: BrowserSide, tab: TabId, token: RequestToken, entries: Arc<[Entry]> },
    FolderItemsFailed { browser: BrowserSide, tab: TabId, token: RequestToken, kind: ListingErrorKind },
    FolderItemsCancelled { browser: BrowserSide, tab: TabId, token: RequestToken },
    FavoriteTargetProbed { item_id: i64, target: Location, available: bool },
    SettingsSaved { revision: u64 },
    SettingsSaveFailed { revision: u64 },
    SettingsLoaded { snapshot: SettingsSnapshot },
}
impl From<Command> for Input {
    fn from(value: Command) -> Self {
        Self::Command(value)
    }
}
impl From<Event> for Input {
    fn from(value: Event) -> Self {
        Self::Event(value)
    }
}
