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
fn context_menu_preferences_round_trip_and_reject_malformed_rows() {
    let (_directory, path) = temp_database();
    let mut database = SettingsDatabase::open(&path).unwrap();
    let mut snapshot = database.load().unwrap();
    snapshot.context_menus.set_visible(ContextMenuKind::Normal, ActionId::OpenItem, false).unwrap();
    database.save(&snapshot).unwrap();
    assert_eq!(database.load().unwrap().context_menus, snapshot.context_menus);

    database.connection.execute("DELETE FROM context_menu_action WHERE menu_kind = 'Normal' AND action = 'CopyItems'", []).unwrap();
    assert!(matches!(database.load(), Err(SettingsStorageError::InvalidData("context menu row count"))));
}
#[test]
fn context_menu_loading_rejects_unknown_duplicate_invalid_and_empty_configurations() {
    let cases = [("UPDATE context_menu_action SET action = 'UnknownAction' WHERE menu_kind = 'Normal' AND action = 'CopyItems'", "context menu action"), ("PRAGMA ignore_check_constraints = ON; UPDATE context_menu_action SET menu_kind = 'Unknown' WHERE menu_kind = 'Normal' AND action = 'CopyItems'", "context menu kind"), ("PRAGMA ignore_check_constraints = ON; UPDATE context_menu_action SET visible = 2 WHERE menu_kind = 'Normal' AND action = 'CopyItems'", "context menu visibility"), ("UPDATE context_menu_action SET visible = 0 WHERE menu_kind = 'Normal'", "context menu configuration"), ("ALTER TABLE context_menu_action RENAME TO old_context_menu_action; CREATE TABLE context_menu_action(menu_kind TEXT NOT NULL, action TEXT NOT NULL, visible INTEGER NOT NULL); INSERT INTO context_menu_action SELECT * FROM old_context_menu_action; INSERT INTO context_menu_action SELECT * FROM old_context_menu_action WHERE menu_kind = 'Normal' AND action = 'CopyItems'; DELETE FROM context_menu_action WHERE rowid IN (SELECT rowid FROM context_menu_action WHERE menu_kind = 'Comprehensive' AND action = 'DeletePermanently' LIMIT 1); DROP TABLE old_context_menu_action", "duplicate context menu row")];
    for (sql, reason) in cases {
        let (_directory, path) = temp_database();
        let database = SettingsDatabase::open(&path).unwrap();
        database.connection.execute_batch(sql).unwrap();
        assert!(matches!(database.load(), Err(SettingsStorageError::InvalidData(found)) if found == reason), "{reason}");
    }
}
#[test]
fn version_ten_migration_seeds_context_menu_defaults() {
    let (_directory, path) = temp_database();
    let database = SettingsDatabase::open(&path).unwrap();
    database.connection.execute("DROP TABLE context_menu_action", []).unwrap();
    database.connection.pragma_update(None, "user_version", 10).unwrap();
    drop(database);
    let database = SettingsDatabase::open(&path).unwrap();
    assert_eq!(database.load().unwrap().context_menus, ContextMenuPreferences::default());
}
#[test]
fn every_supported_older_schema_seeds_context_menu_defaults() {
    for version in 1..=2 {
        let (_directory, path) = legacy_database(version, "");
        assert_eq!(SettingsDatabase::open(&path).unwrap().load().unwrap().context_menus, ContextMenuPreferences::default(), "version {version}");
    }
    for version in 3..=10 {
        let (_directory, path) = temp_database();
        drop(SettingsDatabase::open(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        remove_newer_schema(&connection, version);
        if version < 8 {
            drop_preview_height_columns(&connection);
        }
        connection.pragma_update(None, "user_version", version).unwrap();
        drop(connection);
        assert_eq!(SettingsDatabase::open(&path).unwrap().load().unwrap().context_menus, ContextMenuPreferences::default(), "version {version}");
    }
}
#[test]
fn session_round_trips_tab_order_active_tabs_and_each_window_state() {
    let (_directory, path) = temp_database();
    let mut db = SettingsDatabase::open(&path).unwrap();
    let left_first = non_utf8_location();
    let left_second = Location::root().join(&EntryName::new("left-second").unwrap());
    let right = Location::root().join(&EntryName::new("right").unwrap());
    for state in [WindowState::Normal, WindowState::Zoomed, WindowState::FullScreen] {
        let session = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: left_first.clone() }, TabSnapshot { location: left_second.clone() }], active_tab: Some(1) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: right.clone() }], active_tab: Some(0) }, active_browser: BrowserSide::Right, layout: WindowLayout { frame: WindowFrame { x: -42, y: 17, width: 1200, height: 800 }, state, sidebar_splitter: 280, browser_splitter: 610, left_preview_height: 0, right_preview_height: 0 } };
        db.save_session(&session).unwrap();
        assert_eq!(db.load_with_session().unwrap().session, SessionSnapshot::Saved(session));
    }
}
#[test]
fn damaged_session_rows_do_not_block_settings_or_a_replacement_session_save() {
    let (_directory, path) = temp_database();
    let mut db = SettingsDatabase::open(&path).unwrap();
    db.connection.execute("INSERT INTO session_window(id, x, y, width, height, state, sidebar_splitter, browser_splitter) VALUES (1, 0, 0, 800, 600, 0, 200, 400)", []).unwrap();
    assert_eq!(db.load_with_session().unwrap().session, SessionSnapshot::Damaged);

    let location = Location::root().join(&EntryName::new("restored").unwrap());
    let replacement = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location.clone() }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location }], active_tab: Some(0) }, active_browser: BrowserSide::Left, layout: WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 } };
    db.save_session(&replacement).unwrap();
    assert_eq!(db.load_with_session().unwrap().session, SessionSnapshot::Saved(replacement));
}
#[test]
fn favorites_round_trip_in_order() {
    let (_directory, path) = temp_database();
    let mut db = SettingsDatabase::open(&path).unwrap();
    let snapshot = SettingsSnapshot { bindings: dual_pane_application::default_bindings(), folder_sorts: vec![], favorites: FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "A".into(), position: 0 }, FavoriteGroupRecord { id: 2, name: "B".into(), position: 1 }], items: vec![FavoriteItemRecord { id: 4, group_id: 1, name: "item".into(), target: non_utf8_location(), position: 0 }] }, ..SettingsSnapshot::default() };
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
fn failed_open_does_not_preload_bindings_or_commit_a_migration() {
    for version in [2, SCHEMA_VERSION] {
        let (_directory, path) = temp_database();
        let db = SettingsDatabase::open(&path).unwrap();
        if version < 8 {
            drop_preview_height_columns(&db.connection);
        }
        remove_newer_schema(&db.connection, version);
        db.connection.execute("DELETE FROM action_binding WHERE action = 'NewTab'", []).unwrap();
        db.connection.execute("INSERT INTO favorite_group(id, name, position) VALUES (1, ' ', 0)", []).unwrap();
        db.connection.pragma_update(None, "user_version", version).unwrap();
        drop(db);

        assert!(matches!(SettingsDatabase::open(&path), Err(SettingsStorageError::InvalidData("Favorites hierarchy"))));
        let connection = Connection::open(&path).unwrap();
        let stored_version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        let new_tab_bindings: i64 = connection.query_row("SELECT COUNT(*) FROM action_binding WHERE action = 'NewTab'", [], |row| row.get(0)).unwrap();
        assert_eq!(stored_version, version);
        assert_eq!(new_tab_bindings, 0);
    }
}
#[test]
fn supported_old_schema_migrates_transactionally() {
    let (_directory, path) = temp_database();
    let database = SettingsDatabase::open(&path).unwrap();
    drop(database);
    let connection = Connection::open(&path).unwrap();
    drop_preview_height_columns(&connection);
    remove_newer_schema(&connection, 0);
    connection.pragma_update(None, "user_version", 0).unwrap();
    drop(connection);
    let db = SettingsDatabase::open(&path).unwrap();
    assert_eq!(db.load().unwrap().bindings.len(), ActionId::ALL.len());
}
#[test]
fn version_three_migration_starts_with_no_session_and_default_startup_notice_preference() {
    let (_directory, path) = legacy_database(3, "");
    let database = SettingsDatabase::open(&path).unwrap();
    let loaded = database.load_with_session().unwrap();
    assert_eq!(loaded.session, SessionSnapshot::Absent);
    assert!(!loaded.snapshot.hide_notices_at_startup);
    let version: i64 = database.connection.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}
