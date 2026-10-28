//! Moving a damaged database aside.

use super::*;

/// Moves a failed database and its SQLite sidecars aside, then creates fresh
/// storage at `path`. `current` is closed first so SQLite releases the files.
/// On failure no file stays moved, and no in-memory stand-in is returned, so a
/// later save either reaches disk or reports its own failure.
pub fn reset_database(path: &Path, current: Option<SettingsDatabase>) -> (Option<SettingsDatabase>, Result<Option<PathBuf>, SettingsStorageError>) {
    let stamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_nanos(),
        Err(_) => return (current, Err(SettingsStorageError::InvalidData("clock before epoch"))),
    };
    reset_database_to(path, current, &path.with_extension(format!("failed-{stamp}")))
}

pub(super) fn reset_database_to(path: &Path, current: Option<SettingsDatabase>, backup: &Path) -> (Option<SettingsDatabase>, Result<Option<PathBuf>, SettingsStorageError>) {
    drop(current);
    let preserved = match preserve_files(path, backup) {
        Ok(preserved) => preserved,
        Err(error) => return (SettingsDatabase::open(path).ok(), Err(error)),
    };
    match SettingsDatabase::open(path) {
        Ok(database) => (Some(database), Ok(preserved.then(|| backup.to_path_buf()))),
        Err(error) => (None, Err(error)),
    }
}

pub(super) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut text = path.as_os_str().to_owned();
    text.push(suffix);
    PathBuf::from(text)
}

/// Moves the database and its sidecars to `backup`. If any move fails, the
/// files already moved are returned before the error is reported.
pub(super) fn preserve_files(path: &Path, backup: &Path) -> Result<bool, SettingsStorageError> {
    let mut moved = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let (from, to) = (with_suffix(path, suffix), with_suffix(backup, suffix));
        match fs::symlink_metadata(&from) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                restore_files(&moved);
                return Err(error.into());
            }
        }
        if let Err(error) = fs::rename(&from, &to) {
            restore_files(&moved);
            return Err(error.into());
        }
        moved.push((from, to));
    }
    Ok(moved.iter().any(|(from, _)| from == path))
}

fn restore_files(moved: &[(PathBuf, PathBuf)]) {
    for (from, to) in moved.iter().rev() {
        // Best effort: the original error is the one worth reporting.
        fs::rename(to, from).ok();
    }
}
