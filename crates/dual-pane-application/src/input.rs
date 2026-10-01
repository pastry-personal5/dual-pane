use std::sync::Arc;

use dual_pane_domain::{BrowserSide, Entry, EntryName, ListingErrorKind, Location, RequestToken};

/// Everything the workspace reacts to, processed one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    Event(Event),
}

/// A request made by a person or by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ActivateBrowser {
        browser: BrowserSide,
    },
    /// Show the given location in the browser.
    Navigate {
        browser: BrowserSide,
        location: Location,
    },
    /// Select the entry at `row` if its exact name still matches.
    SelectEntry {
        browser: BrowserSide,
        row: usize,
        name: EntryName,
    },
    ClearSelection {
        browser: BrowserSide,
    },
    /// Open the entry at `row` if its exact name still matches.
    OpenEntry {
        browser: BrowserSide,
        row: usize,
        name: EntryName,
    },
    /// Show the location that contains the current one.
    GoToParent {
        browser: BrowserSide,
    },
}

/// The typed result of outside work requested by the workspace. The token
/// alone identifies the request, and so the location it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory read succeeded. `entries` are already in listing order.
    FolderItemsLoaded {
        browser: BrowserSide,
        token: RequestToken,
        entries: Arc<[Entry]>,
    },
    FolderItemsFailed {
        browser: BrowserSide,
        token: RequestToken,
        kind: ListingErrorKind,
    },
    /// A requested listing was stopped before it produced a result.
    FolderItemsCancelled {
        browser: BrowserSide,
        token: RequestToken,
    },
}

impl From<Command> for Input {
    fn from(command: Command) -> Self {
        Self::Command(command)
    }
}

impl From<Event> for Input {
    fn from(event: Event) -> Self {
        Self::Event(event)
    }
}
