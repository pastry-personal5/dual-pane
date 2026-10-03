use crate::{FavoriteEdit, FavoriteRejection, FavoritesRecords, Notice, OperationJob, SettingsFailure, WindowLayout};
use dual_pane_domain::{BrowserSide, Entry, EntryName, ListingError, Location, OperationCommand, OperationId, OperationRejection, ScrollAnchor, Selection, TabId};
use std::sync::Arc;

/// A framework-neutral contiguous listing mutation. `removed` rows beginning
/// at `row` are replaced by `inserted` rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowChange {
    pub row: usize,
    pub removed: usize,
    pub inserted: usize,
}

/// What changed, for presentation.
///
/// In `FolderItemsLoaded`, `changes` turns the Folder Items the tab showed
/// before into `entries`. `None` means every row is replaced, as for another
/// folder, a first listing, or a gateway change that does not fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    OperationChanged {
        job: Box<OperationJob>,
    },
    OperationRejected {
        browser: BrowserSide,
        tab: TabId,
        command: OperationCommand,
        reason: OperationRejection,
    },
    /// The inline editor for Rename or New Folder may open. `target` is the
    /// Item a Rename edits.
    NameEditorOpened {
        browser: BrowserSide,
        tab: TabId,
        command: OperationCommand,
        target: Option<EntryName>,
    },
    /// A finished job left application state.
    OperationDismissed {
        id: OperationId,
    },
    /// Open `item` with its default application on the GUI thread. A refusal
    /// returns as `Event::OpenFailed`.
    OpenItem {
        item: Location,
    },
    /// Quitting needs confirmation because `running` operations are active.
    QuitConfirmationRequired {
        running: usize,
    },
    /// Every operation was told to cancel; the application may exit.
    QuitAccepted,
    /// Emitted once after the first settings answer or the protected timeout;
    /// the desktop applies the supplied layout before revealing the window.
    SessionRestored {
        active_browser: BrowserSide,
        layout: Option<WindowLayout>,
    },
    LayoutReset,
    ActiveBrowserChanged {
        browser: BrowserSide,
    },
    ActiveTabChanged {
        browser: BrowserSide,
        tab: TabId,
    },
    TabsChanged {
        browser: BrowserSide,
        active_tab: TabId,
    },
    TabViewChanged {
        browser: BrowserSide,
        tab: TabId,
        location: Option<Location>,
        entries: Arc<[Entry]>,
        selection: Selection,
        row: Option<usize>,
        scroll_hint: Option<ScrollAnchor>,
        loading: bool,
        error: Option<ListingError>,
    },
    LoadingStarted {
        browser: BrowserSide,
        tab: TabId,
        location: Location,
    },
    FolderItemsLoaded {
        browser: BrowserSide,
        tab: TabId,
        location: Location,
        entries: Arc<[Entry]>,
        changes: Option<Vec<RowChange>>,
        scroll_hint: Option<ScrollAnchor>,
    },
    SelectionChanged {
        browser: BrowserSide,
        tab: TabId,
        selection: Selection,
        row: Option<usize>,
    },
    FolderItemsFailed {
        browser: BrowserSide,
        tab: TabId,
        error: ListingError,
    },
    FolderItemsCancelled {
        browser: BrowserSide,
        tab: TabId,
    },
    FavoritesChanged {
        favorites: FavoritesRecords,
    },
    /// A Favorites edit left the hierarchy unchanged, and why.
    FavoriteEditRejected {
        edit: FavoriteEdit,
        reason: FavoriteRejection,
    },
    SettingsSaveFailed {
        revision: u64,
        failure: SettingsFailure,
    },
    SettingsLoadFailed {
        failure: SettingsFailure,
    },
    /// A message for Notices. `open` asks to show Notices because the person
    /// can act on it.
    NoticeAdded {
        notice: Notice,
        open: bool,
    },
}

/// The smallest single contiguous change that turns `old` into `new`: rows
/// shared at both ends are kept and only the differing middle is replaced.
/// A directory-read gateway calls this on its worker so the GUI owner never
/// compares listings.
pub fn listing_changes(old: &[Entry], new: &[Entry]) -> Vec<RowChange> {
    let prefix = old.iter().zip(new).take_while(|(old, new)| old == new).count();
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(old, new)| old == new).count();
    let (removed, inserted) = (old.len() - prefix - suffix, new.len() - prefix - suffix);
    if removed == 0 && inserted == 0 { vec![] } else { vec![RowChange { row: prefix, removed, inserted }] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::{EntryKind, EntryMetadata, EntryName};
    use proptest::prelude::*;

    fn entry(name: u8, size: u8) -> Entry {
        Entry::with_metadata(EntryName::new(format!("item-{name}")).unwrap(), EntryKind::File, EntryMetadata::new(None, Some(u64::from(size))))
    }

    proptest! {
        #[test]
        fn applying_the_reported_change_to_the_old_rows_yields_the_new_rows(old in proptest::collection::vec((0..6u8, 0..2u8), 0..12), new in proptest::collection::vec((0..6u8, 0..2u8), 0..12)) {
            let old = old.into_iter().map(|(name, size)| entry(name, size)).collect::<Vec<_>>();
            let new = new.into_iter().map(|(name, size)| entry(name, size)).collect::<Vec<_>>();
            let changes = listing_changes(&old, &new);
            prop_assert!(changes.len() <= 1);
            let mut rows = old.clone();
            for change in &changes {
                prop_assert!(change.removed > 0 || change.inserted > 0);
                prop_assert!(change.row + change.removed <= rows.len());
                rows.splice(change.row..change.row + change.removed, new[change.row..change.row + change.inserted].iter().cloned());
            }
            prop_assert_eq!(rows, new);
        }
    }

    #[test]
    fn a_metadata_change_replaces_only_its_row() {
        let old = [entry(1, 0), entry(2, 0), entry(3, 0)];
        let new = [entry(1, 0), entry(2, 1), entry(3, 0)];
        assert_eq!(listing_changes(&old, &new), vec![RowChange { row: 1, removed: 1, inserted: 1 }]);
        assert_eq!(listing_changes(&old, &old), vec![]);
    }
}
