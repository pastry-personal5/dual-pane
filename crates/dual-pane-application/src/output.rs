use std::sync::Arc;

use dual_pane_domain::{Entry, ListingError, Location};

/// What changed in the workspace, for presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// The pane started loading `location`; its current listing stays shown.
    LoadingStarted { location: Location },
    /// The pane now shows `location` with these entries, replacing the whole
    /// previous listing.
    ListingReplaced { location: Location, entries: Arc<[Entry]> },
    /// Loading failed; the pane keeps its previous location and listing.
    ListingFailed { error: ListingError },
}
