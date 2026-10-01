use std::sync::Arc;

use dual_pane_domain::{BrowserSide, Entry, ListingError, Location, Selection};

/// What changed in the workspace, for presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    ActiveBrowserChanged {
        browser: BrowserSide,
    },
    /// The browser started loading `location`; its current listing stays shown.
    LoadingStarted {
        browser: BrowserSide,
        location: Location,
    },
    /// The browser now shows `location` with these entries, replacing the whole
    /// previous listing.
    FolderItemsReplaced {
        browser: BrowserSide,
        location: Location,
        entries: Arc<[Entry]>,
    },
    /// The browser's validated single selection changed.
    SelectionChanged {
        browser: BrowserSide,
        selection: Selection,
        row: Option<usize>,
    },
    /// Loading failed; the browser keeps its previous location and listing.
    FolderItemsFailed {
        browser: BrowserSide,
        error: ListingError,
    },
    /// Loading ended because its request was cancelled. This is deliberately
    /// distinct from an error so presenters can keep it silent.
    FolderItemsCancelled {
        browser: BrowserSide,
    },
}
