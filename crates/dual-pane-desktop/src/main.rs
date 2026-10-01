mod browser_session;
mod folder_items;
mod folder_items_list_model;
mod native_location;
mod runtime;

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::browser_session::BrowserStartup;
use crate::runtime::{FolderItemsSource, FolderItemsSourceFactory};
fn main() -> ExitCode {
    let location = env::current_dir().ok().and_then(|path| native_location::location_from_path(&path)).unwrap_or_else(Location::root);
    let source_factory: FolderItemsSourceFactory = Arc::new(|| {
        let source: FolderItemsSource = Box::new(folder_items::read);
        source
    });
    let startup = BrowserStartup { location, source_factory };
    let status = folder_items_list_model::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