#[test]
fn quit_confirmation_choice_defaults_to_asking_and_round_trips() {
    let (_directory, path) = temp_database();
    let mut database = SettingsDatabase::open(&path).unwrap();
    assert!(!database.load().unwrap().skip_quit_confirmation);
    for skip in [true, false] {
        database.save(&SettingsSnapshot { skip_quit_confirmation: skip, ..database.load().unwrap() }).unwrap();
        drop(database);
        database = SettingsDatabase::open(&path).unwrap();
        assert_eq!(database.load().unwrap().skip_quit_confirmation, skip);
    }
}
#[test]
fn visibility_defaults_migrate_and_all_values_round_trip() {
    let (_directory, path) = temp_database();
    drop(SettingsDatabase::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    drop_preview_height_columns(&connection);
    remove_newer_schema(&connection, 6);
    connection.execute("DELETE FROM setting_marker WHERE key IN ('show_macos_items', 'show_dotfiles', 'show_macos_invisible')", []).unwrap();
    connection.pragma_update(None, "user_version", 6).unwrap();
    drop(connection);
    let mut database = SettingsDatabase::open(&path).unwrap();
    assert_eq!(database.load().unwrap().visibility, VisibilityPolicy::default());
    let stored: i64 = database.connection.query_row("SELECT COUNT(*) FROM setting_marker WHERE key IN ('show_macos_items', 'show_dotfiles', 'show_macos_invisible')", [], |row| row.get(0)).unwrap();
    assert_eq!(stored, 3, "migration persists defaults, rather than only supplying fallbacks at read time");
    for show_macos_items in [false, true] {
        for show_dotfiles in [false, true] {
            for show_macos_invisible in [false, true] {
                let snapshot = SettingsSnapshot { visibility: VisibilityPolicy { show_macos_items, show_dotfiles, show_macos_invisible }, ..SettingsSnapshot::default() };
                database.save(&snapshot).unwrap();
                assert_eq!(database.load().unwrap().visibility, snapshot.visibility);
            }
        }
    }
    let (replacement, reset) = reset_database(&path, Some(database));
    let backup = reset.unwrap().unwrap();
    assert_eq!(SettingsDatabase::open(&backup).unwrap().load().unwrap().visibility, VisibilityPolicy { show_macos_items: true, show_dotfiles: true, show_macos_invisible: true });
    let database = replacement.unwrap();
    assert_eq!(database.load().unwrap().visibility, VisibilityPolicy::default());
    database.connection.execute("INSERT INTO setting_marker(key, value) VALUES ('show_dotfiles', 2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", []).unwrap();
    assert!(matches!(database.load(), Err(SettingsStorageError::InvalidData("Boolean setting"))));
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
    assert!(matches!(results.as_slice(), [SettingsResult::Saved { revision: 3 }, SettingsResult::Loaded(loaded)] if loaded.snapshot.folder_sorts == snapshot.folder_sorts));
}
#[test]
fn a_reset_discards_saves_queued_before_it() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path).unwrap();
    assert!(worker.submit(SettingsJob::Save { revision: 3, snapshot: sorted_snapshot() }));
    assert!(worker.submit(SettingsJob::Reset));
    assert!(worker.submit(SettingsJob::Load));
    let results = worker.shutdown(None, Duration::from_secs(10));
    assert!(matches!(results.as_slice(), [SettingsResult::Reset { .. }, SettingsResult::Loaded(loaded)] if loaded.snapshot.folder_sorts.is_empty()));
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
#[test]
fn shutdown_flushes_a_session_when_no_settings_snapshot_is_dirty() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path.clone()).unwrap();
    let location = Location::root().join(&EntryName::new("saved").unwrap());
    let session = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location.clone() }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location }], active_tab: Some(0) }, active_browser: BrowserSide::Left, layout: WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 } };
    let results = worker.shutdown_with_session(None, Some((12, session.clone())), Duration::from_secs(10));
    assert!(results.iter().any(|result| matches!(result, SettingsResult::SessionSaved { revision: 12 })));
    assert_eq!(SettingsDatabase::open(&path).unwrap().load_with_session().unwrap().session, SessionSnapshot::Saved(session));
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
fn loaded_binding(path: &Path, action: ActionId) -> Option<Option<Shortcut>> {
    SettingsDatabase::open(path).unwrap().load().unwrap().bindings.into_iter().find(|binding| binding.action == action).map(|binding| binding.shortcut)
}
/// The bindings the application uses after loading the database at `path`.
fn effective_bindings(path: &Path) -> SettingsState {
    let mut state = SettingsState::new();
    state.apply(SettingsDatabase::open(path).unwrap().load().unwrap());
    state
}
fn assert_no_collisions(state: &SettingsState) {
    let bound = state.bindings().into_iter().filter_map(|binding| binding.shortcut).collect::<Vec<_>>();
    assert_eq!(bound.iter().filter(|shortcut| bound.iter().filter(|other| other == shortcut).count() > 1).count(), 0, "no shortcut is shared: {bound:?}");
}
fn command_w() -> Option<Shortcut> {
    Some(Shortcut::new(true, false, false, false, Key::Character('W')))
}
#[test]
fn version_one_bindings_migrate_to_named_keys_and_explicit_defaults() {
    let (_directory, path) = legacy_database(1, "UPDATE action_binding SET command = NULL, shift = NULL, option = NULL, control = NULL, key = NULL WHERE action = 'NewFolder';");
    assert_eq!(loaded_binding(&path, ActionId::NavigateParent), Some(dual_pane_application::default_shortcut(ActionId::NavigateParent)));
    assert_eq!(loaded_binding(&path, ActionId::NewFolder), Some(dual_pane_application::default_shortcut(ActionId::NewFolder)));
    assert_eq!(loaded_binding(&path, ActionId::SortByNameAscending), Some(None));
    let version: i64 = Connection::open(&path).unwrap().pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}
