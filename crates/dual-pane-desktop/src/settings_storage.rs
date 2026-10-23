//! SQLite-backed settings driver. This module owns paths, SQLite, and worker
//! scheduling; it does not know about Qt objects or widgets.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dual_pane_application::{ActionBinding, ActionId, ActionShortcuts, ApplicationIdentity, BrowserSnapshot, CONTEXT_MENU_ACTIONS, ContextMenuKind, ContextMenuPreference, ContextMenuPreferences, FavoriteGroupRecord, FavoriteItemRecord, FavoritesRecords, FileAssociationPreference, FileAssociationPreferences, Key, LoadedSettings, SessionSnapshot, SettingsFailure, SettingsSnapshot, Shortcut, VisibilityPolicy, WindowFrame, WindowLayout, WindowState, WorkspaceSnapshot, default_shortcuts};
use dual_pane_domain::{BrowserSide, EntryName, Location, SortDirection, SortField, SortSpec};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};

mod codec;
mod context_menus;
mod error;
mod favorites;
mod file_associations;
mod folder_sorts;
mod markers;
mod migration;
mod reset;
mod session;
mod shortcuts;
#[cfg(test)]
mod tests;
mod worker;

pub use error::SettingsStorageError;
pub use reset::reset_database;
pub use worker::{SettingsJob, SettingsResult, SettingsWorker};

use codec::{decode_location, encode_location, to_sql_error};
use markers::marker_bool;
use migration::migrate;
use session::read_session;
use shortcuts::preload_bindings;

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
        let bindings = shortcuts::load_bindings(connection)?;
        let folder_sorts = folder_sorts::load(connection)?;
        let initialized = marker_bool(connection, "favorites_initialized", false)?;
        let hide_notices_at_startup = marker_bool(connection, "hide_notices_at_startup", false)?;
        let skip_quit_confirmation = marker_bool(connection, "skip_quit_confirmation", false)?;
        let visibility = VisibilityPolicy { show_macos_items: marker_bool(connection, "show_macos_items", false)?, show_dotfiles: marker_bool(connection, "show_dotfiles", true)?, show_macos_invisible: marker_bool(connection, "show_macos_invisible", false)? };
        let favorites = favorites::load(connection, initialized)?;
        let context_menus = context_menus::load(connection)?;
        let file_associations = file_associations::load(connection)?;
        let shortcut_lists = shortcuts::load_lists(connection, &bindings)?;
        let snapshot = SettingsSnapshot { bindings, shortcut_lists, folder_sorts, favorites, hide_notices_at_startup, skip_quit_confirmation, visibility, context_menus, file_associations };
        let session = read_session(connection)?;
        Ok(LoadedSettings { snapshot, session })
    }
    pub fn save(&mut self, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
        snapshot.favorites.hierarchy().map_err(|_| SettingsStorageError::InvalidData("Favorites hierarchy"))?;
        let context_menus = ContextMenuPreferences::from_entries(snapshot.context_menus.entries().to_vec()).map_err(|_| SettingsStorageError::InvalidData("context menu configuration"))?;
        let file_associations = FileAssociationPreferences::from_entries(snapshot.file_associations.entries().to_vec()).map_err(|_| SettingsStorageError::InvalidData("file association configuration"))?;
        let tx = self.connection.transaction()?;
        shortcuts::save(&tx, snapshot)?;
        folder_sorts::save(&tx, snapshot)?;
        favorites::clear(&tx)?;
        markers::save(&tx, snapshot)?;
        favorites::insert(&tx, snapshot)?;
        context_menus::save(&tx, &context_menus)?;
        file_associations::save(&tx, &file_associations)?;
        tx.commit()?;
        Ok(())
    }
    pub fn save_session(&mut self, session: &WorkspaceSnapshot) -> Result<(), SettingsStorageError> {
        session::save(&mut self.connection, session)
    }
}

pub fn application_support_database_path(home: &Path) -> PathBuf {
    home.join("Library").join("Application Support").join("Dual Pane").join("settings.sqlite3")
}
