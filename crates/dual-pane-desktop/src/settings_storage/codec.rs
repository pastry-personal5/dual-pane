//! Byte and integer encodings shared by the tables.

use super::*;

pub(super) fn to_sql_error(error: SettingsStorageError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

/// Prefix each logical component with its big-endian length so keys are
/// unambiguous and preserve every non-UTF-8 byte.
pub(super) fn encode_location(location: &Location) -> Vec<u8> {
    let mut bytes = Vec::new();
    for component in location.components() {
        bytes.extend_from_slice(&(component.as_bytes().len() as u32).to_be_bytes());
        bytes.extend_from_slice(component.as_bytes());
    }
    bytes
}

pub(super) fn decode_location(bytes: &[u8]) -> Result<Location, SettingsStorageError> {
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
