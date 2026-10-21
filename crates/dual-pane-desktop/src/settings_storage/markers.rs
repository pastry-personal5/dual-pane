//! Boolean settings stored as markers.

use super::*;

pub(super) fn marker_bool(connection: &Connection, key: &str, default: bool) -> Result<bool, SettingsStorageError> {
    match connection.query_row("SELECT value FROM setting_marker WHERE key = ?1", [key], |row| row.get::<_, i64>(0)).optional()? {
        None => Ok(default),
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        Some(_) => Err(SettingsStorageError::InvalidData("Boolean setting")),
    }
}

pub(super) fn save(tx: &Transaction<'_>, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
    tx.execute("INSERT INTO setting_marker(key, value) VALUES ('favorites_initialized', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.favorites.initialized)])?;
    tx.execute("INSERT INTO setting_marker(key, value) VALUES ('hide_notices_at_startup', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.hide_notices_at_startup)])?;
    tx.execute("INSERT INTO setting_marker(key, value) VALUES ('skip_quit_confirmation', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.skip_quit_confirmation)])?;
    tx.execute("INSERT INTO setting_marker(key, value) VALUES ('show_macos_items', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.visibility.show_macos_items)])?;
    tx.execute("INSERT INTO setting_marker(key, value) VALUES ('show_dotfiles', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.visibility.show_dotfiles)])?;
    tx.execute("INSERT INTO setting_marker(key, value) VALUES ('show_macos_invisible', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [i64::from(snapshot.visibility.show_macos_invisible)])?;
    Ok(())
}
