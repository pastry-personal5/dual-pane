use std::sync::Arc;

use dual_pane_domain::{Entry, EntryName, ListingErrorKind, Location, PaneSide, RequestToken};

/// Everything the workspace reacts to, processed one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    Event(Event),
}

/// A request made by a person or by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ActivatePane {
        pane: PaneSide,
    },
    /// Show the given location in the pane.
    Navigate {
        pane: PaneSide,
        location: Location,
    },
    /// Select the entry at `row` if its exact name still matches.
    SelectEntry {
        pane: PaneSide,
        row: usize,
        name: EntryName,
    },
    ClearSelection {
        pane: PaneSide,
    },
    /// Open the entry at `row` if its exact name still matches.
    OpenEntry {
        pane: PaneSide,
        row: usize,
        name: EntryName,
    },
    /// Show the location that contains the current one.
    GoToParent {
        pane: PaneSide,
    },
}

/// The typed result of outside work requested by the workspace. The token
/// alone identifies the request, and so the location it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory read succeeded. `entries` are already in listing order.
    ListingLoaded {
        pane: PaneSide,
        token: RequestToken,
        entries: Arc<[Entry]>,
    },
    ListingFailed {
        pane: PaneSide,
        token: RequestToken,
        kind: ListingErrorKind,
    },
    /// A requested listing was stopped before it produced a result.
    ListingCancelled {
        pane: PaneSide,
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
