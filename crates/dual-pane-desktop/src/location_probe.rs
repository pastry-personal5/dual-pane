use std::fs;
use std::io;

use dual_pane_application::{FavoriteProbeOutcome, ResolvedTarget};
use dual_pane_domain::Location;

use crate::native_location::path_from_location;
use crate::native_shell::is_bundle;

/// Whether `location` is a folder, following symbolic links. A missing item
/// or one that is not a folder is `Unavailable`; any other failure, such as a
/// privacy denial, proves nothing and is `Failed`. The runtime invokes this
/// only on its probe worker.
pub fn probe_directory(location: &Location) -> FavoriteProbeOutcome {
    match fs::metadata(path_from_location(location)) {
        Ok(metadata) if metadata.is_dir() => FavoriteProbeOutcome::Available,
        Ok(_) => FavoriteProbeOutcome::Unavailable,
        Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => FavoriteProbeOutcome::Unavailable,
        Err(_) => FavoriteProbeOutcome::Failed,
    }
}

/// What the link at `location` resolves to, for activating it: a folder
/// navigates through the link, while a regular file or a package opens.
/// Anything missing or unreadable is `Unavailable`. The runtime invokes this
/// only on its probe worker.
pub fn resolve_item(location: &Location) -> ResolvedTarget {
    let path = path_from_location(location);
    match fs::metadata(&path) {
        // Qt does not follow the link when it recognizes a package.
        Ok(metadata) if metadata.is_dir() && fs::canonicalize(&path).is_ok_and(|target| is_bundle(&target)) => ResolvedTarget::Package,
        Ok(metadata) if metadata.is_dir() => ResolvedTarget::Folder,
        Ok(metadata) if metadata.is_file() => ResolvedTarget::File,
        _ => ResolvedTarget::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::*;
    use crate::native_location::location_from_path;

    #[test]
    fn only_an_existing_folder_or_folder_link_is_available() {
        let temporary = tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join("folder")).unwrap();
        File::create(root.join("file")).unwrap();
        symlink("folder", root.join("folder-link")).unwrap();
        symlink("missing", root.join("dangling-link")).unwrap();
        let probe = |name: &str| probe_directory(&location_from_path(&root.join(name)).unwrap());
        assert_eq!(probe("folder"), FavoriteProbeOutcome::Available);
        assert_eq!(probe("folder-link"), FavoriteProbeOutcome::Available);
        assert_eq!(probe("file"), FavoriteProbeOutcome::Unavailable);
        assert_eq!(probe("missing"), FavoriteProbeOutcome::Unavailable);
        assert_eq!(probe("dangling-link"), FavoriteProbeOutcome::Unavailable);
        assert_eq!(probe("file/inside"), FavoriteProbeOutcome::Unavailable);
    }

    #[test]
    fn a_link_resolves_to_its_target_kind() {
        let temporary = tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("file"), b"").unwrap();
        fs::create_dir_all(root.join("Tool.app/Contents/MacOS")).unwrap();
        fs::write(root.join("Tool.app/Contents/Info.plist"), br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#).unwrap();
        for (link, target) in [("to-folder", "folder"), ("to-file", "file"), ("to-tool", "Tool.app"), ("dangling", "missing")] {
            symlink(target, root.join(link)).unwrap();
        }
        let resolve = |name: &str| resolve_item(&location_from_path(&root.join(name)).unwrap());
        assert_eq!(resolve("to-folder"), ResolvedTarget::Folder);
        assert_eq!(resolve("to-file"), ResolvedTarget::File);
        assert_eq!(resolve("to-tool"), ResolvedTarget::Package);
        assert_eq!(resolve("dangling"), ResolvedTarget::Unavailable);
    }
}
