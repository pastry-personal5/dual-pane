//! Durable Dual Pane file-association menu choices. macOS remains the
//! authority for candidates and defaults.

use super::*;

pub(super) fn load(connection: &Connection) -> Result<FileAssociationPreferences, SettingsStorageError> {
    let types = connection.prepare("SELECT content_type, pinned FROM file_association_type ORDER BY content_type")?.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    let apps = connection.prepare("SELECT content_type, menu_kind, bundle_identifier, bundle_url, position FROM file_association_app ORDER BY content_type, menu_kind, position")?.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?)))?.collect::<Result<Vec<_>, _>>()?;
    let mut entries = Vec::with_capacity(types.len());
    for (content_type, pinned) in types {
        if !matches!(pinned, 0 | 1) {
            return Err(SettingsStorageError::InvalidData("file association pin"));
        }
        entries.push(FileAssociationPreference { content_type, pinned: pinned == 1, normal: Vec::new(), comprehensive: Vec::new() });
    }
    let mut positions: HashSet<(String, ContextMenuKind, i64)> = HashSet::new();
    for (content_type, menu_kind, bundle_identifier, bundle_url, position) in apps {
        let kind = ContextMenuKind::parse(&menu_kind).ok_or(SettingsStorageError::InvalidData("file association menu kind"))?;
        if position < 0 || !positions.insert((content_type.clone(), kind, position)) {
            return Err(SettingsStorageError::InvalidData("file association app position"));
        }
        let entry = entries.iter_mut().find(|entry| entry.content_type == content_type).ok_or(SettingsStorageError::InvalidData("file association parent"))?;
        let application = ApplicationIdentity { bundle_identifier, bundle_url };
        match kind {
            ContextMenuKind::Normal => entry.normal.push(application),
            ContextMenuKind::Comprehensive => entry.comprehensive.push(application),
        }
    }
    FileAssociationPreferences::from_entries(entries).map_err(|_| SettingsStorageError::InvalidData("file association configuration"))
}

pub(super) fn save(tx: &Transaction<'_>, preferences: &FileAssociationPreferences) -> Result<(), SettingsStorageError> {
    tx.execute("DELETE FROM file_association_app", [])?;
    tx.execute("DELETE FROM file_association_type", [])?;
    for entry in preferences.entries() {
        tx.execute("INSERT INTO file_association_type(content_type, pinned) VALUES (?1, ?2)", params![entry.content_type, i64::from(entry.pinned)])?;
        for (kind, applications) in [(ContextMenuKind::Normal, &entry.normal), (ContextMenuKind::Comprehensive, &entry.comprehensive)] {
            for (position, application) in applications.iter().enumerate() {
                tx.execute("INSERT INTO file_association_app(content_type, menu_kind, bundle_identifier, bundle_url, position) VALUES (?1, ?2, ?3, ?4, ?5)", params![entry.content_type, kind.as_str(), application.bundle_identifier, application.bundle_url, i64::try_from(position).map_err(|_| SettingsStorageError::InvalidData("file association app position"))?])?;
            }
        }
    }
    Ok(())
}
