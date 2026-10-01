//! SQLite-backed settings driver. This module owns paths, SQLite, and worker
//! scheduling; it does not know about Qt objects or widgets.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dual_pane_application::{ActionBinding, ActionId, FavoriteGroupRecord, FavoriteItemRecord, FavoritesRecords, Key, SettingsFailure, SettingsSnapshot, Shortcut};
use dual_pane_domain::{EntryName, Location, SortDirection, SortField, SortSpec};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};

const SCHEMA_VERSION: i64 = 2;
const SAVE_DEBOUNCE: Duration = Duration::from_millis(50);
type BindingParameters = (&'static str, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<String>);

#[derive(Debug)]
pub enum SettingsStorageError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    UnsupportedSchema(i64),
    InvalidData(&'static str),
}
impl std::fmt::Display for SettingsStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "settings I/O failed: {error}"),
            Self::Sql(error) => write!(f, "settings database failed: {error}"),
            Self::UnsupportedSchema(version) => write!(f, "settings database schema {version} is newer than supported"),
            Self::InvalidData(reason) => write!(f, "settings database contains invalid data: {reason}"),
        }
    }
}
impl std::error::Error for SettingsStorageError {}
impl SettingsStorageError {
    /// The application-facing category of this failure.
    pub fn failure(&self) -> SettingsFailure {
        match self {
            Self::Io(_) => SettingsFailure::Unavailable,
            Self::UnsupportedSchema(_) => SettingsFailure::UnsupportedSchema,
            Self::InvalidData(_) => SettingsFailure::Corrupt,
            Self::Sql(error) => match error.sqlite_error_code() {
                Some(ErrorCode::CannotOpen | ErrorCode::PermissionDenied | ErrorCode::ReadOnly | ErrorCode::DiskFull | ErrorCode::SystemIoFailure | ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked | ErrorCode::FileLockingProtocolFailed | ErrorCode::OutOfMemory | ErrorCode::NoLargeFileSupport) => SettingsFailure::Unavailable,
                _ => SettingsFailure::Corrupt,
            },
        }
    }
}
impl From<std::io::Error> for SettingsStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<rusqlite::Error> for SettingsStorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

pub struct SettingsDatabase {
    connection: Connection,
}

