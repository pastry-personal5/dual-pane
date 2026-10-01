use std::fs;
use std::io;

use dual_pane_application::FavoriteProbeOutcome;
use dual_pane_domain::Location;

use crate::native_location::path_from_location;

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
}
