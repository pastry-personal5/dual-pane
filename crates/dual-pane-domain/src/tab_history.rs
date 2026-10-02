use std::collections::HashSet;

use crate::{Entry, EntryName, Location, Selection};

/// A scroll position named by an entry and the view's offset from it, so it
/// survives a reload that inserts or removes rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollAnchor {
    entry: EntryName,
    offset: i32,
}

impl ScrollAnchor {
    pub fn new(entry: EntryName, offset: i32) -> Self {
        Self { entry, offset }
    }
    pub fn entry(&self) -> &EntryName {
        &self.entry
    }
    pub fn offset(&self) -> i32 {
        self.offset
    }
}

/// The selection, cursor, range anchor, and scroll position of one visited
/// folder. Every gesture names an entry by its row and exact name in the
/// current Folder Items and is ignored when they disagree, so the state only
/// ever refers to entries that are present.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VisitState {
    selection: Selection,
    cursor: Option<EntryName>,
    anchor: Option<EntryName>,
    scroll: Option<ScrollAnchor>,
}

fn is_at(entries: &[Entry], row: usize, name: &EntryName) -> bool {
    entries.get(row).is_some_and(|entry| entry.name() == name)
}

impl VisitState {
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    pub fn cursor(&self) -> Option<&EntryName> {
        self.cursor.as_ref()
    }
    pub fn anchor(&self) -> Option<&EntryName> {
        self.anchor.as_ref()
    }
    pub fn scroll(&self) -> Option<&ScrollAnchor> {
        self.scroll.as_ref()
    }
    /// The cursor's row in `entries`.
    pub fn cursor_row(&self, entries: &[Entry]) -> Option<usize> {
        let cursor = self.cursor.as_ref()?;
        entries.iter().position(|entry| entry.name() == cursor)
    }

    /// A plain click or movement: selects only `name` and makes it the cursor
    /// and range anchor. Reports whether anything changed, including an anchor
    /// that was missing.
    pub fn select(&mut self, entries: &[Entry], row: usize, name: &EntryName) -> bool {
        if !is_at(entries, row, name) {
            return false;
        }
        let changed = self.cursor.as_ref() != Some(name) || self.anchor.as_ref() != Some(name);
        let selection_changed = self.selection.select(name.clone());
        self.cursor = Some(name.clone());
        self.anchor = Some(name.clone());
        changed || selection_changed
    }

    /// A Command-click: toggles `name` and moves the cursor there. An existing
    /// range anchor moves too; a missing one is not established.
    pub fn toggle(&mut self, entries: &[Entry], row: usize, name: &EntryName) -> bool {
        if !is_at(entries, row, name) {
            return false;
        }
        self.selection.toggle(name.clone());
        if self.anchor.is_some() {
            self.anchor = Some(name.clone());
        }
        self.cursor = Some(name.clone());
        true
    }

    /// A Shift-click or Shift-movement: selects every row from the range anchor
    /// to `row` and moves the cursor there. Without an anchor this is a no-op.
    pub fn select_range(&mut self, entries: &[Entry], row: usize, name: &EntryName) -> bool {
        if !is_at(entries, row, name) {
            return false;
        }
        let Some(anchor_row) = self.anchor.as_ref().and_then(|anchor| entries.iter().position(|entry| entry.name() == anchor)) else { return false };
        let (start, end) = if anchor_row <= row { (anchor_row, row) } else { (row, anchor_row) };
        let changed = self.selection.replace(entries[start..=end].iter().map(|entry| entry.name().clone()).collect()) || self.cursor.as_ref() != Some(name);
        self.cursor = Some(name.clone());
        changed
    }

    /// A right-click: a selected entry keeps the selection; an unselected one
    /// becomes the only selected entry and the cursor, without establishing a
    /// missing range anchor.
    pub fn select_secondary(&mut self, entries: &[Entry], row: usize, name: &EntryName) -> bool {
        if !is_at(entries, row, name) || self.selection.contains(name) {
            return false;
        }
        self.selection.select(name.clone());
        self.cursor = Some(name.clone());
        true
    }

    /// Selects every entry without moving the cursor or range anchor.
    pub fn select_all(&mut self, entries: &[Entry]) -> bool {
        self.selection.replace(entries.iter().map(|entry| entry.name().clone()).collect())
    }

    /// Clears the selection and the cursor, so a later reload or tab switch
    /// does not report the cleared row again. The range anchor is kept.
    pub fn clear_selection(&mut self) -> bool {
        let cursor_changed = self.cursor.take().is_some();
        self.selection.clear() || cursor_changed
    }

