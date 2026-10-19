//! The file-operation safety journal: one durable record per operation and
//! destination folder, written before the operation's first temporary file
//! there, so a later launch can remove leftovers of a crash. It is a SQLite
//! file of its own beside the settings database and shares no connection or
//! transaction with settings.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use dual_pane_domain::Location;
use rusqlite::{Connection, params};

use crate::native_calls;
use crate::native_location::location_from_path;

const SCHEMA_VERSION: i64 = 1;

/// The journal's file name, beside `settings.sqlite3`. Reset Settings moves
/// only the settings database and its exact sidecars, never this file.
pub const JOURNAL_FILE_NAME: &str = "operation-journal.sqlite3";

/// A random identity for one launch, carried in every temporary name so the
/// name positively identifies its creator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LaunchId([u8; 16]);

impl LaunchId {
    /// 16 bytes from `getentropy`. If that ever fails, the time and process
    /// stand in, which still tell launches apart.
    pub fn generate() -> Self {
        native_calls::random_bytes().map_or_else(
            |_| {
                let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
                let mut bytes = [0_u8; 16];
                bytes.copy_from_slice(&(nanos ^ (u128::from(std::process::id()) << 96)).to_le_bytes());
                Self(bytes)
            },
            Self,
        )
    }
    #[cfg(test)]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    pub fn hex(self) -> String {
        self.0.iter().fold(String::with_capacity(32), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
    }
}

const PREFIX: &str = ".dual-pane-";
const SUFFIX: &str = ".partial";

/// The temporary name `.dual-pane-<launch>-<operation>-<n>.partial`.
pub fn temporary_name(launch: LaunchId, operation: u64, n: u64) -> String {
    format!("{PREFIX}{}-{operation}-{n}{SUFFIX}", launch.hex())
}

/// The `<operation>-<n>` part of a temporary name of `launch`, if `name` is one.
fn temporary_parts(name: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let rest = name.strip_prefix(PREFIX.as_bytes())?.strip_suffix(SUFFIX.as_bytes())?;
    let mut parts = rest.split(|byte| *byte == b'-');
    let (launch, operation, n) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || launch.len() != 32 || !launch.iter().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)) {
        return None;
    }
    let digits = |part: &[u8]| !part.is_empty() && part.iter().all(u8::is_ascii_digit);
    (digits(operation) && digits(n)).then_some((launch, operation, n))
}

/// Whether `name` uses Dual Pane's exact reserved operation-temporary shape.
pub fn is_reserved_temporary(name: &[u8]) -> bool {
    temporary_parts(name).is_some()
}

/// Whether `name` is a temporary created during `launch`, which listings and
/// scans never show.
#[cfg(test)]
pub fn is_launch_temporary(name: &[u8], launch: LaunchId) -> bool {
    temporary_parts(name).is_some_and(|(found, _, _)| found == launch.hex().as_bytes())
}

/// Whether `name` is exactly a temporary of `operation` during the launch
/// whose identity is `launch_hex`.
pub fn is_operation_temporary(name: &[u8], launch_hex: &str, operation: u64) -> bool {
    temporary_parts(name).is_some_and(|(launch, found, _)| launch == launch_hex.as_bytes() && found == operation.to_string().as_bytes())
}

/// One recorded folder that may hold an operation's temporaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRecord {
    pub launch: String,
    pub operation: u64,
    pub folder: PathBuf,
    pub device: u64,
}

/// The journal database. One owner writes it at a time.
pub struct SafetyJournal {
    connection: Connection,
}

