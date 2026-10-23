use super::migration::SCHEMA_VERSION;
use super::reset::{reset_database_to, with_suffix};
use super::*;
use dual_pane_application::{SettingsState, TabSnapshot};
use tempfile::{TempDir, tempdir};
/// A database path inside a temporary directory that lives as long as the
/// returned guard.
fn temp_database() -> (TempDir, PathBuf) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("settings.sqlite3");
    (directory, path)
}
fn drop_preview_height_columns(connection: &Connection) {
    connection.execute_batch("ALTER TABLE session_window DROP COLUMN right_preview_height; ALTER TABLE session_window DROP COLUMN left_preview_height;").unwrap();
}
/// Removes structures a database at `version` did not have before a test
/// asks the production migrator to upgrade it again.
fn remove_newer_schema(connection: &Connection, version: i64) {
    if version < 12 {
        connection.execute_batch("DROP TABLE file_association_app; DROP TABLE file_association_type;").unwrap();
    }
    if version < 11 {
        connection.execute("DROP TABLE context_menu_action", []).unwrap();
    }
}
fn wait_for_results(worker: &SettingsWorker) -> Vec<SettingsResult> {
    std::iter::repeat_with(|| worker.take_results()).find(|items| !items.is_empty()).unwrap()
}
fn non_utf8_location() -> Location {
    Location::root().join(&EntryName::new(b"caf\xFF".to_vec()).unwrap())
}

/// The action catalogue before schema version 3 added the tab actions.
const LEGACY_ACTIONS: [&str; 13] = ["FocusOtherBrowser", "NavigateParent", "CloseWindow", "QuitApplication", "NewFolder", "SortByNameAscending", "SortByNameDescending", "SortByTypeAscending", "SortByTypeDescending", "SortByDateAscending", "SortByDateDescending", "SortBySizeAscending", "SortBySizeDescending"];

/// A database as schema `version` (1 or 2) wrote it, built by hand so the
/// current migrations and catalogue cannot shape it, then changed by `sql`.
/// Version 1 stored NavigateParent as a plain `L`; both stored CloseWindow
/// as Command+W and had no tab actions.
fn legacy_database(version: i64, sql: &str) -> (TempDir, PathBuf) {
    let (directory, path) = temp_database();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE action_binding(action TEXT PRIMARY KEY NOT NULL, command INTEGER, shift INTEGER, option INTEGER, control INTEGER, key TEXT); CREATE TABLE folder_sort(target BLOB PRIMARY KEY NOT NULL, field INTEGER NOT NULL, direction INTEGER NOT NULL, recent INTEGER NOT NULL); CREATE INDEX folder_sort_recent ON folder_sort(recent); CREATE TABLE setting_marker(key TEXT PRIMARY KEY NOT NULL, value INTEGER NOT NULL); CREATE TABLE favorite_group(id INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, position INTEGER NOT NULL); CREATE UNIQUE INDEX favorite_group_position ON favorite_group(position); CREATE TABLE favorite_item(id INTEGER PRIMARY KEY NOT NULL, group_id INTEGER NOT NULL REFERENCES favorite_group(id) ON DELETE RESTRICT, name TEXT NOT NULL, target BLOB NOT NULL, position INTEGER NOT NULL); CREATE INDEX favorite_item_group ON favorite_item(group_id, position);").unwrap();
    for action in LEGACY_ACTIONS {
        let (command, shift, option, key) = match action {
            "FocusOtherBrowser" => (Some(0), Some(0), Some(1), Some("F")),
            "NavigateParent" if version == 1 => (Some(0), Some(0), Some(0), Some("L")),
            "NavigateParent" => (Some(1), Some(0), Some(0), Some("Up")),
            "CloseWindow" => (Some(1), Some(0), Some(0), Some("W")),
            "QuitApplication" => (Some(1), Some(0), Some(0), Some("Q")),
            "NewFolder" => (Some(1), Some(1), Some(0), Some("N")),
            _ => (None, None, None, None),
        };
        connection.execute("INSERT INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![action, command, shift, option, command.map(|_| 0), key]).unwrap();
    }
    connection.execute_batch(sql).unwrap();
    connection.pragma_update(None, "user_version", version).unwrap();
    (directory, path)
}

mod database;
mod preferences;
mod shortcuts;
mod worker;
