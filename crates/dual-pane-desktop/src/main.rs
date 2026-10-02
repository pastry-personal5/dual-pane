mod browser_session;
mod folder_items;
mod folder_items_list_model;
mod location_probe;
mod native_calls;
mod native_location;
mod native_shell;
mod operation_journal;
mod operation_lane;
mod operation_scan;
mod operation_step;
mod operations_bridge;
mod runtime;
pub mod settings_storage;
mod workspace_bridge;

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::browser_session::BrowserStartup;
use crate::folder_items::ListingRules;
use crate::operation_journal::{LaunchId, is_launch_temporary};
use crate::runtime::{FolderItemsSource, FolderItemsSourceFactory, LocationProbe};
fn main() -> ExitCode {
    let location = env::current_dir().ok().and_then(|path| native_location::location_from_path(&path)).unwrap_or_else(Location::root);
    let home = env::var_os("HOME").and_then(|path| native_location::location_from_path(Path::new(&path))).unwrap_or_else(Location::root);
    let launch = LaunchId::generate();
    // Listings hide this launch's operation temporaries and mark packages.
    let source_factory: FolderItemsSourceFactory = Arc::new(move || {
        let source: FolderItemsSource = Box::new(move |location, sort, cancelled| folder_items::read_listing(location, sort, cancelled, &ListingRules { hidden: &|name| is_launch_temporary(name, launch), package: &native_shell::is_bundle }));
        source
    });
    let settings_path = env::var_os("HOME").map(|home| settings_storage::application_support_database_path(Path::new(&home)));
    let location_probe: LocationProbe = Box::new(location_probe::probe_directory);
    let startup = BrowserStartup { location, home, settings_path, source_factory, location_probe, launch };
    let status = workspace_bridge::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