impl SafetyJournal {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        let connection = Connection::open(path)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS temporary_folder(launch TEXT NOT NULL, operation INTEGER NOT NULL, folder BLOB NOT NULL, device INTEGER NOT NULL, PRIMARY KEY(launch, operation, folder));")?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { connection })
    }
    /// Durably records that `operation` may create temporaries in `folder`.
    pub fn record(&self, record: &JournalRecord) -> Result<(), rusqlite::Error> {
        self.connection.execute("INSERT OR IGNORE INTO temporary_folder(launch, operation, folder, device) VALUES (?1, ?2, ?3, ?4)", params![record.launch, record.operation as i64, record.folder.as_os_str().as_bytes(), record.device as i64])?;
        Ok(())
    }
    pub fn records(&self, launch: &str, operation: u64) -> Result<Vec<JournalRecord>, rusqlite::Error> {
        self.query("SELECT launch, operation, folder, device FROM temporary_folder WHERE launch = ?1 AND operation = ?2", params![launch, operation as i64])
    }
    /// Records left by other launches.
    pub fn earlier_records(&self, launch: &str) -> Result<Vec<JournalRecord>, rusqlite::Error> {
        self.query("SELECT launch, operation, folder, device FROM temporary_folder WHERE launch <> ?1 ORDER BY launch, operation, folder", params![launch])
    }
    fn query(&self, sql: &str, parameters: impl rusqlite::Params) -> Result<Vec<JournalRecord>, rusqlite::Error> {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(parameters, |row| Ok(JournalRecord { launch: row.get(0)?, operation: row.get::<_, i64>(1)? as u64, folder: PathBuf::from(std::ffi::OsStr::from_bytes(&row.get::<_, Vec<u8>>(2)?)), device: row.get::<_, i64>(3)? as u64 }))?;
        rows.collect()
    }
    pub fn remove(&self, record: &JournalRecord) -> Result<(), rusqlite::Error> {
        self.connection.execute("DELETE FROM temporary_folder WHERE launch = ?1 AND operation = ?2 AND folder = ?3", params![record.launch, record.operation as i64, record.folder.as_os_str().as_bytes()])?;
        Ok(())
    }
}

/// The journal shared by the file-operation workers. The mutex makes it the
/// one serialized writer; only workers lock it, never the GUI thread.
#[derive(Clone)]
pub struct Journal {
    path: Option<PathBuf>,
    launch: LaunchId,
    database: Arc<Mutex<Option<SafetyJournal>>>,
    liveness: Arc<Mutex<Option<File>>>,
}

