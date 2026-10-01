//! SQLite-backed settings driver. This module owns paths, SQLite, and worker
//! scheduling; it does not know about Qt objects or widgets.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dual_pane_application::{ActionBinding, ActionId, FavoriteGroupRecord, FavoriteItemRecord, FavoritesRecords, SettingsSnapshot};
use dual_pane_domain::{EntryName, Location, SortDirection, SortField, SortSpec};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

const SCHEMA_VERSION: i64 = 1;
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
    path: PathBuf,
    connection: Connection,
}

impl SettingsDatabase {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, SettingsStorageError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut connection)?;
        preload_bindings(&mut connection)?;
        Ok(Self { path, connection })
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
                        let key: Option<String> = row.get(4)?;
                        Ok(ActionBinding { action, shortcut: key.and_then(|key| key.chars().next()).map(|key| dual_pane_application::Shortcut::new(command, shift, option, control, key)) })
                    })
                    .optional()
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let mut statement = self.connection.prepare("SELECT target, field, direction FROM folder_sort ORDER BY recent ASC")?;
        let folder_sorts = statement.query_map([], |row| Ok((decode_location(&row.get::<_, Vec<u8>>(0)?).map_err(to_sql_error)?, decode_sort(row.get(1)?, row.get(2)?).map_err(to_sql_error)?)))?.collect::<Result<Vec<_>, _>>()?;
        let initialized = self.connection.query_row("SELECT value FROM setting_marker WHERE key = 'favorites_initialized'", [], |row| row.get::<_, i64>(0)).optional()?.unwrap_or(0) != 0;
        let groups = self.connection.prepare("SELECT id, name, position FROM favorite_group ORDER BY position, id")?.query_map([], |row| Ok(FavoriteGroupRecord { id: row.get(0)?, name: row.get(1)?, position: row.get(2)? }))?.collect::<Result<Vec<_>, _>>()?;
        let items = self.connection.prepare("SELECT id, group_id, name, target, position FROM favorite_item ORDER BY group_id, position, id")?.query_map([], |row| Ok(FavoriteItemRecord { id: row.get(0)?, group_id: row.get(1)?, name: row.get(2)?, target: decode_location(&row.get::<_, Vec<u8>>(3)?).map_err(to_sql_error)?, position: row.get(4)? }))?.collect::<Result<Vec<_>, _>>()?;
        Ok(SettingsSnapshot { bindings, folder_sorts, favorites: FavoritesRecords { initialized, groups, items } })
    }
    pub fn save(&mut self, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
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
    pub fn reset(&mut self) -> Result<PathBuf, SettingsStorageError> {
        let replacement = Connection::open_in_memory()?;
        let old_connection = std::mem::replace(&mut self.connection, replacement);
        drop(old_connection);
        let backup = self.path.with_extension(format!("failed-{}", SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| SettingsStorageError::InvalidData("clock before epoch"))?.as_nanos()));
        if self.path.exists() {
            fs::rename(&self.path, &backup)?;
        }
        for extension in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{}", self.path.display(), extension));
            if sidecar.exists() {
                fs::rename(&sidecar, PathBuf::from(format!("{}{}", backup.display(), extension)))?;
            }
        }
        self.connection = Self::open(&self.path)?.connection;
        Ok(backup)
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
    tx.execute_batch("CREATE TABLE IF NOT EXISTS action_binding(action TEXT PRIMARY KEY NOT NULL, command INTEGER, shift INTEGER, option INTEGER, control INTEGER, key TEXT); CREATE TABLE IF NOT EXISTS folder_sort(target BLOB PRIMARY KEY NOT NULL, field INTEGER NOT NULL, direction INTEGER NOT NULL, recent INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS folder_sort_recent ON folder_sort(recent); CREATE TABLE IF NOT EXISTS setting_marker(key TEXT PRIMARY KEY NOT NULL, value INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS favorite_group(id INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, position INTEGER NOT NULL); CREATE UNIQUE INDEX IF NOT EXISTS favorite_group_position ON favorite_group(position); CREATE TABLE IF NOT EXISTS favorite_item(id INTEGER PRIMARY KEY NOT NULL, group_id INTEGER NOT NULL REFERENCES favorite_group(id) ON DELETE RESTRICT, name TEXT NOT NULL, target BLOB NOT NULL, position INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS favorite_item_group ON favorite_item(group_id, position);")?;
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
    (binding.action.as_str(), shortcut.map(|value| i64::from(value.command)), shortcut.map(|value| i64::from(value.shift)), shortcut.map(|value| i64::from(value.option)), shortcut.map(|value| i64::from(value.control)), shortcut.map(|value| value.key.to_string()))
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

