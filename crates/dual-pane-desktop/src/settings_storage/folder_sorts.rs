//! Per-folder sort choices.

use super::*;

pub(super) fn load(connection: &Connection) -> Result<Vec<(Location, SortSpec)>, SettingsStorageError> {
    let mut statement = connection.prepare("SELECT target, field, direction FROM folder_sort ORDER BY recent ASC")?;
    let folder_sorts = statement.query_map([], |row| Ok((decode_location(&row.get::<_, Vec<u8>>(0)?).map_err(to_sql_error)?, decode_sort(row.get(1)?, row.get(2)?).map_err(to_sql_error)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(folder_sorts)
}

pub(super) fn save(tx: &Transaction<'_>, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
    tx.execute("DELETE FROM folder_sort", [])?;
    for (recent, (location, sort)) in snapshot.folder_sorts.iter().enumerate() {
        tx.execute("INSERT INTO folder_sort(target, field, direction, recent) VALUES (?1, ?2, ?3, ?4)", params![encode_location(location), sort_field(sort.field()), sort_direction(sort.direction()), recent as i64])?;
    }
    Ok(())
}

pub(super) fn sort_field(field: SortField) -> i64 {
    match field {
        SortField::Name => 0,
        SortField::Type => 1,
        SortField::Modified => 2,
        SortField::Size => 3,
    }
}

pub(super) fn sort_direction(direction: SortDirection) -> i64 {
    match direction {
        SortDirection::Ascending => 0,
        SortDirection::Descending => 1,
    }
}

pub(super) fn decode_sort(field: i64, direction: i64) -> Result<SortSpec, SettingsStorageError> {
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
