//! Platform-neutral file-manager values and policies.
//!
//! This crate performs no I/O and knows nothing about threads, Qt, or macOS.

mod browser;
mod browser_side;
mod entry;
mod favorites;
mod listing_error;
mod location;
mod operation;
mod request_token;
mod selection;
mod sort_spec;
mod tab;
mod tab_history;

pub use browser::BrowserTabs;
pub use browser_side::BrowserSide;
pub use entry::{Activation, Entry, EntryKind, EntryMetadata, ListingSortKey, entry_type_text, listing_sort_key, sort_entries};
pub use favorites::{FavoriteError, FavoriteGroup, FavoriteGroupId, FavoriteItem, FavoriteItemId, Favorites, valid_favorite_name};
pub use listing_error::{ListingError, ListingErrorKind};
pub use location::{EntryName, InvalidEntryName, Location};
pub use operation::{DecisionToken, OperationChoice, OperationCommand, OperationId, OperationIntent, OperationIssue, OperationKind, OperationRejection, OperationTarget, starts_with};
pub use request_token::RequestToken;
pub use selection::Selection;
pub use sort_spec::{SortDirection, SortField, SortSpec};
pub use tab::TabId;
pub use tab_history::{ScrollAnchor, TabHistory, Visit, VisitState};
