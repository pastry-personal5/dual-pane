use std::sync::Arc;

use dual_pane_domain::{Entry, EntryName, ListingErrorKind, Location, RequestToken};

/// Everything the workspace reacts to, processed one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    Event(Event),
}

/// A request made by a person or by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Show the given location in the pane.
    Navigate(Location),
    /// Select the entry at `row` if its exact name still matches.
    SelectEntry { row: usize, name: EntryName },
    /// Open the entry at `row` if its exact name still matches.
    OpenEntry { row: usize, name: EntryName },
    /// Show the location that contains the current one.
    GoToParent,
}

/// The typed result of outside work requested by the workspace. The token
/// alone identifies the request, and so the location it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory read succeeded. `entries` are already in listing order.
    ListingLoaded {
        token: RequestToken,
        entries: Arc<[Entry]>,
    },
    ListingFailed {
        token: RequestToken,
        kind: ListingErrorKind,
    },
    /// A requested listing was stopped before it produced a result.
    ListingCancelled {
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
