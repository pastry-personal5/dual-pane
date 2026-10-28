//! Moving a damaged database aside.

use super::*;

/// Moves a failed database and its SQLite sidecars aside, then creates fresh
/// storage at `path`. `current` is closed first so SQLite releases the files.
/// A failed move restores earlier moves. If restoration cannot be completed,
/// no database is opened at the original path, so a later save cannot create
/// fresh state over an incomplete backup.
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
        Err(failure) => return (failure.restored.then(|| SettingsDatabase::open(path).ok()).flatten(), Err(failure.error)),
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

struct PreserveFailure {
    error: SettingsStorageError,
    restored: bool,
}

/// Moves the database and its sidecars to `backup`. If any move fails, every
/// earlier move is restored without replacing a file that appeared meanwhile.
fn preserve_files(path: &Path, backup: &Path) -> Result<bool, PreserveFailure> {
    let mut moved = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let (from, to) = (with_suffix(path, suffix), with_suffix(backup, suffix));
        match fs::symlink_metadata(&from) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(preservation_failure(error, &moved)),
        }
        // A reset must never replace an earlier backup, even if its timestamp
        // collides or another process creates that path during this reset.
        if let Err(error) = crate::native_calls::rename_exclusive(&from, &to) {
            return Err(preservation_failure(error, &moved));
        }
        moved.push((from, to));
    }
    Ok(moved.iter().any(|(from, _)| from == path))
}

fn preservation_failure(error: std::io::Error, moved: &[(PathBuf, PathBuf)]) -> PreserveFailure {
    match restore_files(moved) {
        Ok(()) => PreserveFailure { error: error.into(), restored: true },
        Err(restore) => PreserveFailure { error: std::io::Error::other(format!("reset backup rollback failed: {restore}; original error: {error}")).into(), restored: false },
    }
}

fn restore_files(moved: &[(PathBuf, PathBuf)]) -> std::io::Result<()> {
    let mut first_error = None;
    for (from, to) in moved.iter().rev() {
        if let Err(error) = crate::native_calls::rename_exclusive(to, from) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_does_not_replace_a_file_that_appeared_at_the_original_path() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("settings.sqlite3");
        let backup = directory.path().join("settings.failed");
        fs::write(&original, b"new occupant").unwrap();
        fs::write(&backup, b"old settings").unwrap();

        assert_eq!(restore_files(&[(original.clone(), backup.clone())]).unwrap_err().raw_os_error(), Some(libc::EEXIST));
        assert_eq!(fs::read(&original).unwrap(), b"new occupant");
        assert_eq!(fs::read(&backup).unwrap(), b"old settings");
    }
}
