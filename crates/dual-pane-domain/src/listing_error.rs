use crate::Location;

/// Why a directory could not be listed. Only categories that change what the
/// application or the person can do are distinguished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingErrorKind {
    ItemMissing,
    NotADirectory,
    PermissionDenied,
    PrivacyRestricted,
    /// The reader stopped unexpectedly. Details are intentionally not shown
    /// outside the driver boundary.
    Internal,
    Unknown,
}

/// A failed directory listing with the location it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingError {
    location: Location,
    kind: ListingErrorKind,
}

impl ListingError {
    pub fn new(location: Location, kind: ListingErrorKind) -> Self {
        Self { location, kind }
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn kind(&self) -> ListingErrorKind {
        self.kind
    }
}