impl Journal {
    /// A journal at `path`, opened later by [`Self::reopen`]. `None` means no
    /// user location is known, so the journal stays unavailable.
    pub fn new(path: Option<PathBuf>, launch: LaunchId) -> Self {
        Self { path, launch, database: Arc::new(Mutex::new(None)), liveness: Arc::new(Mutex::new(None)) }
    }
    pub fn launch(&self) -> LaunchId {
        self.launch
    }
    fn lock(&self) -> MutexGuard<'_, Option<SafetyJournal>> {
        self.database.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    /// Opens the journal if it is closed and reports whether it is available.
    pub fn reopen(&self) -> bool {
        let mut database = self.lock();
        if database.is_none() {
            let Some(path) = self.path.as_deref() else { return false };
            let mut liveness = self.liveness.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if liveness.is_none() {
                *liveness = acquire_liveness(path, &self.launch.hex()).map_err(|error| log_liveness_error("acquire", &error)).ok();
            }
            if liveness.is_none() {
                return false;
            }
            *database = SafetyJournal::open(path).map_err(|error| log_journal_error("open", &error)).ok();
        }
        database.is_some()
    }
    pub fn is_available(&self) -> bool {
        self.lock().is_some()
    }
    /// Records `folder` for `operation` before its first temporary there.
    pub fn record(&self, operation: u64, folder: &Path, device: u64) -> io::Result<()> {
        let database = self.lock();
        let Some(database) = database.as_ref() else { return Err(io::Error::from(io::ErrorKind::NotConnected)) };
        database.record(&JournalRecord { launch: self.launch.hex(), operation, folder: folder.to_path_buf(), device }).map_err(|error| {
            log_journal_error("record", &error);
            io::Error::other("journal write failed")
        })
    }
    /// The folders recorded for `operation` during this launch.
    pub fn folders(&self, operation: u64) -> Vec<PathBuf> {
        self.lock().as_ref().and_then(|database| database.records(&self.launch.hex(), operation).ok()).unwrap_or_default().into_iter().map(|record| record.folder).collect()
    }
    /// Forgets `operation`'s records once no temporary of it remains.
    pub fn clear(&self, operation: u64) -> bool {
        let database = self.lock();
        let Some(database) = database.as_ref() else { return false };
        database.records(&self.launch.hex(), operation).and_then(|records| records.iter().try_for_each(|record| database.remove(record))).map_err(|error| log_journal_error("clear", &error)).is_ok()
    }
    /// Removes temporaries that earlier launches left behind and reports how
    /// many were removed and the folders where some could not be.
    pub fn sweep(&self) -> (usize, Vec<Location>) {
        let database = self.lock();
        let Some(database) = database.as_ref() else { return (0, vec![]) };
        let Ok(records) = database.earlier_records(&self.launch.hex()) else { return (0, vec![]) };
        let groups = records.into_iter().fold(BTreeMap::<String, Vec<JournalRecord>>::new(), |mut groups, record| {
            groups.entry(record.launch.clone()).or_default().push(record);
            groups
        });
        let recorded_launches = groups.keys().cloned().collect::<std::collections::BTreeSet<_>>();
        let Some(journal_path) = self.path.as_deref() else { return (0, vec![]) };
        let mut removed = 0;
        let mut failed = Vec::new();
        for (launch, records) in groups {
            let lock = match lock_dead_launch(journal_path, &launch) {
                Ok(Some(lock)) => lock,
                Ok(None) => continue,
                Err(_) => {
                    failed.extend(records.iter().filter_map(|record| location_from_path(&record.folder)));
                    continue;
                }
            };
            let mut group_complete = true;
            for record in &records {
                match sweep_folder(record) {
                    SweepOutcome::Done(count) => {
                        removed += count;
                        if database.remove(record).is_err() {
                            group_complete = false;
                        }
                    }
                    SweepOutcome::Unreachable => group_complete = false,
                    SweepOutcome::Failed(count) => {
                        removed += count;
                        group_complete = false;
                        failed.extend(location_from_path(&record.folder));
                    }
                }
            }
            if group_complete {
                let path = liveness_path(journal_path, &launch);
                if let Err(error) = fs::remove_file(&path)
                    && error.kind() != io::ErrorKind::NotFound
                {
                    log_liveness_error("remove", &error);
                }
            }
            drop(lock);
        }
        prune_orphan_liveness(journal_path, &self.launch.hex(), &recorded_launches);
        (removed, failed)
    }
}

fn liveness_path(journal: &Path, launch: &str) -> PathBuf {
    journal.with_file_name(format!("launch-{launch}.lock"))
}

fn validated_lock_file(path: &Path, create: bool) -> io::Result<File> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(io::Error::other("liveness path is not a regular file"));
    }
    let file = OpenOptions::new().read(true).write(true).create(create).mode(0o600).open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("liveness path is not a regular file"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn acquire_liveness(journal: &Path, launch: &str) -> io::Result<File> {
    let path = liveness_path(journal, launch);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = validated_lock_file(&path, true)?;
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}

/// Returns a held lock when `launch` is dead, `None` while it is live, and an
/// error when its liveness cannot be established safely.
fn lock_dead_launch(journal: &Path, launch: &str) -> io::Result<Option<File>> {
    if launch.len() != 32 || !launch.as_bytes().iter().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)) {
        return Err(io::Error::other("invalid launch identity"));
    }
    let file = validated_lock_file(&liveness_path(journal, launch), false)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn prune_orphan_liveness(journal: &Path, current: &str, recorded: &std::collections::BTreeSet<String>) {
    let Some(parent) = journal.parent() else { return };
    let Ok(entries) = fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let bytes = name.as_bytes();
        let Some(launch) = bytes.strip_prefix(b"launch-").and_then(|bytes| bytes.strip_suffix(b".lock")).and_then(|bytes| std::str::from_utf8(bytes).ok()) else { continue };
        if launch == current || recorded.contains(launch) {
            continue;
        }
        if let Ok(Some(lock)) = lock_dead_launch(journal, launch) {
            let _ = fs::remove_file(entry.path());
            drop(lock);
        }
    }
}