    /// Records the scroll position, or clears it when `scroll` is `None` or
    /// names an entry that is not present.
    pub fn set_scroll(&mut self, entries: &[Entry], scroll: Option<ScrollAnchor>) {
        self.scroll = scroll.filter(|scroll| entries.iter().any(|entry| entry.name() == scroll.entry()));
    }

    /// Keeps only what still names an entry in the reloaded `entries`. A
    /// missing cursor is cleared rather than moved onto another entry. One
    /// lookup set keeps this linear in the listing size.
    pub fn reconcile(&mut self, entries: &[Entry]) {
        let present = entries.iter().map(Entry::name).collect::<HashSet<_>>();
        self.selection.retain(|name| present.contains(name));
        for name in [&mut self.cursor, &mut self.anchor] {
            if name.as_ref().is_some_and(|name| !present.contains(name)) {
                *name = None;
            }
        }
        if self.scroll.as_ref().is_some_and(|scroll| !present.contains(scroll.entry())) {
            self.scroll = None;
        }
    }
}

/// One folder a tab has shown, with its state as last left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    location: Location,
    state: VisitState,
}

impl Visit {
    pub fn location(&self) -> &Location {
        &self.location
    }
    pub fn state(&self) -> &VisitState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut VisitState {
        &mut self.state
    }
}

/// A tab's Back/Forward history. It is empty until the first folder is
/// shown; afterwards its current position always names a visit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabHistory {
    visits: Vec<Visit>,
    current: Option<usize>,
}

impl TabHistory {
    pub fn current_index(&self) -> Option<usize> {
        self.current
    }
    pub fn current(&self) -> Option<&Visit> {
        self.visits.get(self.current?)
    }
    pub fn current_mut(&mut self) -> Option<&mut Visit> {
        self.visits.get_mut(self.current?)
    }
    pub fn get(&self, index: usize) -> Option<&Visit> {
        self.visits.get(index)
    }
    pub fn len(&self) -> usize {
        self.visits.len()
    }
    pub fn is_empty(&self) -> bool {
        self.visits.is_empty()
    }

    /// The visit one step back or forward from `index`, if there is one.
    pub fn neighbor(&self, index: usize, forward: bool) -> Option<usize> {
        let target = if forward { index.checked_add(1) } else { index.checked_sub(1) }?;
        (target < self.visits.len()).then_some(target)
    }

