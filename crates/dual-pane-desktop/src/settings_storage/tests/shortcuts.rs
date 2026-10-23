use super::*;

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
