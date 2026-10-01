use crate::Location;

/// A stable, durable Favorite Group identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FavoriteGroupId(i64);

impl FavoriteGroupId {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }
    pub const fn value(self) -> i64 {
        self.0
    }
}

/// A stable, durable Favorite Item identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FavoriteItemId(i64);

impl FavoriteItemId {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }
    pub const fn value(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteGroup {
    pub id: FavoriteGroupId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteItem {
    pub id: FavoriteItemId,
    pub group_id: FavoriteGroupId,
    pub name: String,
    pub target: Location,
}

/// A one-level, ordered Favorites hierarchy. Its constructors enforce the
/// product's exact-text naming and membership rules without performing I/O.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Favorites {
    groups: Vec<FavoriteGroup>,
    items: Vec<FavoriteItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteError {
    InvalidName,
    DuplicateName,
    MissingGroup,
    MissingItem,
}

impl Favorites {
    pub fn new(groups: Vec<FavoriteGroup>, items: Vec<FavoriteItem>) -> Result<Self, FavoriteError> {
        let favorites = Self { groups, items };
        favorites.validate()?;
        Ok(favorites)
    }
    pub fn groups(&self) -> &[FavoriteGroup] {
        &self.groups
    }
    pub fn items(&self) -> &[FavoriteItem] {
        &self.items
    }
    pub fn validate(&self) -> Result<(), FavoriteError> {
        for (index, group) in self.groups.iter().enumerate() {
            if invalid_name(&group.name) {
                return Err(FavoriteError::InvalidName);
            }
            if self.groups[..index].iter().any(|other| other.name == group.name || other.id == group.id) {
                return Err(FavoriteError::DuplicateName);
            }
        }
        for (index, item) in self.items.iter().enumerate() {
            if invalid_name(&item.name) || !self.groups.iter().any(|group| group.id == item.group_id) {
                return Err(if invalid_name(&item.name) { FavoriteError::InvalidName } else { FavoriteError::MissingGroup });
            }
            if self.items[..index].iter().any(|other| other.id == item.id || (other.group_id == item.group_id && other.name == item.name)) {
                return Err(FavoriteError::DuplicateName);
            }
        }
        Ok(())
    }
}

pub fn valid_favorite_name(name: &str) -> bool {
    !name.trim().is_empty()
}
fn invalid_name(name: &str) -> bool {
    !valid_favorite_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_and_exact_duplicate_names() {
        let group = FavoriteGroup { id: FavoriteGroupId::new(1), name: "A".into() };
        assert!(Favorites::new(vec![group.clone(), FavoriteGroup { id: FavoriteGroupId::new(2), name: "A".into() }], vec![]).is_err());
        assert!(Favorites::new(vec![group], vec![FavoriteItem { id: FavoriteItemId::new(1), group_id: FavoriteGroupId::new(1), name: " ".into(), target: Location::root() }]).is_err());
    }
}
