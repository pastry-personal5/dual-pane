//! Favorite groups and items.

use super::*;

pub(super) fn load(connection: &Connection, initialized: bool) -> Result<FavoritesRecords, SettingsStorageError> {
    let groups = connection.prepare("SELECT id, name, position FROM favorite_group ORDER BY position, id")?.query_map([], |row| Ok(FavoriteGroupRecord { id: row.get(0)?, name: row.get(1)?, position: row.get(2)? }))?.collect::<Result<Vec<_>, _>>()?;
    let items = connection.prepare("SELECT id, group_id, name, target, position FROM favorite_item ORDER BY group_id, position, id")?.query_map([], |row| Ok(FavoriteItemRecord { id: row.get(0)?, group_id: row.get(1)?, name: row.get(2)?, target: decode_location(&row.get::<_, Vec<u8>>(3)?).map_err(to_sql_error)?, position: row.get(4)? }))?.collect::<Result<Vec<_>, _>>()?;
    let favorites = FavoritesRecords { initialized, groups, items };
    favorites.hierarchy().map_err(|_| SettingsStorageError::InvalidData("Favorites hierarchy"))?;
    Ok(favorites)
}

pub(super) fn clear(tx: &Transaction<'_>) -> Result<(), SettingsStorageError> {
    tx.execute("DELETE FROM favorite_item", [])?;
    tx.execute("DELETE FROM favorite_group", [])?;
    Ok(())
}

pub(super) fn insert(tx: &Transaction<'_>, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
    for group in &snapshot.favorites.groups {
        tx.execute("INSERT INTO favorite_group(id, name, position) VALUES (?1, ?2, ?3)", params![group.id, group.name, group.position])?;
    }
    for item in &snapshot.favorites.items {
        tx.execute("INSERT INTO favorite_item(id, group_id, name, target, position) VALUES (?1, ?2, ?3, ?4, ?5)", params![item.id, item.group_id, item.name, encode_location(&item.target), item.position])?;
    }
    Ok(())
}
