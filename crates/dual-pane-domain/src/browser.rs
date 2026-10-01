use crate::TabId;

/// Ordered, nonempty tab identity state for one Browser. Listing and history
/// data live in the application layer; this value protects structural tab
/// invariants independently of the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTabs {
    tabs: Vec<TabId>,
    active: TabId,
}

impl BrowserTabs {
    pub fn new(initial: TabId) -> Self {
        Self { tabs: vec![initial], active: initial }
    }
    pub fn active(&self) -> TabId {
        self.active
    }
    pub fn tabs(&self) -> &[TabId] {
        &self.tabs
    }
    pub fn activate(&mut self, tab: TabId) -> bool {
        if self.tabs.contains(&tab) && self.active != tab {
            self.active = tab;
            true
        } else {
            false
        }
    }
    pub fn insert_active(&mut self, tab: TabId) {
        self.tabs.push(tab);
        self.active = tab;
    }
    pub fn reorder(&mut self, tab: TabId, position: usize) -> bool {
        let Some(index) = self.tabs.iter().position(|item| *item == tab) else { return false };
        let tab = self.tabs.remove(index);
        let position = position.min(self.tabs.len());
        self.tabs.insert(position, tab);
        true
    }
    /// Removes a nonfinal tab and returns the tab that becomes active when the
    /// removed tab was active. The final tab must be replaced by its caller.
    pub fn close(&mut self, tab: TabId) -> Option<TabId> {
        if self.tabs.len() == 1 {
            return None;
        }
        let index = self.tabs.iter().position(|item| *item == tab)?;
        self.tabs.remove(index);
        if self.active == tab {
            self.active = self.tabs.get(index).copied().or_else(|| self.tabs.last().copied()).expect("nonempty after close");
        }
        Some(self.active)
    }
    pub fn replace_final(&mut self, old: TabId, replacement: TabId) -> bool {
        if self.tabs.as_slice() != [old] {
            return false;
        }
        self.tabs[0] = replacement;
        self.active = replacement;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_one_active_tab_through_ordering_and_closure() {
        let mut tabs = BrowserTabs::new(TabId::new(1));
        tabs.insert_active(TabId::new(2));
        tabs.reorder(TabId::new(2), 0);
        assert_eq!(tabs.tabs(), &[TabId::new(2), TabId::new(1)]);
        assert_eq!(tabs.close(TabId::new(2)), Some(TabId::new(1)));
        assert!(tabs.replace_final(TabId::new(1), TabId::new(3)));
        assert_eq!(tabs.active(), TabId::new(3));
    }
}
