mod listing_model;
mod native_location;
mod pane_session;
mod runtime;
mod synthetic_listing;

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::pane_session::PaneStartup;
use crate::runtime::{ListingSource, ListingSourceFactory};
use crate::synthetic_listing::SyntheticListing;

/// How many rows the synthetic listing shows.
const SYNTHETIC_ROWS: usize = 100_000;

fn main() -> ExitCode {
    let location = env::current_dir().ok().and_then(|path| native_location::location_from_path(&path)).unwrap_or_else(Location::root);
    let listing = SyntheticListing::new(SYNTHETIC_ROWS);
    let source_factory: ListingSourceFactory = Arc::new(move || {
        let source: ListingSource = Box::new(move |location, cancelled| listing.read(location, cancelled));
        source
    });
    let startup = PaneStartup { location, source_factory };
    let status = listing_model::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
