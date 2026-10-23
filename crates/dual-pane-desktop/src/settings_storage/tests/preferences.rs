use super::*;

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
    remove_newer_schema(&database.connection, 10);
    database.connection.pragma_update(None, "user_version", 10).unwrap();
    drop(database);
    let database = SettingsDatabase::open(&path).unwrap();
    assert_eq!(database.load().unwrap().context_menus, ContextMenuPreferences::default());
}
#[test]
fn file_association_choices_round_trip_and_malformed_rows_are_rejected() {
    let (_directory, path) = temp_database();
    let mut database = SettingsDatabase::open(&path).unwrap();
    let editor = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
    let mut snapshot = database.load().unwrap();
    snapshot.file_associations.pin("public.plain-text").unwrap();
    snapshot.file_associations.set_selected("public.plain-text", ContextMenuKind::Normal, editor.clone(), true).unwrap();
    database.save(&snapshot).unwrap();
    drop(database);
    let mut database = SettingsDatabase::open(&path).unwrap();
    assert_eq!(database.load().unwrap().file_associations, snapshot.file_associations, "choices survive a database relaunch");

    database.connection.execute_batch("CREATE TRIGGER reject_file_association_app BEFORE INSERT ON file_association_app BEGIN SELECT RAISE(ABORT, 'stop'); END;").unwrap();
    let viewer = ApplicationIdentity { bundle_identifier: "com.example.Viewer".into(), bundle_url: "file:///Applications/Viewer.app".into() };
    let mut replacement = snapshot.clone();
    replacement.file_associations.set_selected("public.plain-text", ContextMenuKind::Comprehensive, viewer, true).unwrap();
    assert!(database.save(&replacement).is_err());
    database.connection.execute("DROP TRIGGER reject_file_association_app", []).unwrap();
    assert_eq!(database.load().unwrap().file_associations, snapshot.file_associations, "a failed transactional save preserves every prior association row");

    database.connection.execute("UPDATE file_association_app SET bundle_url = 'not-a-file-url'", []).unwrap();
    assert!(matches!(database.load(), Err(SettingsStorageError::InvalidData("file association configuration"))));
}

#[test]
fn reset_removes_local_file_associations_and_preserves_them_in_the_backup() {
    let (_directory, path) = temp_database();
    let mut database = SettingsDatabase::open(&path).unwrap();
    let editor = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
    let mut snapshot = database.load().unwrap();
    snapshot.file_associations.set_selected("public.plain-text", ContextMenuKind::Normal, editor, true).unwrap();
    database.save(&snapshot).unwrap();
    let (replacement, result) = reset_database(&path, Some(database));
    let backup = result.unwrap().unwrap();
    assert_eq!(SettingsDatabase::open(&backup).unwrap().load().unwrap().file_associations, snapshot.file_associations);
    assert!(replacement.unwrap().load().unwrap().file_associations.entries().is_empty());
}

#[test]
fn version_eleven_migration_adds_empty_associations_and_setup_action_rows() {
    let (_directory, path) = temp_database();
    let database = SettingsDatabase::open(&path).unwrap();
    database.connection.execute_batch("DELETE FROM context_menu_action WHERE action = 'ConfigureOpenWith'; DROP TABLE file_association_app; DROP TABLE file_association_type;").unwrap();
    database.connection.pragma_update(None, "user_version", 11).unwrap();
    drop(database);
    let database = SettingsDatabase::open(&path).unwrap();
    let loaded = database.load().unwrap();
    assert!(loaded.file_associations.entries().is_empty());
    assert!(loaded.context_menus.visible(ContextMenuKind::Normal, ActionId::ConfigureOpenWith));
    assert!(loaded.context_menus.visible(ContextMenuKind::Comprehensive, ActionId::ConfigureOpenWith));
}
#[test]
fn every_supported_older_schema_seeds_context_menu_defaults() {
    for version in 1..=2 {
        let (_directory, path) = legacy_database(version, "");
        let loaded = SettingsDatabase::open(&path).unwrap().load().unwrap();
        assert_eq!(loaded.context_menus, ContextMenuPreferences::default(), "version {version}");
        assert!(loaded.file_associations.entries().is_empty(), "version {version}");
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
        let loaded = SettingsDatabase::open(&path).unwrap().load().unwrap();
        assert_eq!(loaded.context_menus, ContextMenuPreferences::default(), "version {version}");
        assert!(loaded.file_associations.entries().is_empty(), "version {version}");
    }
}
