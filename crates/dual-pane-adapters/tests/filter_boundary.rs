use std::sync::Arc;

use dual_pane_adapters::{BrowserPresenter, FolderItemsUpdate};
use dual_pane_application::{FilterStatus, Output};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryMetadata, EntryName, ListingError, ListingErrorKind, Location, Selection, TabId};

fn entry(text: &str, size: u64) -> Entry {
    Entry::with_metadata(EntryName::new(text).unwrap(), EntryKind::File, EntryMetadata::new(None, Some(size)))
}

fn changed(entries: Arc<[Entry]>, status: FilterStatus, revision: u64, selection: Selection) -> Output {
    changed_with_read_state(entries, status, revision, selection, false, None)
}

fn changed_with_read_state(entries: Arc<[Entry]>, status: FilterStatus, revision: u64, selection: Selection, loading: bool, error: Option<ListingError>) -> Output {
    Output::VisibleItemsChanged { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries, selection, row: None, scroll_hint: None, filter_text: "a".into(), filter_status: status, total_count: 3, visible_revision: revision, loading, error }
}

#[test]
fn filtering_does_not_erase_directory_loading_or_error_state() {
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    presenter.apply(&changed_with_read_state(Arc::from([]), FilterStatus::Updating, 1, Selection::default(), true, None));
    assert_eq!(presenter.view().status_text(), "Loading…");

    presenter.apply(&changed_with_read_state(Arc::from([]), FilterStatus::Failed, 2, Selection::default(), false, Some(ListingError::new(Location::root(), ListingErrorKind::ItemMissing))));
    assert_eq!(presenter.view().status_text(), "“/” no longer exists.");
}

#[test]
fn filtered_summary_status_and_rows_share_one_visible_mapping() {
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    let all: Arc<[Entry]> = vec![entry("alpha", 100), entry("beta", 200), entry("gamma", 300)].into();
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries: Arc::clone(&all), changes: None, scroll_hint: None, visible_revision: 1 });
    let before = presenter.view().clone();
    let mut selection = Selection::default();
    selection.select(EntryName::new("alpha").unwrap());
    presenter.apply(&changed(Arc::from([]), FilterStatus::Updating, 2, selection.clone()));
    assert_eq!(presenter.view().row_count(), 0);
    assert_eq!(presenter.view().summary().count, "0 of 3 Items");
    assert_eq!(presenter.view().summary().selected, "0 selected");
    assert_eq!(presenter.view().status_text(), "Updating filter…");
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);
    presenter.apply(&changed(vec![all[0].clone(), all[1].clone(), all[2].clone()].into(), FilterStatus::Ready, 3, selection));
    assert_eq!(presenter.view().visible_revision(), 3);
    assert_eq!(presenter.view().summary().size, "100 B / 600 B");
    assert_eq!(presenter.view().selected_rows(), vec![0]);
    presenter.apply(&changed(Arc::from([]), FilterStatus::Ready, 4, Selection::default()));
    assert!(presenter.view().no_matches());
    assert_eq!(presenter.view().summary().count, "0 of 3 Items");
    assert!(presenter.view().row(0).is_none());
}
