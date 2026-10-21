//! SQLite-backed settings driver. This module owns paths, SQLite, and worker
//! scheduling; it does not know about Qt objects or widgets.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dual_pane_application::{ActionBinding, ActionId, ActionShortcuts, BrowserSnapshot, CONTEXT_MENU_ACTIONS, ContextMenuKind, ContextMenuPreference, ContextMenuPreferences, FavoriteGroupRecord, FavoriteItemRecord, FavoritesRecords, Key, LoadedSettings, SessionSnapshot, SettingsFailure, SettingsSnapshot, Shortcut, VisibilityPolicy, WindowFrame, WindowLayout, WindowState, WorkspaceSnapshot, default_shortcuts};
use dual_pane_domain::{BrowserSide, EntryName, Location, SortDirection, SortField, SortSpec};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};

const SCHEMA_VERSION: i64 = 11;
/// Move to Trash factory lists as the migrations that introduced them wrote
/// them, so a later default never changes what an earlier step writes.
const VERSION_NINE_TRASH: [Shortcut; 3] = [Shortcut::new(true, false, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::ForwardDelete)];
const VERSION_TEN_TRASH: [Shortcut; 4] = [Shortcut::new(true, false, false, false, Key::Delete), Shortcut::new(true, false, false, false, Key::ForwardDelete), Shortcut::new(false, true, false, false, Key::Delete), Shortcut::new(false, true, false, false, Key::ForwardDelete)];
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
        migrate(&mut connection)?;
        let mut database = Self { connection };
        // A damaged existing database must remain untouched until the person
        // explicitly resets it. Validate before changing its journal mode or
        // inserting newly catalogued actions.
        database.load()?;
        database.connection.pragma_update(None, "journal_mode", "WAL")?;
        preload_bindings(&mut database.connection)?;
        Ok(database)
    }
    pub fn load(&self) -> Result<SettingsSnapshot, SettingsStorageError> {
        Ok(self.load_with_session()?.snapshot)
    }
    pub fn load_with_session(&self) -> Result<LoadedSettings, SettingsStorageError> {
        Self::load_connection(&self.connection)
    }
    fn load_connection(connection: &Connection) -> Result<LoadedSettings, SettingsStorageError> {
        let bindings: Vec<ActionBinding> = ActionId::ALL
            .into_iter()
            .map(|action| {
                connection
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
        let mut statement = connection.prepare("SELECT target, field, direction FROM folder_sort ORDER BY recent ASC")?;
        let folder_sorts = statement.query_map([], |row| Ok((decode_location(&row.get::<_, Vec<u8>>(0)?).map_err(to_sql_error)?, decode_sort(row.get(1)?, row.get(2)?).map_err(to_sql_error)?)))?.collect::<Result<Vec<_>, _>>()?;
        let initialized = marker_bool(connection, "favorites_initialized", false)?;
        let hide_notices_at_startup = marker_bool(connection, "hide_notices_at_startup", false)?;
        let skip_quit_confirmation = marker_bool(connection, "skip_quit_confirmation", false)?;
        let visibility = VisibilityPolicy { show_macos_items: marker_bool(connection, "show_macos_items", false)?, show_dotfiles: marker_bool(connection, "show_dotfiles", true)?, show_macos_invisible: marker_bool(connection, "show_macos_invisible", false)? };
        let groups = connection.prepare("SELECT id, name, position FROM favorite_group ORDER BY position, id")?.query_map([], |row| Ok(FavoriteGroupRecord { id: row.get(0)?, name: row.get(1)?, position: row.get(2)? }))?.collect::<Result<Vec<_>, _>>()?;
        let items = connection.prepare("SELECT id, group_id, name, target, position FROM favorite_item ORDER BY group_id, position, id")?.query_map([], |row| Ok(FavoriteItemRecord { id: row.get(0)?, group_id: row.get(1)?, name: row.get(2)?, target: decode_location(&row.get::<_, Vec<u8>>(3)?).map_err(to_sql_error)?, position: row.get(4)? }))?.collect::<Result<Vec<_>, _>>()?;
        let favorites = FavoritesRecords { initialized, groups, items };
        favorites.hierarchy().map_err(|_| SettingsStorageError::InvalidData("Favorites hierarchy"))?;
        let stored_context_menus = connection.prepare("SELECT menu_kind, action, visible FROM context_menu_action ORDER BY menu_kind, action")?.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
        if stored_context_menus.len() != CONTEXT_MENU_ACTIONS.len() * ContextMenuKind::ALL.len() {
            return Err(SettingsStorageError::InvalidData("context menu row count"));
        }
        let mut context_entries = CONTEXT_MENU_ACTIONS.into_iter().map(|action| ContextMenuPreference { action, normal: false, comprehensive: false }).collect::<Vec<_>>();
        for (kind, action, visible) in stored_context_menus {
            let kind = ContextMenuKind::parse(&kind).ok_or(SettingsStorageError::InvalidData("context menu kind"))?;
            let action = ActionId::parse(&action).filter(|action| CONTEXT_MENU_ACTIONS.contains(action)).ok_or(SettingsStorageError::InvalidData("context menu action"))?;
            if !matches!(visible, 0 | 1) {
                return Err(SettingsStorageError::InvalidData("context menu visibility"));
            }
            let Some(entry) = context_entries.iter_mut().find(|entry| entry.action == action) else { return Err(SettingsStorageError::InvalidData("context menu action")) };
            match kind {
                ContextMenuKind::Normal => entry.normal = visible == 1,
                ContextMenuKind::Comprehensive => entry.comprehensive = visible == 1,
            }
        }
        let context_menus = ContextMenuPreferences::from_entries(context_entries).map_err(|_| SettingsStorageError::InvalidData("context menu configuration"))?;
        let mut shortcut_lists = Vec::new();
        for binding in &bindings {
            let mut statement = connection.prepare("SELECT command, shift, option, control, key FROM action_shortcut WHERE action = ?1 ORDER BY position")?;
            let entries = statement
                .query_map([binding.action.as_str()], |row| {
                    let text: String = row.get(4)?;
                    let command = row.get::<_, i64>(0)? != 0;
                    let shift = row.get::<_, i64>(1)? != 0;
                    let option = row.get::<_, i64>(2)? != 0;
                    let control = row.get::<_, i64>(3)? != 0;
                    Ok(Key::parse(&text).map(|key| Shortcut::new(command, shift, option, control, key)))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if entries.iter().all(Option::is_some) && (!entries.is_empty() || binding.shortcut.is_none()) {
                shortcut_lists.push(ActionShortcuts { action: binding.action, shortcuts: entries.into_iter().flatten().collect() });
            }
        }
        let snapshot = SettingsSnapshot { bindings, shortcut_lists, folder_sorts, favorites, hide_notices_at_startup, skip_quit_confirmation, visibility, context_menus };
        let session = read_session(connection)?;
        Ok(LoadedSettings { snapshot, session })
    }
    pub fn save(&mut self, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
        snapshot.favorites.hierarchy().map_err(|_| SettingsStorageError::InvalidData("Favorites hierarchy"))?;
        let context_menus = ContextMenuPreferences::from_entries(snapshot.context_menus.entries().to_vec()).map_err(|_| SettingsStorageError::InvalidData("context menu configuration"))?;
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM action_shortcut", [])?;
        tx.execute("DELETE FROM action_binding", [])?;
        let lists = if snapshot.shortcut_lists.is_empty() { snapshot.bindings.iter().map(|binding| ActionShortcuts { action: binding.action, shortcuts: binding.shortcut.into_iter().collect() }).collect::<Vec<_>>() } else { snapshot.shortcut_lists.clone() };
        for action in ActionId::ALL {
            let shortcuts = lists.iter().find(|list| list.action == action).map(|list| list.shortcuts.as_slice()).unwrap_or(&[]);
            let first = ActionBinding { action, shortcut: shortcuts.first().copied().or_else(|| (!lists.iter().any(|list| list.action == action)).then(|| default_shortcuts(action).first().copied()).flatten()) };
            insert_binding(&tx, &first)?;
            for (position, shortcut) in shortcuts.iter().enumerate() {
                insert_shortcut(&tx, action, position, *shortcut)?;
            }
        }
        tx.execute("DELETE FROM folder_sort", [])?;
        for (recent, (location, sort)) in snapshot.folder_sorts.iter().enumerate() {
            tx.execute("INSERT INTO folder_sort(target, field, direction, recent) VALUES (?1, ?2, ?3, ?4)", params![encode_location(location), sort_field(sort.field()), sort_direction(sort.direction()), recent as i64])?;
        }
        tx.execute("DELETE FROM favorite_item", [])?;
        tx.execute("DELETE FROM favorite_group", [])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('favorites_initialized', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.favorites.initialized)])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('hide_notices_at_startup', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.hide_notices_at_startup)])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('skip_quit_confirmation', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.skip_quit_confirmation)])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('show_macos_items', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.visibility.show_macos_items)])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('show_dotfiles', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.visibility.show_dotfiles)])?;
        tx.execute("INSERT INTO setting_marker(key, value) VALUES ('show_macos_invisible', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.visibility.show_macos_invisible)])?;
        for group in &snapshot.favorites.groups {
            tx.execute("INSERT INTO favorite_group(id, name, position) VALUES (?1, ?2, ?3)", params![group.id, group.name, group.position])?;
        }
        for item in &snapshot.favorites.items {
            tx.execute("INSERT INTO favorite_item(id, group_id, name, target, position) VALUES (?1, ?2, ?3, ?4, ?5)", params![item.id, item.group_id, item.name, encode_location(&item.target), item.position])?;
        }
        tx.execute("DELETE FROM context_menu_action", [])?;
        for kind in ContextMenuKind::ALL {
            for action in CONTEXT_MENU_ACTIONS {
                tx.execute("INSERT INTO context_menu_action(menu_kind, action, visible) VALUES (?1, ?2, ?3)", params![kind.as_str(), action.as_str(), i64::from(context_menus.visible(kind, action))])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn save_session(&mut self, session: &WorkspaceSnapshot) -> Result<(), SettingsStorageError> {
        if !session.layout.valid() {
            return Err(SettingsStorageError::InvalidData("window layout"));
        }
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM session_tab", [])?;
        tx.execute("DELETE FROM session_browser", [])?;
        tx.execute("DELETE FROM session_window", [])?;
        for (number, browser) in [(0_i64, &session.left), (1, &session.right)] {
            let active = browser.active_tab.ok_or(SettingsStorageError::InvalidData("active tab"))?;
            if browser.tabs.is_empty() || browser.tabs.len() > 8 || active >= browser.tabs.len() {
                return Err(SettingsStorageError::InvalidData("browser tabs"));
            }
            tx.execute("INSERT INTO session_browser(browser, active_position, active) VALUES (?1, ?2, ?3)", params![number, active as i64, i64::from(session.active_browser == if number == 0 { BrowserSide::Left } else { BrowserSide::Right })])?;
            for (position, tab) in browser.tabs.iter().enumerate() {
                tx.execute("INSERT INTO session_tab(browser, position, location) VALUES (?1, ?2, ?3)", params![number, position as i64, encode_location(&tab.location)])?;
            }
        }
        let layout = session.layout;
        tx.execute("INSERT INTO session_window(id, x, y, width, height, state, sidebar_splitter, browser_splitter, left_preview_height, right_preview_height) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", params![layout.frame.x, layout.frame.y, layout.frame.width, layout.frame.height, encode_window_state(layout.state), layout.sidebar_splitter, layout.browser_splitter, layout.left_preview_height, layout.right_preview_height])?;
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
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    // Validate migrated records before committing the schema change. A
    // malformed old collection leaves the original database intact.
    SettingsDatabase::load_connection(&tx)?;
    tx.commit()?;
    Ok(())
}
fn marker_bool(connection: &Connection, key: &str, default: bool) -> Result<bool, SettingsStorageError> {
    match connection.query_row("SELECT value FROM setting_marker WHERE key = ?1", [key], |row| row.get::<_, i64>(0)).optional()? {
        None => Ok(default),
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        Some(_) => Err(SettingsStorageError::InvalidData("Boolean setting")),
    }
}
fn preload_bindings(connection: &mut Connection) -> Result<(), SettingsStorageError> {
    let tx = connection.transaction()?;
    for binding in dual_pane_application::default_bindings() {
        let inserted = tx.execute("INSERT OR IGNORE INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", binding_params(&binding))?;
        if inserted != 0 {
            for (position, shortcut) in default_shortcuts(binding.action).into_iter().enumerate() {
                insert_shortcut(&tx, binding.action, position, shortcut)?;
            }
        }
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
fn insert_shortcut(tx: &Transaction<'_>, action: ActionId, position: usize, shortcut: Shortcut) -> Result<(), rusqlite::Error> {
    tx.execute("INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![action.as_str(), position as i64, i64::from(shortcut.command), i64::from(shortcut.shift), i64::from(shortcut.option), i64::from(shortcut.control), shortcut.key.as_text()])?;
    Ok(())
}

fn stored_shortcuts(connection: &Connection, action: ActionId) -> Result<Vec<Shortcut>, rusqlite::Error> {
    let mut statement = connection.prepare("SELECT command, shift, option, control, key FROM action_shortcut WHERE action = ?1 ORDER BY position")?;
    statement
        .query_map([action.as_str()], |row| {
            let key: String = row.get(4)?;
            let command = row.get::<_, i64>(0)? != 0;
            let shift = row.get::<_, i64>(1)? != 0;
            let option = row.get::<_, i64>(2)? != 0;
            let control = row.get::<_, i64>(3)? != 0;
            Ok(Key::parse(&key).map(|key| Shortcut::new(command, shift, option, control, key)))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map(|shortcuts| shortcuts.into_iter().flatten().collect())
}

fn replace_shortcuts(tx: &Transaction<'_>, action: ActionId, shortcuts: &[Shortcut]) -> Result<(), rusqlite::Error> {
    tx.execute("DELETE FROM action_shortcut WHERE action = ?1", [action.as_str()])?;
    for (position, shortcut) in shortcuts.iter().enumerate() {
        insert_shortcut(tx, action, position, *shortcut)?;
    }
    let first = shortcuts.first().copied();
    tx.execute("UPDATE action_binding SET command = ?2, shift = ?3, option = ?4, control = ?5, key = ?6 WHERE action = ?1", params![action.as_str(), first.map(|value| i64::from(value.command)), first.map(|value| i64::from(value.shift)), first.map(|value| i64::from(value.option)), first.map(|value| i64::from(value.control)), first.map(|value| value.key.as_text())])?;
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

/// Session corruption is deliberately non-fatal: settings and Favorites can
/// still load, and the next valid session save replaces these rows.
fn read_session(connection: &Connection) -> Result<SessionSnapshot, SettingsStorageError> {
    let browser_rows = connection.prepare("SELECT browser, active_position, active FROM session_browser ORDER BY browser")?.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let tab_rows = connection.prepare("SELECT browser, position, location FROM session_tab ORDER BY browser, position")?.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let layout_rows = connection.prepare("SELECT x, y, width, height, state, sidebar_splitter, browser_splitter, left_preview_height, right_preview_height FROM session_window")?.query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?, row.get::<_, i32>(2)?, row.get::<_, i32>(3)?, row.get::<_, i64>(4)?, row.get::<_, i32>(5)?, row.get::<_, i32>(6)?, row.get::<_, rusqlite::types::Value>(7)?, row.get::<_, rusqlite::types::Value>(8)?)))?.collect::<Result<Vec<_>, _>>()?;
    if browser_rows.is_empty() && tab_rows.is_empty() && layout_rows.is_empty() {
        return Ok(SessionSnapshot::Absent);
    }
    let damaged = || Ok(SessionSnapshot::Damaged);
    if browser_rows.len() != 2 || layout_rows.len() != 1 {
        return damaged();
    }
    let layout_row = &layout_rows[0];
    let Some(state) = decode_window_state(layout_row.4) else { return damaged() };
    let preview_height = |value: &rusqlite::types::Value| match value {
        rusqlite::types::Value::Integer(height) if (0..=32_767).contains(height) => *height as i32,
        _ => 0,
    };
    let layout = WindowLayout { frame: WindowFrame { x: layout_row.0, y: layout_row.1, width: layout_row.2, height: layout_row.3 }, state, sidebar_splitter: layout_row.5, browser_splitter: layout_row.6, left_preview_height: preview_height(&layout_row.7), right_preview_height: preview_height(&layout_row.8) };
    if !layout.valid() {
        return damaged();
    }
    let mut browsers = Vec::new();
    let mut active_browser = None;
    for (expected, (number, active_position, active)) in browser_rows.into_iter().enumerate() {
        if number != expected as i64 || !(0..=1).contains(&active) || (active == 1 && active_browser.replace(number).is_some()) {
            return damaged();
        }
        let rows = tab_rows.iter().filter(|(browser, _, _)| *browser == number).collect::<Vec<_>>();
        if rows.is_empty() || rows.len() > 8 || active_position < 0 || active_position as usize >= rows.len() || rows.iter().enumerate().any(|(position, (_, stored, _))| *stored != position as i64) {
            return damaged();
        }
        let locations = rows.into_iter().map(|(_, _, bytes)| decode_location(bytes)).collect::<Result<Vec<_>, _>>();
        let Ok(locations) = locations else { return damaged() };
        browsers.push(BrowserSnapshot { tabs: locations.into_iter().map(|location| dual_pane_application::TabSnapshot { location }).collect(), active_tab: Some(active_position as usize) });
    }
    if tab_rows.iter().any(|(browser, _, _)| *browser != 0 && *browser != 1) {
        return damaged();
    }
    let Some(active_browser) = active_browser else { return damaged() };
    Ok(SessionSnapshot::Saved(WorkspaceSnapshot { left: browsers.remove(0), right: browsers.remove(0), active_browser: if active_browser == 0 { BrowserSide::Left } else { BrowserSide::Right }, layout }))
}
fn encode_window_state(state: WindowState) -> i64 {
    match state {
        WindowState::Normal => 0,
        WindowState::Zoomed => 1,
        WindowState::FullScreen => 2,
    }
}
fn decode_window_state(value: i64) -> Option<WindowState> {
    match value {
        0 => Some(WindowState::Normal),
        1 => Some(WindowState::Zoomed),
        2 => Some(WindowState::FullScreen),
        _ => None,
    }
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
    SaveSession {
        revision: u64,
        session: WorkspaceSnapshot,
    },
    /// Saves this snapshot without waiting for more changes, then stops.
    Quit {
        revision: u64,
        snapshot: SettingsSnapshot,
    },
    /// Flushes independent settings and workspace snapshots before stopping.
    QuitWithSession {
        settings: Option<(u64, SettingsSnapshot)>,
        session: Option<(u64, WorkspaceSnapshot)>,
    },
    Reset,
}
pub enum SettingsResult {
    Loaded(Box<LoadedSettings>),
    LoadFailed(SettingsStorageError),
    Saved { revision: u64 },
    SaveFailed { revision: u64, error: SettingsStorageError },
    SessionSaved { revision: u64 },
    SessionSaveFailed { revision: u64, error: SettingsStorageError },
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
        self.shutdown_with_session(final_save, None, timeout)
    }
    pub fn shutdown_with_session(self, final_settings: Option<(u64, SettingsSnapshot)>, final_session: Option<(u64, WorkspaceSnapshot)>, timeout: Duration) -> Vec<SettingsResult> {
        let Self { jobs, results } = self;
        if final_settings.is_some() || final_session.is_some() {
            jobs.send(SettingsJob::QuitWithSession { settings: final_settings, session: final_session }).ok();
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
    fn save_session(&mut self, revision: u64, session: &WorkspaceSnapshot) -> SettingsResult {
        self.database().and_then(|database| database.save_session(session)).map_or_else(|error| SettingsResult::SessionSaveFailed { revision, error }, |()| SettingsResult::SessionSaved { revision })
    }
}
fn worker_loop(mut state: WorkerState, jobs: Receiver<SettingsJob>, results: Sender<SettingsResult>, wake: Box<dyn Fn() + Send>) {
    let send = |result| {
        results.send(result).ok();
        wake();
    };
    while let Ok(job) = jobs.recv() {
        let mut latest = None;
        let mut latest_session = None;
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
                    if let Some((revision, session)) = latest_session.take() {
                        send(state.save_session(revision, &session));
                    }
                    send(state.database().and_then(|database| database.load_with_session()).map_or_else(SettingsResult::LoadFailed, |loaded| SettingsResult::Loaded(Box::new(loaded))));
                }
                SettingsJob::Save { revision, snapshot } => latest = Some((revision, snapshot)),
                SettingsJob::SaveSession { revision, session } => latest_session = Some((revision, session)),
                SettingsJob::Quit { revision, snapshot } => {
                    latest = Some((revision, snapshot));
                    quitting = true;
                }
                SettingsJob::QuitWithSession { settings, session } => {
                    if let Some(settings) = settings {
                        latest = Some(settings)
                    }
                    if let Some(session) = session {
                        latest_session = Some(session)
                    }
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
                            latest_session = None;
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
        if let Some((revision, session)) = latest_session {
            send(state.save_session(revision, &session));
        }
        if quitting {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
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
}