#[test]
fn version_one_migration_keeps_a_customized_navigate_parent() {
    let (_directory, path) = legacy_database(1, "UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'P' WHERE action = 'NavigateParent';");
    assert_eq!(loaded_binding(&path, ActionId::NavigateParent), Some(Some(Shortcut::new(true, false, false, false, Key::Character('P')))));
}
#[test]
fn a_fresh_database_binds_command_w_to_close_tab() {
    let (_directory, path) = temp_database();
    let state = effective_bindings(&path);
    assert_eq!(state.binding(ActionId::CloseTab), command_w());
    assert_eq!(state.binding(ActionId::CloseWindow), None);
    assert_eq!(state.binding(ActionId::NewTab), dual_pane_application::default_shortcut(ActionId::NewTab));
    assert_no_collisions(&state);
}
#[test]
fn a_new_open_item_default_does_not_replace_an_existing_command_down_choice() {
    let (_directory, path) = temp_database();
    drop(SettingsDatabase::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection.execute("DELETE FROM action_shortcut WHERE action = 'OpenItem'", []).unwrap();
    connection.execute("DELETE FROM action_binding WHERE action = 'OpenItem'", []).unwrap();
    connection.execute("UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'Down' WHERE action = 'NewFolder'", []).unwrap();
    connection.execute("UPDATE action_shortcut SET command = 1, shift = 0, option = 0, control = 0, key = 'Down' WHERE action = 'NewFolder'", []).unwrap();
    drop(connection);

    let state = effective_bindings(&path);
    assert_eq!(state.binding(ActionId::NewFolder), Some(Shortcut::new(true, false, false, false, Key::Down)));
    assert_eq!(state.binding(ActionId::OpenItem), None);
    assert_no_collisions(&state);
}
#[test]
fn version_eight_profiles_gain_new_rename_and_trash_factory_shortcuts() {
    let (_directory, path) = temp_database();
    drop(SettingsDatabase::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    remove_newer_schema(&connection, 8);
    connection.execute_batch("DELETE FROM action_shortcut WHERE action IN ('RenameItem', 'MoveToTrash'); INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('RenameItem', 0, 0, 0, 0, 0, 'F2'); INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 0, 0, 1, 0, 0, 'Delete'); INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 1, 0, 1, 0, 0, 'Forward Delete'); UPDATE action_binding SET command = 0, shift = 0, option = 0, control = 0, key = 'F2' WHERE action = 'RenameItem'; UPDATE action_binding SET command = 0, shift = 1, option = 0, control = 0, key = 'Delete' WHERE action = 'MoveToTrash'; PRAGMA user_version = 8;").unwrap();
    drop(connection);

    let state = effective_bindings(&path);
    assert_eq!(state.shortcuts(ActionId::RenameItem), default_shortcuts(ActionId::RenameItem));
    assert_eq!(state.shortcuts(ActionId::MoveToTrash), default_shortcuts(ActionId::MoveToTrash));
    assert_no_collisions(&state);
}
#[test]
fn version_eight_profiles_keep_custom_rename_and_trash_shortcuts() {
    let (_directory, path) = temp_database();
    drop(SettingsDatabase::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    remove_newer_schema(&connection, 8);
    connection.execute_batch("DELETE FROM action_shortcut WHERE action IN ('RenameItem', 'MoveToTrash'); INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('RenameItem', 0, 1, 0, 0, 0, 'J'); INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 0, 1, 0, 0, 0, 'K'); UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'J' WHERE action = 'RenameItem'; UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'K' WHERE action = 'MoveToTrash'; PRAGMA user_version = 8;").unwrap();
    drop(connection);

    let state = effective_bindings(&path);
    assert_eq!(state.shortcuts(ActionId::RenameItem), &[Shortcut::new(true, false, false, false, Key::Character('J'))]);
    assert_eq!(state.shortcuts(ActionId::MoveToTrash), &[Shortcut::new(true, false, false, false, Key::Character('K'))]);
    assert_no_collisions(&state);
}
#[test]
fn version_nine_profiles_gain_command_forward_delete_for_trash() {
    let (_directory, path) = temp_database();
    drop(SettingsDatabase::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    remove_newer_schema(&connection, 9);
    connection.execute_batch("DELETE FROM action_shortcut WHERE action = 'MoveToTrash'; INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 0, 1, 0, 0, 0, 'Delete'), ('MoveToTrash', 1, 0, 1, 0, 0, 'Delete'), ('MoveToTrash', 2, 0, 1, 0, 0, 'Forward Delete'); PRAGMA user_version = 9;").unwrap();
    drop(connection);

    let state = effective_bindings(&path);
    assert_eq!(state.shortcuts(ActionId::MoveToTrash), default_shortcuts(ActionId::MoveToTrash));
    assert_no_collisions(&state);
}
#[test]
fn version_nine_profiles_keep_custom_trash_shortcuts() {
    let (_directory, path) = temp_database();
    drop(SettingsDatabase::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    remove_newer_schema(&connection, 9);
    connection.execute_batch("DELETE FROM action_shortcut WHERE action = 'MoveToTrash'; INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 0, 1, 0, 0, 0, 'Delete'), ('MoveToTrash', 1, 0, 1, 0, 0, 'Delete'); PRAGMA user_version = 9;").unwrap();
    drop(connection);

    let state = effective_bindings(&path);
    assert_eq!(state.shortcuts(ActionId::MoveToTrash), &[Shortcut::new(true, false, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::Delete)]);
}
#[test]
fn command_forward_delete_held_by_another_action_stays_there() {
    let command_forward_delete = Shortcut::new(true, false, false, false, Key::ForwardDelete);
    let prior_trash = "INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 0, 1, 0, 0, 0, 'Delete'), ('MoveToTrash', 1, 0, 1, 0, 0, 'Delete'), ('MoveToTrash', 2, 0, 1, 0, 0, 'Forward Delete');";
    let version_three_trash = "INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('MoveToTrash', 0, 0, 1, 0, 0, 'Delete'), ('MoveToTrash', 1, 0, 1, 0, 0, 'Forward Delete');";
    for (version, trash) in [(8, version_three_trash), (9, prior_trash)] {
        let (_directory, path) = temp_database();
        drop(SettingsDatabase::open(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        remove_newer_schema(&connection, version);
        // Open Item loads after Move to Trash, so a conflicting trash list
        // would win and drop this custom shortcut.
        connection.execute_batch(&format!("DELETE FROM action_shortcut WHERE action IN ('MoveToTrash', 'OpenItem'); {trash} INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES ('OpenItem', 0, 1, 0, 0, 0, 'Forward Delete'); UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'Forward Delete' WHERE action = 'OpenItem'; PRAGMA user_version = {version};")).unwrap();
        drop(connection);

        let state = effective_bindings(&path);
        assert_eq!(state.shortcuts(ActionId::OpenItem), &[command_forward_delete], "version {version}");
        assert_eq!(state.shortcuts(ActionId::MoveToTrash), &[Shortcut::new(true, false, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::ForwardDelete)], "version {version}");
        assert_no_collisions(&state);
    }
}
#[test]
fn version_two_moves_command_w_from_close_window_to_close_tab() {
    let (_directory, path) = legacy_database(2, "");
    let state = effective_bindings(&path);
    assert_eq!(state.binding(ActionId::CloseTab), command_w());
    assert_eq!(state.binding(ActionId::CloseWindow), None);
    assert_eq!(loaded_binding(&path, ActionId::CloseWindow), Some(None), "Close Window is stored as unbound, not deleted");
    for action in [ActionId::NewTab, ActionId::NavigateBack, ActionId::NavigateForward, ActionId::RefreshFolder] {
        assert_eq!(state.binding(action), dual_pane_application::default_shortcut(action), "{action:?}");
    }
    assert_no_collisions(&state);
}
#[test]
fn version_one_chains_through_version_two_to_three() {
    let (_directory, path) = legacy_database(1, "UPDATE action_binding SET command = NULL, shift = NULL, option = NULL, control = NULL, key = NULL WHERE action = 'CloseWindow';");
    let state = effective_bindings(&path);
    assert_eq!(state.binding(ActionId::CloseTab), command_w(), "a version 1 default Close Window does not keep Command+W");
    assert_eq!(state.binding(ActionId::CloseWindow), None);
    assert_eq!(state.binding(ActionId::NavigateParent), dual_pane_application::default_shortcut(ActionId::NavigateParent));

    let (_directory, path) = legacy_database(1, "");
    let state = effective_bindings(&path);
    assert_eq!((state.binding(ActionId::CloseTab), state.binding(ActionId::CloseWindow)), (command_w(), None));
    assert_no_collisions(&state);
}
#[test]
fn customized_bindings_survive_the_close_tab_migration() {
    let (_directory, path) = legacy_database(2, "UPDATE action_binding SET command = 1, shift = 1, option = 0, control = 0, key = 'W' WHERE action = 'CloseWindow'; UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'T' WHERE action = 'NewFolder';");
    let state = effective_bindings(&path);
    assert_eq!(state.binding(ActionId::CloseWindow), Some(Shortcut::new(true, true, false, false, Key::Character('W'))));
    assert_eq!(state.binding(ActionId::CloseTab), command_w());
    assert_eq!(state.binding(ActionId::NewFolder), Some(Shortcut::new(true, false, false, false, Key::Character('T'))), "an earlier catalogue action keeps its custom shortcut");
    assert_eq!(state.binding(ActionId::NewTab), None, "the new action's default is taken, so it is left unbound");
    assert_no_collisions(&state);
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
#[test]
fn ordered_shortcut_lists_and_unbinding_survive_reopen() {
    let (_directory, path) = temp_database();
    let mut state = SettingsState::new();
    let first = Shortcut::new(true, false, false, false, Key::Character('J'));
    let second = Shortcut::new(false, true, false, true, Key::Function(24));
    state.set_shortcuts(ActionId::NewFolder, vec![first, second]).unwrap();
    state.set_shortcuts(ActionId::NewTab, vec![]).unwrap();
    SettingsDatabase::open(&path).unwrap().save(&state.snapshot()).unwrap();
    let mut restored = SettingsState::new();
    restored.apply(SettingsDatabase::open(&path).unwrap().load().unwrap());
    assert_eq!(restored.shortcuts(ActionId::NewFolder), &[first, second]);
    assert!(restored.shortcuts(ActionId::NewTab).is_empty());
}
#[test]
fn version_four_shortcuts_migrate_to_ordered_entries() {
    let (_directory, path) = temp_database();
    let database = SettingsDatabase::open(&path).unwrap();
    remove_newer_schema(&database.connection, 4);
    database.connection.execute_batch("DROP TABLE action_shortcut; UPDATE action_binding SET command = 1, shift = 0, option = 0, control = 0, key = 'J' WHERE action = 'NewFolder'; UPDATE action_binding SET command = NULL, shift = NULL, option = NULL, control = NULL, key = NULL WHERE action = 'NewTab'; PRAGMA user_version = 4;").unwrap();
    drop_preview_height_columns(&database.connection);
    drop(database);
    let loaded = SettingsDatabase::open(&path).unwrap().load().unwrap();
    let mut state = SettingsState::new();
    state.apply(loaded);
    assert_eq!(state.shortcuts(ActionId::NewFolder), &[Shortcut::new(true, false, false, false, Key::Character('J'))]);
    assert!(state.shortcuts(ActionId::NewTab).is_empty());
    let count: i64 = Connection::open(&path).unwrap().query_row("SELECT COUNT(*) FROM action_shortcut WHERE action = 'NewFolder'", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn a_reset_never_moves_or_changes_the_operation_journal() {
    use crate::operation_journal::{JOURNAL_FILE_NAME, Journal, LaunchId, journal_path};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sqlite3");
    let database = SettingsDatabase::open(&path).unwrap();
    let journal = Journal::new(Some(journal_path(&path)), LaunchId::generate());
    assert!(journal.reopen());
    journal.record(1, directory.path(), 0).unwrap();
    let journal_files = |directory: &Path| fs::read_dir(directory).unwrap().filter_map(Result::ok).map(|entry| entry.file_name().to_string_lossy().into_owned()).filter(|name| name.starts_with(JOURNAL_FILE_NAME)).collect::<std::collections::BTreeSet<_>>();
    let before = journal_files(directory.path());
    let contents = fs::read(directory.path().join(JOURNAL_FILE_NAME)).unwrap();
    let (_, result) = reset_database(&path, Some(database));
    assert!(result.unwrap().is_some(), "the settings database was preserved aside");
    assert_eq!(journal_files(directory.path()), before, "no journal file moved");
    assert_eq!(fs::read(directory.path().join(JOURNAL_FILE_NAME)).unwrap(), contents);
    assert_eq!(journal.folders(1), vec![directory.path().to_path_buf()], "the record is still there");
}