enum SweepOutcome {
    /// Every matching temporary is gone; the record can be dropped.
    Done(usize),
    /// The folder's volume is not mounted, so the record waits for a later
    /// launch without a Notice.
    Unreachable,
    /// Some temporaries could not be removed; the record stays.
    Failed(usize),
}

/// Removes the regular files and links in a recorded folder whose names match
/// the record's exact launch and operation. Links are never followed and
/// folders are never touched.
fn sweep_folder(record: &JournalRecord) -> SweepOutcome {
    match fs::symlink_metadata(&record.folder) {
        Ok(metadata) if metadata.is_dir() && metadata.dev() == record.device => {}
        Ok(_) => return SweepOutcome::Unreachable,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // The folder is gone if its volume is still there; otherwise the
            // volume is unmounted and the record waits.
            let device = record.folder.ancestors().skip(1).find_map(|ancestor| fs::symlink_metadata(ancestor).ok()).map(|metadata| metadata.dev());
            return if device == Some(record.device) { SweepOutcome::Done(0) } else { SweepOutcome::Unreachable };
        }
        Err(_) => return SweepOutcome::Failed(0),
    }
    let Ok(entries) = fs::read_dir(&record.folder) else { return SweepOutcome::Failed(0) };
    let mut removed = 0;
    let mut failed = false;
    for entry in entries {
        let Ok(entry) = entry else {
            failed = true;
            continue;
        };
        if !is_operation_temporary(entry.file_name().as_bytes(), &record.launch, record.operation) {
            continue;
        }
        let path = entry.path();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => match fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => failed = true,
            },
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => failed = true,
        }
    }
    if failed { SweepOutcome::Failed(removed) } else { SweepOutcome::Done(removed) }
}

/// The journal beside the settings database at `settings`.
pub fn journal_path(settings: &Path) -> PathBuf {
    settings.with_file_name(JOURNAL_FILE_NAME)
}

fn log_journal_error(action: &str, error: &rusqlite::Error) {
    eprintln!("Dual Pane safety journal {action} failed: {error}");
}

