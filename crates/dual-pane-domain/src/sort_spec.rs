/// A field by which Folder Items can be ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortField {
    Name,
    Type,
    Modified,
    Size,
}

/// The direction for a Folder Items ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// A complete, location-shared Folder Items ordering choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SortSpec {
    field: SortField,
    direction: SortDirection,
}

impl SortSpec {
    pub const fn new(field: SortField, direction: SortDirection) -> Self {
        Self { field, direction }
    }

    pub const fn field(self) -> SortField {
        self.field
    }

    pub const fn direction(self) -> SortDirection {
        self.direction
    }

    /// The natural existing Folder Items order.
    pub const fn name_ascending() -> Self {
        Self::new(SortField::Name, SortDirection::Ascending)
    }
}

impl Default for SortSpec {
    fn default() -> Self {
        Self::name_ascending()
    }
}