impl SettingsDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SettingsStorageError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut connection)?;
        preload_bindings(&mut connection)?;
        Ok(Self { connection })
    }
    pub fn load(&self) -> Result<SettingsSnapshot, SettingsStorageError> {
        let bindings = ActionId::ALL
            .into_iter()
            .map(|action| {
                self.connection
                    .query_row("SELECT command, shift, option, control, key FROM action_binding WHERE action = ?1", [action.as_str()], |row| {
                        let command = row.get::<_, Option<i64>>(0)?.unwrap_or_default() != 0;
                        let shift = row.get::<_, Option<i64>>(1)?.unwrap_or_default() != 0;
                        let option = row.get::<_, Option<i64>>(2)?.unwrap_or_default() != 0;
                        let control = row.get::<_, Option<i64>>(3)?.unwrap_or_default() != 0;
                        // NULL is a deliberately unbound action. Unknown key text
                        // drops only this binding, so the action uses its default.
                        Ok(match row.get::<_, Option<String>>(4)? {
                            None => Some(ActionBinding { action, shortcut: None }),
                            Some(text) => Key::parse(&text).map(|key| ActionBinding { action, shortcut: Some(Shortcut::new(command, shift, option, control, key)) }),
                        })
                    })
                    .optional()
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        let mut statement = self.connection.prepare("SELECT target, field, direction FROM folder_sort ORDER BY recent ASC")?;
        let folder_sorts = statement.query_map([], |row| Ok((decode_location(&row.get::<_, Vec<u8>>(0)?).map_err(to_sql_error)?, decode_sort(row.get(1)?, row.get(2)?).map_err(to_sql_error)?)))?.collect::<Result<Vec<_>, _>>()?;
        let initialized = self.connection.query_row("SELECT value FROM setting_marker WHERE key = 'favorites_initialized'", [], |row| row.get::<_, i64>(0)).optional()?.unwrap_or(0) != 0;
        let groups = self.connection.prepare("SELECT id, name, position FROM favorite_group ORDER BY position, id")?.query_map([], |row| Ok(FavoriteGroupRecord { id: row.get(0)?, name: row.get(1)?, position: row.get(2)? }))?.collect::<Result<Vec<_>, _>>()?;
        let items = self.connection.prepare("SELECT id, group_id, name, target, position FROM favorite_item ORDER BY group_id, position, id")?.query_map([], |row| Ok(FavoriteItemRecord { id: row.get(0)?, group_id: row.get(1)?, name: row.get(2)?, target: decode_location(&row.get::<_, Vec<u8>>(3)?).map_err(to_sql_error)?, position: row.get(4)? }))?.collect::<Result<Vec<_>, _>>()?;
        let favorites = FavoritesRecords { initialized, groups, items };
        favorites.hierarchy().map_err(|_| SettingsStorageError::InvalidData("Favorites hierarchy"))?;
        Ok(SettingsSnapshot { bindings, folder_sorts, favorites })
    }
    pub fn save(&mut self, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
        snapshot.favorites.hierarchy().map_err(|_| SettingsStorageError::InvalidData("Favorites hierarchy"))?;
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM action_binding", [])?;
        for binding in dual_pane_application::default_bindings() {
            insert_binding(&tx, &binding)?;
        }
        for binding in &snapshot.bindings {
            tx.execute("INSERT OR REPLACE INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", binding_params(binding))?;
        }
        tx.execute("DELETE FROM folder_sort", [])?;
        for (recent, (location, sort)) in snapshot.folder_sorts.iter().enumerate() {
            tx.execute("INSERT INTO folder_sort(target, field, direction, recent) VALUES (?1, ?2, ?3, ?4)", params![encode_location(location), sort_field(sort.field()), sort_direction(sort.direction()), recent as i64])?;
        }
        tx.execute("DELETE FROM favorite_item", [])?;
        tx.execute("DELETE FROM favorite_group", [])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('favorites_initialized', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.favorites.initialized)])?;
        for group in &snapshot.favorites.groups {
            tx.execute("INSERT INTO favorite_group(id, name, position) VALUES (?1, ?2, ?3)", params![group.id, group.name, group.position])?;
        }
        for item in &snapshot.favorites.items {
            tx.execute("INSERT INTO favorite_item(id, group_id, name, target, position) VALUES (?1, ?2, ?3, ?4, ?5)", params![item.id, item.group_id, item.name, encode_location(&item.target), item.position])?;
        }
        tx.commit()?;
        Ok(())
    }
}

pub fn application_support_database_path(home: &Path) -> PathBuf {
    home.join("Library").join("Application Support").join("Dual Pane").join("settings.sqlite3")
}

