mod browser_session;
mod clipboard_native;
mod folder_items;
mod folder_items_list_model;
mod launch_target;
mod location_probe;
mod location_watcher;
mod native_calls;
mod native_location;
mod native_shell;
mod operation_journal;
mod operation_lane;
mod operation_scan;
mod operation_step;
mod operations_bridge;
mod preview_reader;
mod runtime;
pub mod settings_storage;
mod workspace_bridge;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use dual_pane_domain::Location;

use crate::browser_session::BrowserStartup;
use crate::folder_items::ListingRules;
use crate::operation_journal::{LaunchId, is_launch_temporary};
use crate::runtime::{FolderItemsSource, FolderItemsSourceFactory, LocationProbe};
fn main() -> ExitCode {
    let target = match env::current_dir().map_err(|error| format!("cannot read the working directory: {error}")).and_then(|cwd| launch_target::parse(env::args_os().skip(1), &cwd, env::var_os("HOME").map(PathBuf::from).as_deref())) {
        Ok(launch_target::Launch::Run(target)) => target,
        Ok(launch_target::Launch::Help) => {
            println!("{}", launch_target::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("dual-pane: {message}");
            return ExitCode::from(2);
        }
    };
    // Debug verification uses a disposable profile without changing HOME for
    // the process or touching the person's normal settings database.
    #[cfg(debug_assertions)]
    let profile = match env::var_os("DUAL_PANE_TEST_PROFILE") {
        Some(path) if Path::new(&path).is_absolute() && Path::new(&path).is_dir() => Some(path),
        Some(_) => {
            eprintln!("DUAL_PANE_TEST_PROFILE must name an existing absolute directory");
            return ExitCode::FAILURE;
        }
        None => env::var_os("HOME"),
    };
    #[cfg(not(debug_assertions))]
    let profile = env::var_os("HOME");
    let home = profile.as_ref().and_then(|path| native_location::location_from_path(Path::new(path))).unwrap_or_else(Location::root);
    let launch = LaunchId::generate();
    // Listings hide this launch's operation temporaries and mark packages.
    let source_factory: FolderItemsSourceFactory = Arc::new(move || {
        let source: FolderItemsSource = Box::new(move |location, sort, visibility, cancelled| folder_items::read_listing(location, sort, visibility, cancelled, &ListingRules { hidden: &|name| is_launch_temporary(name, launch), package: &native_shell::is_bundle, volume_root: &native_calls::is_volume_root, invisible: &native_calls::is_macos_invisible }));
        source
    });
    let settings_path = profile.as_ref().map(|path| settings_storage::application_support_database_path(Path::new(path)));
    let location_probe: LocationProbe = Box::new(location_probe::probe_directory);
    let startup = BrowserStartup { home, settings_path, source_factory, location_probe, launch, target };
    let status = workspace_bridge::ffi::run_desktop(Box::new(startup));

    u8::try_from(status).map_or(ExitCode::FAILURE, ExitCode::from)
}
