use crate::FavoritesRecords;
use dual_pane_domain::{BrowserSide, Entry, EntryName, ListingError, Location, Selection, TabId};
use std::sync::Arc;

/// A framework-neutral contiguous listing mutation. `removed` rows beginning
/// at `row` are replaced by `inserted` rows. A whole-list replacement is still
/// an explicit, valid delta rather than an implicit widget reset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowChange {
    pub row: usize,
    pub removed: usize,
    pub inserted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    ActiveBrowserChanged { browser: BrowserSide },
    ActiveTabChanged { browser: BrowserSide, tab: TabId },
    TabsChanged { browser: BrowserSide, active_tab: TabId },
    TabViewChanged { browser: BrowserSide, tab: TabId, location: Option<Location>, entries: Arc<[Entry]>, selection: Selection, row: Option<usize>, scroll_hint: Option<(EntryName, i32)>, loading: bool, error: Option<ListingError> },
    LoadingStarted { browser: BrowserSide, tab: TabId, location: Location },
    FolderItemsReplaced { browser: BrowserSide, tab: TabId, location: Location, entries: Arc<[Entry]>, scroll_hint: Option<(EntryName, i32)> },
    FolderItemsRowsChanged { browser: BrowserSide, tab: TabId, changes: Vec<RowChange> },
    SelectionChanged { browser: BrowserSide, tab: TabId, selection: Selection, row: Option<usize> },
    FolderItemsFailed { browser: BrowserSide, tab: TabId, error: ListingError },
    FolderItemsCancelled { browser: BrowserSide, tab: TabId },
    FavoritesChanged { favorites: FavoritesRecords },
    SettingsSaveFailed { revision: u64 },
}