fn migrate(connection: &mut Connection) -> Result<(), SettingsStorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(SettingsStorageError::UnsupportedSchema(version));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    let tx = connection.transaction()?;
    if version < 1 {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS action_binding(action TEXT PRIMARY KEY NOT NULL, command INTEGER, shift INTEGER, option INTEGER, control INTEGER, key TEXT); CREATE TABLE IF NOT EXISTS folder_sort(target BLOB PRIMARY KEY NOT NULL, field INTEGER NOT NULL, direction INTEGER NOT NULL, recent INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS folder_sort_recent ON folder_sort(recent); CREATE TABLE IF NOT EXISTS setting_marker(key TEXT PRIMARY KEY NOT NULL, value INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS favorite_group(id INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, position INTEGER NOT NULL); CREATE UNIQUE INDEX IF NOT EXISTS favorite_group_position ON favorite_group(position); CREATE TABLE IF NOT EXISTS favorite_item(id INTEGER PRIMARY KEY NOT NULL, group_id INTEGER NOT NULL REFERENCES favorite_group(id) ON DELETE RESTRICT, name TEXT NOT NULL, target BLOB NOT NULL, position INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS favorite_item_group ON favorite_item(group_id, position);")?;
    }
    if version < 2 {
        // Version 1 stored NavigateParent's default as a plain L key because
        // it could not name arrow keys; its intended shortcut is Command+Up.
        tx.execute("UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'Up' WHERE action = 'NavigateParent' AND IFNULL(command, 0) = 0 AND IFNULL(shift, 0) = 0 AND IFNULL(option, 0) = 0 AND IFNULL(control, 0) = 0 AND key = 'L'", [])?;
        // Version 1 read a NULL key as "use the default"; version 2 reads it as
        // "unbound". No version 1 interface could unbind, so restore defaults.
        for binding in dual_pane_application::default_bindings().iter().filter(|binding| binding.shortcut.is_some()) {
            tx.execute("UPDATE action_binding SET command = ?2, shift = ?3, option = ?4, control = ?5, key = ?6 WHERE action = ?1 AND key IS NULL", binding_params(binding))?;
        }
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
fn preload_bindings(connection: &mut Connection) -> Result<(), SettingsStorageError> {
    let tx = connection.transaction()?;
    for binding in dual_pane_application::default_bindings() {
        tx.execute("INSERT OR IGNORE INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", binding_params(&binding))?;
    }
    tx.commit()?;
    Ok(())
}
fn binding_params(binding: &ActionBinding) -> BindingParameters {
    let shortcut = binding.shortcut;
    (binding.action.as_str(), shortcut.map(|value| i64::from(value.command)), shortcut.map(|value| i64::from(value.shift)), shortcut.map(|value| i64::from(value.option)), shortcut.map(|value| i64::from(value.control)), shortcut.map(|value| value.key.as_text()))
}
fn insert_binding(tx: &Transaction<'_>, binding: &ActionBinding) -> Result<(), rusqlite::Error> {
    tx.execute("INSERT INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", binding_params(binding))?;
    Ok(())
}
fn sort_field(field: SortField) -> i64 {
    match field {
        SortField::Name => 0,
        SortField::Type => 1,
        SortField::Modified => 2,
        SortField::Size => 3,
    }
}
fn sort_direction(direction: SortDirection) -> i64 {
    match direction {
        SortDirection::Ascending => 0,
        SortDirection::Descending => 1,
    }
}
fn decode_sort(field: i64, direction: i64) -> Result<SortSpec, SettingsStorageError> {
    let field = match field {
        0 => SortField::Name,
        1 => SortField::Type,
        2 => SortField::Modified,
        3 => SortField::Size,
        _ => return Err(SettingsStorageError::InvalidData("sort field")),
    };
    let direction = match direction {
        0 => SortDirection::Ascending,
        1 => SortDirection::Descending,
        _ => return Err(SettingsStorageError::InvalidData("sort direction")),
    };
    Ok(SortSpec::new(field, direction))
}
fn to_sql_error(error: SettingsStorageError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

/// Prefix each logical component with its big-endian length so keys are
/// unambiguous and preserve every non-UTF-8 byte.
fn encode_location(location: &Location) -> Vec<u8> {
    let mut bytes = Vec::new();
    for component in location.components() {
        bytes.extend_from_slice(&(component.as_bytes().len() as u32).to_be_bytes());
        bytes.extend_from_slice(component.as_bytes());
    }
    bytes
}
fn decode_location(bytes: &[u8]) -> Result<Location, SettingsStorageError> {
    let mut components = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(length) = rest.get(..4) else {
            return Err(SettingsStorageError::InvalidData("location length"));
        };
        let length = u32::from_be_bytes(length.try_into().map_err(|_| SettingsStorageError::InvalidData("location length"))?) as usize;
        rest = &rest[4..];
        let Some(name) = rest.get(..length) else {
            return Err(SettingsStorageError::InvalidData("location bytes"));
        };
        components.push(EntryName::new(name.to_vec()).map_err(|_| SettingsStorageError::InvalidData("location component"))?);
        rest = &rest[length..];
    }
    Ok(Location::from_components(components))
}

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
fn reset_database_to(path: &Path, current: Option<SettingsDatabase>, backup: &Path) -> (Option<SettingsDatabase>, Result<Option<PathBuf>, SettingsStorageError>) {
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
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut text = path.as_os_str().to_owned();
    text.push(suffix);
    PathBuf::from(text)
}
/// Moves the database and its sidecars to `backup`. If any move fails, the
/// files already moved are returned before the error is reported.
fn preserve_files(path: &Path, backup: &Path) -> Result<bool, SettingsStorageError> {
    let mut moved = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let (from, to) = (with_suffix(path, suffix), with_suffix(backup, suffix));
        if fs::symlink_metadata(&from).is_err() {
            continue;
        }
        if let Err(error) = fs::rename(&from, &to) {
            for (from, to) in moved.iter().rev() {
                // Best effort: the original error is the one worth reporting.
                fs::rename(to, from).ok();
            }
            return Err(error.into());
        }
        moved.push((from, to));
    }
    Ok(moved.iter().any(|(from, _)| from == path))
}

pub enum SettingsJob {
    Load,
    Save {
        revision: u64,
        snapshot: SettingsSnapshot,
    },
    /// Saves this snapshot without waiting for more changes, then stops.
    Quit {
        revision: u64,
        snapshot: SettingsSnapshot,
    },
    Reset,
}
pub enum SettingsResult {
    Loaded(SettingsSnapshot),
    LoadFailed(SettingsStorageError),
    Saved { revision: u64 },
    SaveFailed { revision: u64, error: SettingsStorageError },
    Reset { backup: Option<PathBuf> },
    ResetFailed(SettingsStorageError),
}
/// One ordered worker. Replaceable saves are coalesced before I/O; quit always
/// flushes the most recent snapshot without blocking the GUI caller.
pub struct SettingsWorker {
    jobs: Sender<SettingsJob>,
    results: Receiver<SettingsResult>,
}
impl SettingsWorker {
    pub fn start(path: PathBuf) -> Result<Self, std::io::Error> {
        Self::start_with_wake(path, Box::new(|| {}))
    }
    pub fn start_with_wake(path: PathBuf, wake: Box<dyn Fn() + Send>) -> Result<Self, std::io::Error> {
        let (jobs, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        thread::Builder::new().name("settings-worker".to_owned()).spawn(move || worker_loop(WorkerState { path, database: None }, receiver, sender, wake))?;
        Ok(Self { jobs, results })
    }
    pub fn submit(&self, job: SettingsJob) -> bool {
        self.jobs.send(job).is_ok()
    }
    pub fn take_results(&self) -> Vec<SettingsResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.results.try_recv() {
            results.push(result);
        }
        results
    }
    /// Flushes queued saves and `final_save`, then waits at most `timeout` for
    /// the worker to stop. A worker that is still busy is left to finish
    /// during process teardown. Returns the results delivered while waiting.
    pub fn shutdown(self, final_save: Option<(u64, SettingsSnapshot)>, timeout: Duration) -> Vec<SettingsResult> {
        let Self { jobs, results } = self;
        if let Some((revision, snapshot)) = final_save {
            jobs.send(SettingsJob::Quit { revision, snapshot }).ok();
        }
        drop(jobs);
        let deadline = Instant::now() + timeout;
        let mut delivered = Vec::new();
        while let Ok(result) = results.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            delivered.push(result);
        }
        delivered
    }
}
struct WorkerState {
    path: PathBuf,
    /// Opened on first use and after a reset; `None` after a failed open.
    database: Option<SettingsDatabase>,
}
impl WorkerState {
    fn database(&mut self) -> Result<&mut SettingsDatabase, SettingsStorageError> {
        let database = match self.database.take() {
            Some(database) => database,
            None => SettingsDatabase::open(&self.path)?,
        };
        Ok(self.database.insert(database))
    }
    fn save(&mut self, revision: u64, snapshot: &SettingsSnapshot) -> SettingsResult {
        self.database().and_then(|database| database.save(snapshot)).map_or_else(|error| SettingsResult::SaveFailed { revision, error }, |()| SettingsResult::Saved { revision })
    }
}
fn worker_loop(mut state: WorkerState, jobs: Receiver<SettingsJob>, results: Sender<SettingsResult>, wake: Box<dyn Fn() + Send>) {
    let send = |result| {
        results.send(result).ok();
        wake();
    };
    while let Ok(job) = jobs.recv() {
        let mut latest = None;
        let mut quitting = false;
        let mut next = Some(job);
        while let Some(job) = next.take() {
            match job {
                SettingsJob::Load => {
                    // A load reports what storage holds, so a save requested
                    // before it must reach storage first.
                    if let Some((revision, snapshot)) = latest.take() {
                        send(state.save(revision, &snapshot));
                    }
                    send(state.database().and_then(|database| database.load()).map_or_else(SettingsResult::LoadFailed, SettingsResult::Loaded));
                }
                SettingsJob::Save { revision, snapshot } => latest = Some((revision, snapshot)),
                SettingsJob::Quit { revision, snapshot } => {
                    latest = Some((revision, snapshot));
                    quitting = true;
                }
                SettingsJob::Reset => {
                    let (database, result) = reset_database(&state.path, state.database.take());
                    state.database = database;
                    match result {
                        Ok(backup) => {
                            // A confirmed reset supersedes unsaved replaceable state
                            // that was queued before it.
                            latest = None;
                            send(SettingsResult::Reset { backup });
                        }
                        Err(error) => send(SettingsResult::ResetFailed(error)),
                    }
                }
            }
            if !quitting {
                next = jobs.recv_timeout(SAVE_DEBOUNCE).ok();
            }
        }
        if let Some((revision, snapshot)) = latest {
            send(state.save(revision, &snapshot));
        }
        if quitting {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_application::SettingsState;
    use tempfile::{TempDir, tempdir};
    /// A database path inside a temporary directory that lives as long as the
    /// returned guard.
    fn temp_database() -> (TempDir, PathBuf) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.sqlite3");
        (directory, path)
    }
    fn wait_for_results(worker: &SettingsWorker) -> Vec<SettingsResult> {
        std::iter::repeat_with(|| worker.take_results()).find(|items| !items.is_empty()).unwrap()
    }
    fn non_utf8_location() -> Location {
        Location::root().join(&EntryName::new(b"caf\xFF".to_vec()).unwrap())
    }
    #[test]
    fn preloads_each_action_and_round_trips_non_utf8_and_empty_favorites() {
        let (_directory, path) = temp_database();
        let mut db = SettingsDatabase::open(&path).unwrap();
        let mut snapshot = SettingsState::new().snapshot();
        snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Size, SortDirection::Descending)));
        snapshot.favorites.initialized = true;
        db.save(&snapshot).unwrap();
        let loaded = db.load().unwrap();
        assert_eq!(loaded.bindings.len(), ActionId::ALL.len());
        assert_eq!(loaded.folder_sorts, snapshot.folder_sorts);
        assert!(loaded.favorites.initialized);
        assert!(loaded.favorites.groups.is_empty());
    }
    #[test]
    fn favorites_round_trip_in_order() {
        let (_directory, path) = temp_database();
        let mut db = SettingsDatabase::open(&path).unwrap();
        let snapshot = SettingsSnapshot { bindings: dual_pane_application::default_bindings(), folder_sorts: vec![], favorites: FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "A".into(), position: 0 }, FavoriteGroupRecord { id: 2, name: "B".into(), position: 1 }], items: vec![FavoriteItemRecord { id: 4, group_id: 1, name: "item".into(), target: non_utf8_location(), position: 0 }] } };
        db.save(&snapshot).unwrap();
        assert_eq!(db.load().unwrap().favorites, snapshot.favorites);
    }
    #[test]
    fn invalid_favorites_are_rejected_before_save_and_on_load() {
        let (_directory, path) = temp_database();
        let mut db = SettingsDatabase::open(&path).unwrap();
        let mut snapshot = SettingsState::new().snapshot();
        snapshot.favorites.initialized = true;
        snapshot.favorites.groups.push(FavoriteGroupRecord { id: 1, name: " ".into(), position: 0 });
        assert!(matches!(db.save(&snapshot), Err(SettingsStorageError::InvalidData("Favorites hierarchy"))));
        assert!(db.load().unwrap().favorites.groups.is_empty());

        db.connection.execute("INSERT INTO favorite_group(id, name, position) VALUES (1, ' ', 0)", []).unwrap();
        assert!(matches!(db.load(), Err(SettingsStorageError::InvalidData("Favorites hierarchy"))));
    }
    #[test]
    fn supported_old_schema_migrates_transactionally() {
        let (_directory, path) = temp_database();
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 0).unwrap();
        drop(connection);
        let db = SettingsDatabase::open(&path).unwrap();
        assert_eq!(db.load().unwrap().bindings.len(), ActionId::ALL.len());
    }
    #[test]
    fn newer_schema_and_failed_open_are_reported_without_reset() {
        let (_directory, path) = temp_database();
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
        drop(connection);
        assert!(matches!(SettingsDatabase::open(&path), Err(SettingsStorageError::UnsupportedSchema(_))));
        let directory = tempdir().unwrap();
        let file = directory.path().join("not-a-directory");
        fs::write(&file, []).unwrap();
        assert!(matches!(SettingsDatabase::open(file.join("settings.sqlite3")), Err(SettingsStorageError::Io(_))));
    }
    #[test]
    fn malformed_binding_is_reported_and_partial_snapshots_keep_the_catalogue() {
        let (_directory, path) = temp_database();
        let mut db = SettingsDatabase::open(&path).unwrap();
        db.save(&SettingsSnapshot::default()).unwrap();
        assert_eq!(db.load().unwrap().bindings.len(), ActionId::ALL.len());
        db.connection.execute("UPDATE action_binding SET command = 'not-an-integer' WHERE action = 'NewFolder'", []).unwrap();
        assert!(matches!(db.load(), Err(SettingsStorageError::Sql(_))));
    }
    #[test]
    fn explicit_reset_preserves_failed_database() {
        let (_directory, path) = temp_database();
        let db = SettingsDatabase::open(&path).unwrap();
        let (database, result) = reset_database(&path, Some(db));
        let backup = result.unwrap().unwrap();
        assert!(backup.exists());
        assert!(database.is_some());
        assert!(SettingsDatabase::open(&path).is_ok());
    }
    #[test]
    fn worker_delivers_without_calling_on_submit_thread() {
        let (_directory, path) = temp_database();
        let worker = SettingsWorker::start(path).unwrap();
        assert!(worker.submit(SettingsJob::Save { revision: 1, snapshot: SettingsState::new().snapshot() }));
        assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Saved { revision: 1 }]));
    }
    #[test]
    fn worker_remains_available_after_reset() {
        let (_directory, path) = temp_database();
        let worker = SettingsWorker::start(path).unwrap();
        assert!(worker.submit(SettingsJob::Reset));
        assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Reset { .. }]));
        assert!(worker.submit(SettingsJob::Save { revision: 2, snapshot: SettingsState::new().snapshot() }));
        assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Saved { revision: 2 }]));
    }
    fn sorted_snapshot() -> SettingsSnapshot {
        let mut snapshot = SettingsState::new().snapshot();
        snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Type, SortDirection::Descending)));
        snapshot
    }
    #[test]
    fn a_load_after_a_queued_save_reports_the_saved_settings() {
        let (_directory, path) = temp_database();
        let worker = SettingsWorker::start(path).unwrap();
        let snapshot = sorted_snapshot();
        assert!(worker.submit(SettingsJob::Save { revision: 3, snapshot: snapshot.clone() }));
        assert!(worker.submit(SettingsJob::Load));
        let results = worker.shutdown(None, Duration::from_secs(10));
        assert!(matches!(results.as_slice(), [SettingsResult::Saved { revision: 3 }, SettingsResult::Loaded(loaded)] if loaded.folder_sorts == snapshot.folder_sorts));
    }
    #[test]
    fn a_reset_discards_saves_queued_before_it() {
        let (_directory, path) = temp_database();
        let worker = SettingsWorker::start(path).unwrap();
        assert!(worker.submit(SettingsJob::Save { revision: 3, snapshot: sorted_snapshot() }));
        assert!(worker.submit(SettingsJob::Reset));
        assert!(worker.submit(SettingsJob::Load));
        let results = worker.shutdown(None, Duration::from_secs(10));
        assert!(matches!(results.as_slice(), [SettingsResult::Reset { .. }, SettingsResult::Loaded(loaded)] if loaded.folder_sorts.is_empty()));
    }
    fn stored_sorts(path: &Path) -> Vec<(Location, SortSpec)> {
        SettingsDatabase::open(path).unwrap().load().unwrap().folder_sorts
    }
    #[test]
    fn failed_reset_restores_moved_files_and_later_saves_reach_disk() {
        let (directory, path) = temp_database();
        let backup = directory.path().join("settings.failed");
        let db = SettingsDatabase::open(&path).unwrap();
        drop(db);
        fs::write(with_suffix(&path, "-wal"), []).unwrap();
        // A directory where the WAL backup belongs makes that move fail after
        // the main database file has already moved.
        fs::create_dir(with_suffix(&backup, "-wal")).unwrap();
        let (database, result) = reset_database_to(&path, None, &backup);
        assert!(result.is_err());
        assert!(path.exists());
        assert!(!backup.exists());
        let mut database = database.expect("the original database reopens");
        let mut snapshot = SettingsState::new().snapshot();
        snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Size, SortDirection::Ascending)));
        database.save(&snapshot).unwrap();
        assert_eq!(stored_sorts(&path), snapshot.folder_sorts);
    }
    #[test]
    fn worker_reports_a_failed_open_by_category_and_stays_available() {
        let (_directory, path) = temp_database();
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
        drop(connection);
        let worker = SettingsWorker::start(path.clone()).unwrap();
        assert!(worker.submit(SettingsJob::Load));
        assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::LoadFailed(error)] if error.failure() == SettingsFailure::UnsupportedSchema));
        assert!(worker.submit(SettingsJob::Reset));
        assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Reset { backup: Some(_) }]));
        assert!(worker.submit(SettingsJob::Load));
        assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Loaded(_)]));
    }
    #[test]
    fn shutdown_flushes_the_final_snapshot_before_returning() {
        let (_directory, path) = temp_database();
        let worker = SettingsWorker::start(path.clone()).unwrap();
        let mut snapshot = SettingsState::new().snapshot();
        snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Modified, SortDirection::Descending)));
        let results = worker.shutdown(Some((9, snapshot.clone())), Duration::from_secs(10));
        assert!(results.iter().any(|result| matches!(result, SettingsResult::Saved { revision: 9 })));
        assert_eq!(stored_sorts(&path), snapshot.folder_sorts);
    }
    /// A version 1 database whose bindings are rewritten by `sql`.
    fn version_one_database(sql: &str) -> (TempDir, PathBuf) {
        let (directory, path) = temp_database();
        drop(SettingsDatabase::open(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(sql).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        (directory, path)
    }
    fn loaded_binding(path: &Path, action: ActionId) -> Option<Option<Shortcut>> {
        SettingsDatabase::open(path).unwrap().load().unwrap().bindings.into_iter().find(|binding| binding.action == action).map(|binding| binding.shortcut)
    }
    #[test]
    fn version_one_bindings_migrate_to_named_keys_and_explicit_defaults() {
        let (_directory, path) = version_one_database("UPDATE action_binding SET command = 0, shift = 0, option = 0, control = 0, key = 'L' WHERE action = 'NavigateParent'; UPDATE action_binding SET command = NULL, shift = NULL, option = NULL, control = NULL, key = NULL WHERE action = 'NewFolder';");
        assert_eq!(loaded_binding(&path, ActionId::NavigateParent), Some(dual_pane_application::default_shortcut(ActionId::NavigateParent)));
        assert_eq!(loaded_binding(&path, ActionId::NewFolder), Some(dual_pane_application::default_shortcut(ActionId::NewFolder)));
        assert_eq!(loaded_binding(&path, ActionId::SortByNameAscending), Some(None));
        let version: i64 = Connection::open(&path).unwrap().pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }
    #[test]
    fn version_one_migration_keeps_a_customized_navigate_parent() {
        let (_directory, path) = version_one_database("UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'P' WHERE action = 'NavigateParent';");
        assert_eq!(loaded_binding(&path, ActionId::NavigateParent), Some(Some(Shortcut::new(true, false, false, false, Key::Character('P')))));
    }
    #[test]
    fn unknown_key_text_drops_only_that_binding() {
        let (_directory, path) = temp_database();
        let database = SettingsDatabase::open(&path).unwrap();
        database.connection.execute("UPDATE action_binding SET key = 'Hyper' WHERE action = 'NewFolder'", []).unwrap();
        let loaded = database.load().unwrap();
        assert!(loaded.bindings.iter().all(|binding| binding.action != ActionId::NewFolder));
        let mut state = SettingsState::new();
        state.apply(loaded);
        assert_eq!(state.binding(ActionId::NewFolder), dual_pane_application::default_shortcut(ActionId::NewFolder));
    }
    #[test]
    fn an_unbound_action_round_trips_as_unbound() {
        let (_directory, path) = temp_database();
        let mut state = SettingsState::new();
        assert!(state.update_binding(ActionBinding { action: ActionId::QuitApplication, shortcut: None }));
        SettingsDatabase::open(&path).unwrap().save(&state.snapshot()).unwrap();
        assert_eq!(loaded_binding(&path, ActionId::QuitApplication), Some(None));
    }
}
