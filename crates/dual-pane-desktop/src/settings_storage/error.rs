//! Storage failures and their conversions.

use super::*;

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
