use std::sync::Arc;

use dual_pane_domain::{Entry, EntryName, ListingError, Location, RequestToken};

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
    /// Open the named entry of the current listing.
    OpenEntry(EntryName),
    /// Show the location that contains the current one.
    GoToParent,
}

/// The typed result of outside work requested by the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory read succeeded. `entries` are already in listing order.
    ListingLoaded {
        token: RequestToken,
        location: Location,
        entries: Arc<[Entry]>,
    },
    ListingFailed {
        token: RequestToken,
        error: ListingError,
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
