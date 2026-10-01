//! Platform-neutral file-manager values and policies.
//!
//! This crate performs no I/O and knows nothing about threads, Qt, or macOS.

mod browser_side;
mod entry;
mod listing_error;
mod location;
mod request_token;
mod selection;

pub use browser_side::BrowserSide;
pub use entry::{Entry, EntryKind, ListingSortKey, listing_sort_key};
pub use listing_error::{ListingError, ListingErrorKind};
pub use location::{EntryName, InvalidEntryName, Location};
pub use request_token::RequestToken;
pub use selection::Selection;
