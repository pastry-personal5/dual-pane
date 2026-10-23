use super::*;

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
