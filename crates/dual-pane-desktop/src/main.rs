mod browser_session;
mod folder_items;
mod folder_items_list_model;
mod native_location;
mod runtime;
pub mod settings_storage;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::browser_session::BrowserStartup;
use crate::runtime::{FolderItemsSource, FolderItemsSourceFactory};
fn main() -> ExitCode {
    let location = env::current_dir().ok().and_then(|path| native_location::location_from_path(&path)).unwrap_or_else(Location::root);
    let home = env::var_os("HOME").and_then(|path| native_location::location_from_path(std::path::Path::new(&path))).unwrap_or_else(Location::root);
    let source_factory: FolderItemsSourceFactory = Arc::new(|| {
        let source: FolderItemsSource = Box::new(folder_items::read);
        source
    });
    let settings_path = env::var_os("HOME").map(PathBuf::from).map(|home| home.join("Library/Application Support/Dual Pane/settings.sqlite3")).unwrap_or_else(|| PathBuf::from("/tmp/dual-pane-settings.sqlite3"));
    let screenshots_exists = env::var_os("HOME").map(PathBuf::from).is_some_and(|home| home.join("Documents/Screenshots").is_dir());
    let startup = BrowserStartup { location, home, screenshots_exists, settings_path, source_factory };
    let status = folder_items_list_model::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
