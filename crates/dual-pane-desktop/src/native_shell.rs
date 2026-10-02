//! Qt's macOS shell services, called with exact path bytes: moving an item
//! to the Trash, recognizing a package, and opening an item with its default
//! application.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
mod ffi {
    unsafe extern "C++" {
        include!("dual_pane_desktop/native_shell.hpp");
        fn move_to_trash(path: &[u8], trashed: &mut Vec<u8>) -> bool;
        fn is_bundle(path: &[u8]) -> bool;
        fn open_with_default_application(path: &[u8]) -> bool;
    }
}

/// Moves `path` to its volume's Trash with `QFile::moveToTrash` and returns
/// where it went. Qt reports only success or failure.
pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    let mut trashed = Vec::new();
    if ffi::move_to_trash(path.as_os_str().as_bytes(), &mut trashed) { Ok(PathBuf::from(OsString::from_vec(trashed))) } else { Err(io::Error::other("the item could not be moved to the Trash")) }
}

/// Whether the folder at `path` is a macOS package, such as an application.
/// `QFileInfo` is reentrant, so a listing worker may ask.
pub fn is_bundle(path: &Path) -> bool {
    ffi::is_bundle(path.as_os_str().as_bytes())
}

/// Asks macOS to open `path` with its default application. Call it on the
/// GUI thread; it reports only whether macOS accepted the request.
pub fn open_with_default_application(path: &Path) -> bool {
    ffi::open_with_default_application(path.as_os_str().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_named_like_an_application_with_a_bundle_layout_is_a_package() {
        let directory = tempfile::tempdir().unwrap();
        let plain = directory.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        assert!(!is_bundle(&plain));
        let application = directory.path().join("Tool.app");
        std::fs::create_dir_all(application.join("Contents/MacOS")).unwrap();
        std::fs::write(application.join("Contents/Info.plist"), br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#).unwrap();
        let worker = std::thread::spawn(move || is_bundle(&application));
        assert!(worker.join().unwrap(), "a worker thread recognizes the package");
    }

    /// Settles whether `QFile::moveToTrash` works off the GUI thread. It moves
    /// a temporary file into the real Trash of its volume, outside any
    /// temporary folder, so it runs only on request.
    #[test]
    #[ignore = "moves a file into the user's real Trash"]
    fn moving_to_the_trash_works_on_a_worker_thread() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("dual-pane-trash-check.txt");
        std::fs::write(&file, b"check").unwrap();
        let moved = std::thread::spawn(move || move_to_trash(&file).map(|trashed| (file, trashed))).join().unwrap();
        let (file, trashed) = moved.unwrap();
        assert!(!file.exists());
        assert!(trashed.exists(), "the item is in the Trash at {trashed:?}");
    }
}