fn log_liveness_error(action: &str, error: &io::Error) {
    eprintln!("Dual Pane safety journal liveness {action} failed: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(byte: u8) -> LaunchId {
        LaunchId::from_bytes([byte; 16])
    }

    #[test]
    fn temporary_names_identify_their_launch_and_operation_exactly() {
        let current = launch(0xab);
        let name = temporary_name(current, 7, 3);
        assert_eq!(name, format!(".dual-pane-{}-7-3.partial", "ab".repeat(16)));
        assert!(is_launch_temporary(name.as_bytes(), current));
        assert!(is_reserved_temporary(name.as_bytes()));
        assert!(!is_launch_temporary(name.as_bytes(), launch(1)));
        assert!(is_operation_temporary(name.as_bytes(), &current.hex(), 7));
        assert!(!is_operation_temporary(name.as_bytes(), &current.hex(), 70));
        for near in [format!(".dual-pane-{}-7-.partial", current.hex()), format!(".dual-pane-{}-7-3.partial.txt", current.hex()), format!(".dual-pane-{}-7-3", current.hex()), format!("x.dual-pane-{}-7-3.partial", current.hex()), format!(".dual-pane-{}-7-a.partial", current.hex())] {
            assert!(!is_launch_temporary(near.as_bytes(), current), "{near}");
            assert!(!is_reserved_temporary(near.as_bytes()), "{near}");
        }
        assert!(!is_reserved_temporary(format!(".dual-pane-{}-7-3.partial", "AB".repeat(16)).as_bytes()));
    }

    #[test]
    fn the_sweep_removes_only_exact_leftovers_of_earlier_launches() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("target");
        fs::create_dir(&folder).unwrap();
        let (earlier, current) = (launch(1), launch(2));
        let leftover = folder.join(temporary_name(earlier, 4, 0));
        let leftover_link = folder.join(temporary_name(earlier, 4, 1));
        let other_operation = folder.join(temporary_name(earlier, 5, 0));
        let current_temporary = folder.join(temporary_name(current, 4, 0));
        let leftover_folder = folder.join(temporary_name(earlier, 4, 2));
        let outside = directory.path().join("outside");
        fs::write(&leftover, b"partial").unwrap();
        fs::write(&outside, b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, &leftover_link).unwrap();
        fs::write(&other_operation, b"other").unwrap();
        fs::write(&current_temporary, b"mine").unwrap();
        fs::create_dir(&leftover_folder).unwrap();
        fs::write(folder.join("user-file.partial"), b"keep").unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let device = fs::metadata(&folder).unwrap().dev();
        let earlier_journal = Journal::new(Some(path.clone()), earlier);
        assert!(earlier_journal.reopen());
        earlier_journal.record(4, &folder, device).unwrap();
        drop(earlier_journal);

        let journal = Journal::new(Some(path), current);
        assert!(journal.reopen());
        journal.record(9, &folder, device).unwrap();
        assert_eq!(journal.sweep(), (2, vec![]));
        assert!(!leftover.exists() && fs::symlink_metadata(&leftover_link).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"keep", "a link is removed, never followed");
        assert!(other_operation.exists() && current_temporary.exists() && leftover_folder.is_dir() && folder.join("user-file.partial").exists());
        assert_eq!(journal.sweep(), (0, vec![]), "the swept record is gone");
        assert_eq!(journal.folders(9), vec![folder], "this launch's records stay");
    }

    #[test]
    fn missing_folders_drop_their_record_and_unreachable_volumes_keep_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let earlier = Journal::new(Some(path.clone()), launch(1));
        assert!(earlier.reopen());
        let device = fs::metadata(directory.path()).unwrap().dev();
        earlier.record(1, &directory.path().join("deleted"), device).unwrap();
        earlier.record(2, &directory.path().join("unmounted"), device.wrapping_add(1)).unwrap();
        drop(earlier);
        let journal = Journal::new(Some(path.clone()), launch(2));
        assert!(journal.reopen());
        assert_eq!(journal.sweep(), (0, vec![]));
        let remaining = SafetyJournal::open(&path).unwrap().earlier_records(&launch(2).hex()).unwrap();
        assert_eq!(remaining.iter().map(|record| record.operation).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_live_launch_is_skipped_until_its_lock_is_released() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let folder = directory.path().join("target");
        fs::create_dir(&folder).unwrap();
        let earlier = Journal::new(Some(path.clone()), launch(1));
        assert!(earlier.reopen());
        earlier.record(7, &folder, fs::metadata(&folder).unwrap().dev()).unwrap();
        let temporary = folder.join(temporary_name(launch(1), 7, 0));
        fs::write(&temporary, b"live").unwrap();

        let current = Journal::new(Some(path), launch(2));
        assert!(current.reopen());
        assert_eq!(current.sweep(), (0, vec![]));
        assert!(temporary.exists());
        drop(earlier);
        assert_eq!(current.sweep(), (1, vec![]));
        assert!(!temporary.exists());
    }

    #[test]
    fn a_record_without_a_trustworthy_liveness_file_is_retained() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let folder = directory.path().join("target");
        fs::create_dir(&folder).unwrap();
        SafetyJournal::open(&path).unwrap().record(&JournalRecord { launch: launch(1).hex(), operation: 4, folder: folder.clone(), device: fs::metadata(&folder).unwrap().dev() }).unwrap();
        let temporary = folder.join(temporary_name(launch(1), 4, 0));
        fs::write(&temporary, b"unknown").unwrap();
        let current = Journal::new(Some(path.clone()), launch(2));
        assert!(current.reopen());
        assert_eq!(current.sweep(), (0, vec![location_from_path(&folder).unwrap()]));
        assert!(temporary.exists());
        assert_eq!(SafetyJournal::open(&path).unwrap().earlier_records(&launch(2).hex()).unwrap().len(), 1);
    }

    #[test]
    fn malformed_symlinked_and_unreadable_liveness_paths_are_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let journal = directory.path().join(JOURNAL_FILE_NAME);
        let launch = launch(1).hex();
        let path = liveness_path(&journal, &launch);

        fs::create_dir(&path).unwrap();
        assert!(lock_dead_launch(&journal, &launch).is_err());
        fs::remove_dir(&path).unwrap();

        let target = directory.path().join("target");
        fs::write(&target, b"").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(lock_dead_launch(&journal, &launch).is_err());
        fs::remove_file(&path).unwrap();

        fs::write(&path, b"").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = lock_dead_launch(&journal, &launch);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        if OpenOptions::new().read(true).write(true).open(&path).is_err() {
            assert!(unreadable.is_err());
        }

        assert!(lock_dead_launch(&journal, "not-a-launch-id").is_err());
    }

    #[test]
    fn an_unlocked_liveness_file_without_records_is_pruned() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let earlier = Journal::new(Some(path.clone()), launch(1));
        assert!(earlier.reopen());
        let orphan = liveness_path(&path, &launch(1).hex());
        assert!(orphan.exists());
        drop(earlier);
        let current = Journal::new(Some(path), launch(2));
        assert!(current.reopen());
        assert_eq!(current.sweep(), (0, vec![]));
        assert!(!orphan.exists());
    }

    #[test]
    fn a_folder_whose_leftovers_cannot_be_removed_is_reported_and_kept() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("locked");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join(temporary_name(launch(1), 3, 0)), b"partial").unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let earlier = Journal::new(Some(path.clone()), launch(1));
        assert!(earlier.reopen());
        earlier.record(3, &folder, fs::metadata(&folder).unwrap().dev()).unwrap();
        drop(earlier);
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let journal = Journal::new(Some(path), launch(2));
        assert!(journal.reopen());
        let swept = journal.sweep();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(swept, (0, vec![location_from_path(&folder).unwrap()]));
        assert_eq!(journal.sweep(), (1, vec![]), "the record stayed, so a later sweep removes the leftover");
    }

    #[test]
    fn an_unopenable_journal_is_unavailable_until_it_reopens() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("blocker");
        fs::write(&blocker, b"not a folder").unwrap();
        let journal = Journal::new(Some(blocker.join(JOURNAL_FILE_NAME)), launch(1));
        assert!(!journal.reopen());
        assert!(journal.record(1, directory.path(), 0).is_err());
        assert!(!Journal::new(None, launch(1)).reopen());
    }

    #[test]
    fn the_journal_survives_a_crash_between_record_and_clear() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(JOURNAL_FILE_NAME);
        let folder = directory.path().join("target");
        fs::create_dir(&folder).unwrap();
        let journal = Journal::new(Some(path.clone()), launch(1));
        assert!(journal.reopen());
        journal.record(6, &folder, fs::metadata(&folder).unwrap().dev()).unwrap();
        fs::write(folder.join(temporary_name(launch(1), 6, 0)), b"partial").unwrap();
        // A crash releases the kernel lock but leaves both durable files.
        drop(journal);
        let next = Journal::new(Some(path), launch(2));
        assert!(next.reopen());
        assert_eq!(next.sweep(), (1, vec![]));
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
    }
}
