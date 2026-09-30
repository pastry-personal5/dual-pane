use crate::EntryName;

/// The exact entry selected in one pane, if any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection(Option<EntryName>);

impl Selection {
    pub fn selected(&self) -> Option<&EntryName> {
        self.0.as_ref()
    }

    /// Selects `name` and reports whether the selection changed.
    pub fn select(&mut self, name: EntryName) -> bool {
        if self.0.as_ref() == Some(&name) {
            return false;
        }
        self.0 = Some(name);
        true
    }

    /// Clears the selection and reports whether it was non-empty.
    pub fn clear(&mut self) -> bool {
        self.0.take().is_some()
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