pub enum SettingsJob {
    Save { revision: u64, snapshot: SettingsSnapshot },
    Quit { revision: u64, snapshot: SettingsSnapshot },
    Reset,
}
pub enum SettingsResult {
    Saved { revision: u64 },
    Reset { backup: PathBuf },
    Failed(SettingsStorageError),
}
/// One ordered worker. Replaceable saves are coalesced before I/O; quit always
/// flushes the most recent snapshot without blocking the GUI caller.
pub struct SettingsWorker {
    jobs: Sender<SettingsJob>,
    results: Receiver<SettingsResult>,
}
impl SettingsWorker {
    pub fn start(path: PathBuf) -> Result<Self, std::io::Error> {
        let (jobs, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        thread::Builder::new().name("settings-worker".to_owned()).spawn(move || worker_loop(path, receiver, sender))?;
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
}
fn worker_loop(path: PathBuf, jobs: Receiver<SettingsJob>, results: Sender<SettingsResult>) {
    let mut database = match SettingsDatabase::open(path) {
        Ok(database) => database,
        Err(error) => {
            let _ = results.send(SettingsResult::Failed(error));
            return;
        }
    };
    while let Ok(job) = jobs.recv() {
        let mut latest = None;
        let mut next = Some(job);
        while let Some(job) = next.take() {
            match job {
                SettingsJob::Save { revision, snapshot } | SettingsJob::Quit { revision, snapshot } => latest = Some((revision, snapshot)),
                SettingsJob::Reset => match database.reset() {
                    Ok(backup) => {
                        // A confirmed reset supersedes unsaved replaceable state
                        // that was queued before it.
                        latest = None;
                        let _ = results.send(SettingsResult::Reset { backup });
                    }
                    Err(error) => {
                        let _ = results.send(SettingsResult::Failed(error));
                    }
                },
            }
            next = jobs.recv_timeout(SAVE_DEBOUNCE).ok();
        }
        if let Some((revision, snapshot)) = latest {
            let result = database.save(&snapshot).map(|()| SettingsResult::Saved { revision }).unwrap_or_else(SettingsResult::Failed);
            let _ = results.send(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_application::{SettingsState, Shortcut};
    use tempfile::tempdir;
    fn path() -> PathBuf {
        tempdir().unwrap().keep().join("settings.sqlite3")
    }
    fn non_utf8_location() -> Location {
        Location::root().join(&EntryName::new(b"caf\xFF".to_vec()).unwrap())
    }
    #[test]
    fn preloads_each_action_and_round_trips_non_utf8_and_empty_favorites() {
        let path = path();
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
        let path = path();
        let mut db = SettingsDatabase::open(path).unwrap();
        let snapshot = SettingsSnapshot { bindings: dual_pane_application::default_bindings(), folder_sorts: vec![], favorites: FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "A".into(), position: 0 }, FavoriteGroupRecord { id: 2, name: "B".into(), position: 1 }], items: vec![FavoriteItemRecord { id: 4, group_id: 1, name: "item".into(), target: non_utf8_location(), position: 0 }] } };
        db.save(&snapshot).unwrap();
        assert_eq!(db.load().unwrap().favorites, snapshot.favorites);
    }
    #[test]
    fn supported_old_schema_migrates_transactionally() {
        let path = path();
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 0).unwrap();
        drop(connection);
        let db = SettingsDatabase::open(path).unwrap();
        assert_eq!(db.load().unwrap().bindings.len(), ActionId::ALL.len());
    }
    #[test]
    fn newer_schema_and_failed_open_are_reported_without_reset() {
        let path = path();
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
        let path = path();
        let mut db = SettingsDatabase::open(&path).unwrap();
        db.save(&SettingsSnapshot::default()).unwrap();
        assert_eq!(db.load().unwrap().bindings.len(), ActionId::ALL.len());
        db.connection.execute("UPDATE action_binding SET command = 'not-an-integer' WHERE action = 'NewFolder'", []).unwrap();
        assert!(matches!(db.load(), Err(SettingsStorageError::Sql(_))));
    }
    #[test]
    fn explicit_reset_preserves_failed_database() {
        let path = path();
        let mut db = SettingsDatabase::open(&path).unwrap();
        let backup = db.reset().unwrap();
        assert!(backup.exists());
        assert!(SettingsDatabase::open(&path).is_ok());
    }
    #[test]
    fn worker_delivers_without_calling_on_submit_thread() {
        let path = path();
        let worker = SettingsWorker::start(path).unwrap();
        assert!(worker.submit(SettingsJob::Save { revision: 1, snapshot: SettingsState::new().snapshot() }));
        let result = std::iter::repeat_with(|| worker.take_results()).find(|items| !items.is_empty()).unwrap();
        assert!(matches!(result.as_slice(), [SettingsResult::Saved { revision: 1 }]));
        let _ = Shortcut::new(false, false, false, false, 'A');
    }
    #[test]
    fn worker_remains_available_after_reset() {
        let worker = SettingsWorker::start(path()).unwrap();
        assert!(worker.submit(SettingsJob::Reset));
        let reset = std::iter::repeat_with(|| worker.take_results()).find(|items| !items.is_empty()).unwrap();
        assert!(matches!(reset.as_slice(), [SettingsResult::Reset { .. }]));
        assert!(worker.submit(SettingsJob::Save { revision: 2, snapshot: SettingsState::new().snapshot() }));
        let saved = std::iter::repeat_with(|| worker.take_results()).find(|items| !items.is_empty()).unwrap();
        assert!(matches!(saved.as_slice(), [SettingsResult::Saved { revision: 2 }]));
    }
}
