use crate::EntryName;

/// Exact entry identities selected in one tab. Ordering is insertion order and
/// has no presentation meaning.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection(Vec<EntryName>);

impl Selection {
    /// An empty selection, usable in constants.
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn selected(&self) -> Option<&EntryName> {
        self.0.first()
    }

    /// Selects `name` and reports whether the selection changed.
    pub fn select(&mut self, name: EntryName) -> bool {
        if self.0.len() == 1 && self.0.first() == Some(&name) {
            return false;
        }
        self.0 = vec![name];
        true
    }

    /// Clears the selection and reports whether it was non-empty.
    pub fn clear(&mut self) -> bool {
        let changed = !self.0.is_empty();
        self.0.clear();
        changed
    }

    pub fn entries(&self) -> &[EntryName] {
        &self.0
    }
    pub fn contains(&self, name: &EntryName) -> bool {
        self.0.contains(name)
    }
    pub fn toggle(&mut self, name: EntryName) -> bool {
        if let Some(index) = self.0.iter().position(|item| item == &name) {
            self.0.remove(index);
        } else {
            self.0.push(name);
        }
        true
    }
    pub fn replace(&mut self, entries: Vec<EntryName>) -> bool {
        if self.0 == entries {
            false
        } else {
            self.0 = entries;
            true
        }
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&EntryName) -> bool) -> bool {
        let old = self.0.len();
        self.0.retain(|entry| keep(entry));
        old != self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> EntryName {
        EntryName::new(text).unwrap()
    }

    #[test]
    fn one_exact_name_replaces_another() {
        let mut selection = Selection::default();
        assert!(selection.select(name("first")));
        assert!(!selection.select(name("first")));
        assert!(selection.select(name("second")));
        assert_eq!(selection.selected(), Some(&name("second")));
        assert!(selection.clear());
        assert!(!selection.clear());
    }
}
