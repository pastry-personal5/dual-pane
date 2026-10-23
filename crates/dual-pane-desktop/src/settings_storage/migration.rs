//! Schema versions and the migration steps between them.

use super::shortcuts::{binding_params, insert_shortcut, replace_shortcuts, stored_shortcuts};
use super::*;

pub(super) const SCHEMA_VERSION: i64 = 12;

/// Move to Trash factory lists as the migrations that introduced them wrote
/// them, so a later default never changes what an earlier step writes.
pub(super) const VERSION_NINE_TRASH: [Shortcut; 3] = [Shortcut::new(true, false, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::ForwardDelete)];

pub(super) const VERSION_TEN_TRASH: [Shortcut; 4] = [Shortcut::new(true, false, false, false, Key::Delete), Shortcut::new(true, false, false, false, Key::ForwardDelete), Shortcut::new(false, true, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::ForwardDelete)];

pub(super) fn migrate(connection: &mut Connection) -> Result<(), SettingsStorageError> {
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
    if version < 3 {
        // Command+W moves from CloseWindow to the new CloseTab, which preload
        // adds after this migration. A CloseWindow still on that default
        // becomes unbound so CloseTab can take it; any other CloseWindow
        // choice is kept, and the ID keeps its close-window meaning.
        tx.execute("UPDATE action_binding SET command = NULL, shift = NULL, option = NULL, control = NULL, key = NULL WHERE action = 'CloseWindow' AND IFNULL(command, 0) = 1 AND IFNULL(shift, 0) = 0 AND IFNULL(option, 0) = 0 AND IFNULL(control, 0) = 0 AND key = 'W'", [])?;
    }
    if version < 4 {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS session_browser(browser INTEGER PRIMARY KEY NOT NULL, active_position INTEGER NOT NULL, active INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS session_tab(browser INTEGER NOT NULL, position INTEGER NOT NULL, location BLOB NOT NULL, PRIMARY KEY(browser, position)); CREATE TABLE IF NOT EXISTS session_window(id INTEGER PRIMARY KEY NOT NULL CHECK(id = 1), x INTEGER NOT NULL, y INTEGER NOT NULL, width INTEGER NOT NULL, height INTEGER NOT NULL, state INTEGER NOT NULL, sidebar_splitter INTEGER NOT NULL, browser_splitter INTEGER NOT NULL);")?;
    }
    if version < 5 {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS action_shortcut(action TEXT NOT NULL REFERENCES action_binding(action) ON DELETE CASCADE, position INTEGER NOT NULL, command INTEGER NOT NULL, shift INTEGER NOT NULL, option INTEGER NOT NULL, control INTEGER NOT NULL, key TEXT NOT NULL, PRIMARY KEY(action, position));")?;
        let mut statement = tx.prepare("SELECT action, command, shift, option, control, key FROM action_binding WHERE key IS NOT NULL")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or_default() != 0, row.get::<_, Option<i64>>(2)?.unwrap_or_default() != 0, row.get::<_, Option<i64>>(3)?.unwrap_or_default() != 0, row.get::<_, Option<i64>>(4)?.unwrap_or_default() != 0, row.get::<_, String>(5)?)))?.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (name, command, shift, option, control, text) in rows {
            if let (Some(action), Some(key)) = (ActionId::parse(&name), Key::parse(&text)) {
                let shortcut = Shortcut::new(command, shift, option, control, key);
                if shortcut.is_valid() && !shortcut.is_reserved() {
                    tx.execute("INSERT OR IGNORE INTO action_shortcut(action, position, command, shift, option, control, key) VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6)", params![action.as_str(), i64::from(command), i64::from(shift), i64::from(option), i64::from(control), shortcut.key.as_text()])?;
                }
            }
        }
    }
    if version < 6 {
        let previous_default = Shortcut::new(true, false, false, false, Key::Delete);
        let first_default = Shortcut::new(false, true, false, false, Key::Delete);
        let second_default = Shortcut::new(false, true, false, false, Key::ForwardDelete);
        let parent = tx.query_row("SELECT command, shift, option, control, key FROM action_binding WHERE action = 'MoveToTrash'", [], |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or_default() != 0, row.get::<_, Option<i64>>(1)?.unwrap_or_default() != 0, row.get::<_, Option<i64>>(2)?.unwrap_or_default() != 0, row.get::<_, Option<i64>>(3)?.unwrap_or_default() != 0, row.get::<_, Option<String>>(4)?))).optional()?;
        let mut statement = tx.prepare("SELECT command, shift, option, control, key FROM action_shortcut WHERE action = 'MoveToTrash' ORDER BY position")?;
        let existing = statement
            .query_map([], |row| {
                let command = row.get::<_, i64>(0)? != 0;
                let shift = row.get::<_, i64>(1)? != 0;
                let option = row.get::<_, i64>(2)? != 0;
                let control = row.get::<_, i64>(3)? != 0;
                let key_text: String = row.get(4)?;
                Ok(Key::parse(&key_text).map(|key| Shortcut::new(command, shift, option, control, key)))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let previous_default_is_active = existing.as_slice() == [Some(previous_default)];
        let partially_seeded_default = existing.is_empty() && parent.as_ref().is_some_and(|(command, shift, option, control, key)| !*command && *shift && !*option && !*control && key.as_deref() == Some("Delete"));
        let first_default_is_active = existing.as_slice() == [Some(first_default)];
        if previous_default_is_active || partially_seeded_default || first_default_is_active {
            tx.execute("DELETE FROM action_shortcut WHERE action = 'MoveToTrash'", [])?;
            insert_shortcut(&tx, ActionId::MoveToTrash, 0, first_default)?;
            insert_shortcut(&tx, ActionId::MoveToTrash, 1, second_default)?;
            tx.execute("UPDATE action_binding SET command = 0, shift = 1, option = 0, control = 0, key = 'Delete' WHERE action = 'MoveToTrash'", [])?;
        }
    }
    if version < 7 {
        // Freeze the three new defaults for both older profiles and fresh
        // databases. A future release can change its fresh-profile defaults
        // without silently changing an already migrated person's choices.
        tx.execute_batch("INSERT OR IGNORE INTO setting_marker(key, value) VALUES ('show_macos_items', 0); INSERT OR IGNORE INTO setting_marker(key, value) VALUES ('show_dotfiles', 1); INSERT OR IGNORE INTO setting_marker(key, value) VALUES ('show_macos_invisible', 0);")?;
    }
    if version < 8 {
        tx.execute_batch("ALTER TABLE session_window ADD COLUMN left_preview_height INTEGER NOT NULL DEFAULT 0; ALTER TABLE session_window ADD COLUMN right_preview_height INTEGER NOT NULL DEFAULT 0;")?;
    }
    if version < 9 {
        let old_rename = vec![Shortcut::new(false, false, false, false, Key::Function(2))];
        if stored_shortcuts(&tx, ActionId::RenameItem)? == old_rename {
            replace_shortcuts(&tx, ActionId::RenameItem, &default_shortcuts(ActionId::RenameItem))?;
        }
        let old_trash = vec![Shortcut::new(false, true, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::ForwardDelete)];
        if stored_shortcuts(&tx, ActionId::MoveToTrash)? == old_trash {
            replace_shortcuts(&tx, ActionId::MoveToTrash, &VERSION_NINE_TRASH)?;
        }
    }
    if version < 10 {
        // Command+Forward Delete joins an unchanged version 9 Move to Trash
        // list, unless the person already gave it to another action.
        let taken: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM action_shortcut WHERE action <> 'MoveToTrash' AND command = 1 AND shift = 0 AND option = 0 AND control = 0 AND key = 'Forward Delete')", [], |row| row.get(0))?;
        if !taken && stored_shortcuts(&tx, ActionId::MoveToTrash)? == VERSION_NINE_TRASH {
            replace_shortcuts(&tx, ActionId::MoveToTrash, &VERSION_TEN_TRASH)?;
        }
    }
    if version < 11 {
        tx.execute_batch("CREATE TABLE context_menu_action(menu_kind TEXT NOT NULL CHECK(menu_kind IN ('Normal', 'Comprehensive')), action TEXT NOT NULL, visible INTEGER NOT NULL CHECK(visible IN (0, 1)), PRIMARY KEY(menu_kind, action));")?;
        let defaults = ContextMenuPreferences::default();
        for kind in ContextMenuKind::ALL {
            for action in CONTEXT_MENU_ACTIONS {
                tx.execute("INSERT INTO context_menu_action(menu_kind, action, visible) VALUES (?1, ?2, ?3)", params![kind.as_str(), action.as_str(), i64::from(defaults.visible(kind, action))])?;
            }
        }
    }
    if version < 12 {
        tx.execute_batch("CREATE TABLE file_association_type(content_type TEXT PRIMARY KEY NOT NULL, pinned INTEGER NOT NULL CHECK(pinned IN (0, 1))); CREATE TABLE file_association_app(content_type TEXT NOT NULL REFERENCES file_association_type(content_type) ON DELETE CASCADE, menu_kind TEXT NOT NULL CHECK(menu_kind IN ('Normal', 'Comprehensive')), bundle_identifier TEXT NOT NULL, bundle_url TEXT NOT NULL, position INTEGER NOT NULL CHECK(position >= 0), PRIMARY KEY(content_type, menu_kind, bundle_identifier, bundle_url), UNIQUE(content_type, menu_kind, position));")?;
        let defaults = ContextMenuPreferences::default();
        for kind in ContextMenuKind::ALL {
            tx.execute("INSERT OR IGNORE INTO context_menu_action(menu_kind, action, visible) VALUES (?1, ?2, ?3)", params![kind.as_str(), ActionId::ConfigureOpenWith.as_str(), i64::from(defaults.visible(kind, ActionId::ConfigureOpenWith))])?;
        }
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    // Validate migrated records before committing the schema change. A
    // malformed old collection leaves the original database intact.
    SettingsDatabase::load_connection(&tx)?;
    tx.commit()?;
    Ok(())
}
