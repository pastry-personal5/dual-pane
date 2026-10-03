use crate::TabId;

/// Ordered, nonempty tab identity state for one Browser, holding at most
/// [`BrowserTabs::LIMIT`] tabs. Listing and history data live in the
/// application layer; this value protects structural tab invariants
/// independently of the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTabs {
    tabs: Vec<TabId>,
    active: TabId,
}

impl BrowserTabs {
    /// The most tabs one Browser holds.
    pub const LIMIT: usize = 8;

    pub fn new(initial: TabId) -> Self {
        Self { tabs: vec![initial], active: initial }
    }
    /// Restores a persisted ordered strip.  Keeping this validation in the
    /// domain means adapters cannot accidentally construct an empty Browser.
    pub fn from_ordered(tabs: Vec<TabId>, active_index: usize) -> Option<Self> {
        if tabs.is_empty() || tabs.len() > Self::LIMIT || active_index >= tabs.len() {
            return None;
        }
        let active = tabs[active_index];
        (!tabs.windows(2).any(|pair| pair[0] == pair[1])).then_some(Self { tabs, active })
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
    /// Whether another tab can be added.
    pub fn can_insert(&self) -> bool {
        self.tabs.len() < Self::LIMIT
    }
    /// Appends `tab` and activates it, or refuses at the tab limit or when
    /// `tab` is already present.
    pub fn insert_active(&mut self, tab: TabId) -> bool {
        if !self.can_insert() || self.tabs.contains(&tab) {
            return false;
        }
        self.tabs.push(tab);
        self.active = tab;
        true
    }
    /// Moves `tab` to `position`, clamped to the end, and reports whether the
    /// order changed.
    pub fn reorder(&mut self, tab: TabId, position: usize) -> bool {
        let Some(index) = self.tabs.iter().position(|item| *item == tab) else { return false };
        let position = position.min(self.tabs.len() - 1);
        if position == index {
            return false;
        }
        let tab = self.tabs.remove(index);
        self.tabs.insert(position, tab);
        true
    }
    /// Removes a nonfinal tab and returns the active tab afterwards. When the
    /// removed tab was active, its right neighbor, or else its left neighbor,
    /// becomes active. The final tab must be replaced by its caller.
    pub fn close(&mut self, tab: TabId) -> Option<TabId> {
        if self.tabs.len() == 1 {
            return None;
        }
        let index = self.tabs.iter().position(|item| *item == tab)?;
        self.tabs.remove(index);
        if self.active == tab {
            self.active = self.tabs[index.min(self.tabs.len() - 1)];
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

    #[test]
    fn reordering_reports_only_actual_moves() {
        let mut tabs = BrowserTabs::new(TabId::new(1));
        tabs.insert_active(TabId::new(2));
        assert!(!tabs.reorder(TabId::new(2), 1));
        assert!(!tabs.reorder(TabId::new(2), 7), "a position past the end clamps to the last slot");
        assert!(!tabs.reorder(TabId::new(9), 0));
        assert!(tabs.reorder(TabId::new(1), 7));
        assert_eq!(tabs.tabs(), &[TabId::new(2), TabId::new(1)]);
    }

    #[test]
    fn refuses_a_tab_past_the_limit() {
        let mut tabs = BrowserTabs::new(TabId::new(0));
        for id in 1..BrowserTabs::LIMIT as u64 {
            assert!(tabs.insert_active(TabId::new(id)));
        }
        assert!(!tabs.can_insert());
        assert!(!tabs.insert_active(TabId::new(99)));
        assert_eq!(tabs.tabs().len(), BrowserTabs::LIMIT);
        assert_eq!(tabs.active(), TabId::new(BrowserTabs::LIMIT as u64 - 1));
        assert_eq!(tabs.close(TabId::new(3)), Some(TabId::new(BrowserTabs::LIMIT as u64 - 1)));
        assert!(tabs.insert_active(TabId::new(99)));
    }

    #[test]
    fn closing_the_active_tab_prefers_its_right_then_left_neighbor() {
        let mut tabs = BrowserTabs::new(TabId::new(1));
        tabs.insert_active(TabId::new(2));
        tabs.insert_active(TabId::new(3));
        tabs.activate(TabId::new(2));
        assert_eq!(tabs.close(TabId::new(2)), Some(TabId::new(3)));
        assert_eq!(tabs.close(TabId::new(3)), Some(TabId::new(1)));
        assert_eq!(tabs.close(TabId::new(1)), None, "the final tab is replaced, not closed");
    }
}