    /// Records that the tab now shows `location`. Back or Forward arrives at
    /// `target` when that visit still names `location`. Otherwise a location
    /// other than the current one is a new visit, which discards the Forward
    /// visits; the current location itself, as after a refresh, changes
    /// nothing.
    pub fn arrive(&mut self, location: &Location, target: Option<usize>) {
        if let Some(target) = target.filter(|target| self.visits.get(*target).is_some_and(|visit| visit.location == *location)) {
            self.current = Some(target);
        } else if self.current().is_none_or(|visit| visit.location != *location) {
            self.visits.truncate(self.current.map_or(0, |current| current + 1));
            self.visits.push(Visit { location: location.clone(), state: VisitState::default() });
            self.current = Some(self.visits.len() - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntryKind;
    use proptest::prelude::*;

    fn name(index: u8) -> EntryName {
        EntryName::new(format!("item-{index}")).unwrap()
    }
    fn entries(names: &[u8]) -> Vec<Entry> {
        names.iter().map(|index| Entry::new(name(*index), EntryKind::File)).collect()
    }
    fn location(index: u8) -> Location {
        Location::root().join(&name(index))
    }

    #[test]
    fn a_plain_select_with_a_missing_anchor_reports_a_change() {
        let listed = entries(&[0, 1]);
        let mut state = VisitState::default();
        assert!(state.select(&listed, 0, &name(0)));
        assert!(!state.select(&listed, 0, &name(0)));
        assert!(!state.select(&listed, 1, &name(0)), "a row and name that disagree are ignored");
    }

    #[test]
    fn right_click_and_toggle_do_not_establish_a_missing_anchor() {
        let listed = entries(&[0, 1, 2]);
        let mut state = VisitState::default();
        assert!(state.select_secondary(&listed, 1, &name(1)));
        assert!(state.toggle(&listed, 2, &name(2)));
        assert_eq!(state.anchor(), None);
        assert!(!state.select_range(&listed, 0, &name(0)), "a range needs an anchor");
        state.select(&listed, 0, &name(0));
        assert!(state.select_range(&listed, 2, &name(2)));
        assert_eq!(state.selection().entries(), &[name(0), name(1), name(2)]);
        assert_eq!((state.cursor(), state.anchor()), (Some(&name(2)), Some(&name(0))));
    }

    #[test]
    fn clearing_drops_the_cursor_but_keeps_the_anchor() {
        let listed = entries(&[0, 1]);
        let mut state = VisitState::default();
        state.select(&listed, 1, &name(1));
        assert!(state.clear_selection());
        assert_eq!((state.cursor_row(&listed), state.anchor()), (None, Some(&name(1))));
        assert!(!state.clear_selection(), "nothing is left to clear");
    }

    #[test]
    fn a_new_visit_after_back_discards_forward_visits() {
        let mut history = TabHistory::default();
        assert!(history.current().is_none());
        for index in 0..3 {
            history.arrive(&location(index), None);
        }
        history.arrive(&location(0), history.neighbor(1, false));
        assert_eq!(history.current_index(), Some(0));
        history.arrive(&location(0), None);
        assert_eq!(history.len(), 3, "arriving at the current location is not a new visit");
        history.arrive(&location(9), None);
        assert_eq!(history.len(), 2);
        assert_eq!(history.neighbor(1, true), None);
    }

    #[derive(Debug, Clone)]
    enum Op {
        Select(usize),
        Toggle(usize),
        Range(usize),
        Secondary(usize),
        All,
        Clear,
        Scroll(u8, i32),
        Reload(Vec<u8>),
        Arrive(u8, Option<usize>),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![(0..8usize).prop_map(Op::Select), (0..8usize).prop_map(Op::Toggle), (0..8usize).prop_map(Op::Range), (0..8usize).prop_map(Op::Secondary), Just(Op::All), Just(Op::Clear), (0..8u8, -5..5i32).prop_map(|(entry, offset)| Op::Scroll(entry, offset)), proptest::collection::btree_set(0..8u8, 0..8).prop_map(|names| Op::Reload(names.into_iter().collect())), (0..4u8, proptest::option::of(0..6usize)).prop_map(|(at, target)| Op::Arrive(at, target)),]
    }

    proptest! {
        #[test]
        fn state_only_names_present_entries_and_history_stays_in_range(ops in proptest::collection::vec(op(), 0..60)) {
            let mut history = TabHistory::default();
            history.arrive(&location(0), None);
            let mut listed = entries(&[0, 1, 2, 3]);
            for op in ops {
                let state = history.current_mut().expect("a visit after the first arrival").state_mut();
                let at = |row: usize| listed.get(row).map_or_else(|| name(0), |entry| entry.name().clone());
                match op {
                    Op::Select(row) => { state.select(&listed, row, &at(row)); }
                    Op::Toggle(row) => { state.toggle(&listed, row, &at(row)); }
                    Op::Range(row) => { state.select_range(&listed, row, &at(row)); }
                    Op::Secondary(row) => { state.select_secondary(&listed, row, &at(row)); }
                    Op::All => { state.select_all(&listed); }
                    Op::Clear => { state.clear_selection(); }
                    Op::Scroll(entry, offset) => state.set_scroll(&listed, Some(ScrollAnchor::new(name(entry), offset))),
                    Op::Reload(names) => {
                        listed = entries(&names);
                        state.reconcile(&listed);
                    }
                    Op::Arrive(at, target) => {
                        let before = history.clone();
                        history.arrive(&location(at), target);
                        let current = history.current_index().expect("history stays nonempty");
                        prop_assert!(current < history.len());
                        prop_assert_eq!(history.current().map(Visit::location), Some(&location(at)));
                        if before.current().map(Visit::location) != Some(&location(at)) && target.and_then(|target| before.get(target)).map(Visit::location) != Some(&location(at)) {
                            prop_assert_eq!(current, history.len() - 1, "a new visit has no Forward visits");
                        }
                        listed = entries(&[0, 1, 2, 3]);
                        let state = history.current_mut().expect("current visit").state_mut();
                        state.reconcile(&listed);
                    }
                }
                let state = history.current().expect("current visit").state();
                let present = |name: &EntryName| listed.iter().any(|entry| entry.name() == name);
                prop_assert!(state.selection().entries().iter().all(present));
                prop_assert!(state.cursor().is_none_or(present));
                prop_assert!(state.anchor().is_none_or(present));
                prop_assert!(state.scroll().is_none_or(|scroll| present(scroll.entry())));
            }
        }
    }
}
