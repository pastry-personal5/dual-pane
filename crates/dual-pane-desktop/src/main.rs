mod directory_listing;
mod listing_model;
mod native_location;
mod pane_session;
mod runtime;

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::pane_session::PaneStartup;
use crate::runtime::{ListingSource, ListingSourceFactory};
fn main() -> ExitCode {
    let location = env::current_dir().ok().and_then(|path| native_location::location_from_path(&path)).unwrap_or_else(Location::root);
    let source_factory: ListingSourceFactory = Arc::new(|| {
        let source: ListingSource = Box::new(directory_listing::read);
        source
    });
    let startup = PaneStartup { location, source_factory };
    let status = listing_model::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
