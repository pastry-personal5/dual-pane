//! Platform-neutral file-manager values and policies.
//!
//! This crate performs no I/O and knows nothing about threads, Qt, or macOS.

mod entry;
mod listing_error;
mod location;
mod request_token;

pub use entry::{Entry, EntryKind, listing_order};
pub use listing_error::{ListingError, ListingErrorKind};
pub use location::{EntryName, InvalidEntryName, Location};
pub use request_token::RequestToken;
