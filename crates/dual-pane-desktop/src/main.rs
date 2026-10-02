mod browser_session;
mod folder_items;
mod folder_items_list_model;
mod location_probe;
mod native_location;
mod runtime;
pub mod settings_storage;
mod workspace_bridge;

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::browser_session::BrowserStartup;
use crate::runtime::{FolderItemsSource, FolderItemsSourceFactory, LocationProbe};
fn main() -> ExitCode {
    let location = env::current_dir().ok().and_then(|path| native_location::location_from_path(&path)).unwrap_or_else(Location::root);
    let home = env::var_os("HOME").and_then(|path| native_location::location_from_path(Path::new(&path))).unwrap_or_else(Location::root);
    let source_factory: FolderItemsSourceFactory = Arc::new(|| {
        let source: FolderItemsSource = Box::new(folder_items::read);
        source
    });
    let settings_path = env::var_os("HOME").map(|home| settings_storage::application_support_database_path(Path::new(&home)));
    let location_probe: LocationProbe = Box::new(location_probe::probe_directory);
    let startup = BrowserStartup { location, home, settings_path, source_factory, location_probe };
    let status = workspace_bridge::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
