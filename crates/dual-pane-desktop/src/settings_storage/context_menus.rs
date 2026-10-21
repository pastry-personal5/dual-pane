//! Context menu visibility, one row per menu kind and action.

use super::*;

pub(super) fn load(connection: &Connection) -> Result<ContextMenuPreferences, SettingsStorageError> {
    let stored_context_menus = connection.prepare("SELECT menu_kind, action, visible FROM context_menu_action ORDER BY menu_kind, action")?.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    if stored_context_menus.len() != CONTEXT_MENU_ACTIONS.len() * ContextMenuKind::ALL.len() {
        return Err(SettingsStorageError::InvalidData("context menu row count"));
    }
    let mut context_entries = CONTEXT_MENU_ACTIONS.into_iter().map(|action| ContextMenuPreference { action, normal: false, comprehensive: false }).collect::<Vec<_>>();
    let mut seen_context_rows = HashSet::new();
    for (kind, action, visible) in stored_context_menus {
        let kind = ContextMenuKind::parse(&kind).ok_or(SettingsStorageError::InvalidData("context menu kind"))?;
        let action = ActionId::parse(&action).filter(|action| CONTEXT_MENU_ACTIONS.contains(action)).ok_or(SettingsStorageError::InvalidData("context menu action"))?;
        if !seen_context_rows.insert((kind, action)) {
            return Err(SettingsStorageError::InvalidData("duplicate context menu row"));
        }
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
    Ok(context_menus)
}

pub(super) fn save(tx: &Transaction<'_>, context_menus: &ContextMenuPreferences) -> Result<(), SettingsStorageError> {
    tx.execute("DELETE FROM context_menu_action", [])?;
    for kind in ContextMenuKind::ALL {
        for action in CONTEXT_MENU_ACTIONS {
            tx.execute("INSERT INTO context_menu_action(menu_kind, action, visible) VALUES (?1, ?2, ?3)", params![kind.as_str(), action.as_str(), i64::from(context_menus.visible(kind, action))])?;
        }
    }
    Ok(())
}
