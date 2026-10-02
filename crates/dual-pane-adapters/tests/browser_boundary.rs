use dual_pane_adapters::{BrowserPresenter, FolderItemsColumn, FolderItemsUpdate, InputController, SelectionMovement, UiEvent};
use dual_pane_application::{Command, Output, RowChange};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryMetadata, EntryName, Location, ScrollAnchor, Selection, TabId};
use jiff::tz::TimeZone;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

#[test]
fn controller_uses_exact_row_identity() {
    let entry = Entry::new(EntryName::new("docs").unwrap(), EntryKind::Directory);
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries: Arc::from(vec![entry.clone()]), changes: None, scroll_hint: None });
    assert_eq!(InputController::new().command(BrowserSide::Left, UiEvent::ActivateRow { row: 0 }, presenter.view()), Some(Command::OpenEntry { browser: BrowserSide::Left, tab: TabId::new(0), row: 0, name: entry.name().clone() }));
}

#[test]
fn inactive_tab_result_does_not_replace_the_visible_listing() {
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    let first = TabId::new(1);
    let second = TabId::new(2);
    let shown = Entry::new(EntryName::new("shown").unwrap(), EntryKind::File);
    let hidden = Entry::new(EntryName::new("hidden").unwrap(), EntryKind::File);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: first, location: Some(Location::root()), entries: Arc::from(vec![shown]), selection: Default::default(), row: None, scroll_hint: Some(ScrollAnchor::new(EntryName::new("shown").unwrap(), 7)), loading: false, error: None });
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: second, location: Location::root(), entries: Arc::from(vec![hidden]), changes: None, scroll_hint: Some(ScrollAnchor::new(EntryName::new("hidden").unwrap(), 9)) });
    assert_eq!(presenter.view().row(0).unwrap().name, "shown");
    assert_eq!(presenter.view().scroll_hint(), Some(&ScrollAnchor::new(EntryName::new("shown").unwrap(), 7)));
}

fn files(names: &[&str]) -> Arc<[Entry]> {
    names.iter().map(|name| Entry::new(EntryName::new(*name).unwrap(), EntryKind::File)).collect()
}

/// Applies one same-folder reload of `tab` the way the workspace reports it.
fn reload(presenter: &mut BrowserPresenter, tab: TabId, change: Option<RowChange>, names: &[&str]) {
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab, location: Location::root(), entries: files(names), changes: Some(change.into_iter().collect()), scroll_hint: None });
}

fn shown(names: &[&str]) -> BrowserPresenter {
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: TabId::new(0), location: Some(Location::root()), entries: files(names), selection: Default::default(), row: None, scroll_hint: None, loading: false, error: None });
    presenter
}

#[test]
fn a_same_folder_reload_updates_only_the_changed_rows() {
    let cases = [(RowChange { row: 1, removed: 1, inserted: 1 }, &["a", "x", "c"][..], FolderItemsUpdate::Rows { changed: Some((1, 1)), inserted: None, removed: None }), (RowChange { row: 1, removed: 0, inserted: 2 }, &["a", "x", "y", "b", "c"][..], FolderItemsUpdate::Rows { changed: None, inserted: Some((1, 2)), removed: None }), (RowChange { row: 0, removed: 2, inserted: 0 }, &["c"][..], FolderItemsUpdate::Rows { changed: None, inserted: None, removed: Some((0, 1)) }), (RowChange { row: 1, removed: 2, inserted: 1 }, &["a", "z"][..], FolderItemsUpdate::Rows { changed: Some((1, 1)), inserted: None, removed: Some((2, 2)) }), (RowChange { row: 2, removed: 1, inserted: 3 }, &["a", "b", "x", "y", "z"][..], FolderItemsUpdate::Rows { changed: Some((2, 2)), inserted: Some((3, 4)), removed: None })];
    for (change, names, expected) in cases {
        let mut presenter = shown(&["a", "b", "c"]);
        let before = presenter.view().clone();
        reload(&mut presenter, TabId::new(0), Some(change), names);
        assert_eq!(presenter.view().update_from(&before), expected);
    }
}

#[test]
fn an_identical_reload_is_unchanged_and_a_missed_revision_resets() {
    let mut presenter = shown(&["a", "b"]);
    let before = presenter.view().clone();
    reload(&mut presenter, TabId::new(0), None, &["a", "b"]);
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Unchanged);
    reload(&mut presenter, TabId::new(0), Some(RowChange { row: 2, removed: 0, inserted: 1 }), &["a", "b", "c"]);
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);
}

#[test]
fn an_unchanged_listing_repaints_relative_dates_after_time_advances() {
    let now = Arc::new(AtomicI64::new(1_000_000_000));
    let clock = Arc::clone(&now);
    let mut presenter = BrowserPresenter::with_clock(BrowserSide::Left, Arc::new(move || clock.load(Ordering::Relaxed)), TimeZone::UTC);
    let dated = Entry::with_metadata(EntryName::new("dated").unwrap(), EntryKind::File, EntryMetadata::new(Some(1_000_000_000 - 3_600), None));
    let missing = Entry::new(EntryName::new("missing").unwrap(), EntryKind::File);
    let entries: Arc<[Entry]> = Arc::from([dated, missing]);
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries: Arc::clone(&entries), changes: None, scroll_hint: None });
    let before = presenter.view().clone();
    assert_eq!(before.relative_age_position(0), Some(0.0));
    assert_eq!(before.relative_age_position(1), None);
    assert_eq!(before.cell_text(0, FolderItemsColumn::RelativeDate).as_deref(), Some("1 h"));

    now.fetch_add(86_400 - 3_600, Ordering::Relaxed);
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries, changes: Some(vec![]), scroll_hint: None });
    let after = presenter.view();
    assert_eq!(after.update_from(&before), FolderItemsUpdate::Unchanged);
    assert!(after.relative_dates_changed_from(&before));
    assert_eq!(after.relative_age_position(0), Some(0.25));
    assert_eq!(after.cell_text(0, FolderItemsColumn::RelativeDate).as_deref(), Some("1 d"));
    assert_eq!(after.relative_age_position(1), None);
}

#[test]
fn a_tab_switch_keeps_the_listings_captured_time() {
    let now = Arc::new(AtomicI64::new(1_000_000_000));
    let clock = Arc::clone(&now);
    let mut presenter = BrowserPresenter::with_clock(BrowserSide::Left, Arc::new(move || clock.load(Ordering::Relaxed)), TimeZone::UTC);
    let first = TabId::new(1);
    let second = TabId::new(2);
    let dated = Entry::with_metadata(EntryName::new("dated").unwrap(), EntryKind::File, EntryMetadata::new(Some(1_000_000_000 - 3_600), None));
    let entries: Arc<[Entry]> = Arc::from([dated]);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: first, location: Some(Location::root()), entries: Arc::clone(&entries), selection: Selection::default(), row: None, scroll_hint: None, loading: false, error: None });
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, location: Location::root(), entries: Arc::clone(&entries), changes: Some(vec![]), scroll_hint: None });
    assert_eq!(presenter.view().relative_age_position(0), Some(0.0));

    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: second, location: Some(Location::root()), entries: Arc::from([]), selection: Selection::default(), row: None, scroll_hint: None, loading: false, error: None });
    now.fetch_add(86_400, Ordering::Relaxed);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: first, location: Some(Location::root()), entries: Arc::clone(&entries), selection: Selection::default(), row: None, scroll_hint: None, loading: false, error: None });
    assert_eq!(presenter.view().relative_age_position(0), Some(0.0));
    assert_eq!(presenter.view().cell_text(0, FolderItemsColumn::RelativeDate).as_deref(), Some("1 h"));

    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: second, location: Some(Location::root()), entries: Arc::from([]), selection: Selection::default(), row: None, scroll_hint: None, loading: false, error: None });
    now.fetch_add(86_400, Ordering::Relaxed);
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, location: Location::root(), entries: Arc::clone(&entries), changes: Some(vec![]), scroll_hint: None });
    now.fetch_add(86_400, Ordering::Relaxed);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: first, location: Some(Location::root()), entries, selection: Selection::default(), row: None, scroll_hint: None, loading: false, error: None });
    assert_eq!(presenter.view().cell_text(0, FolderItemsColumn::RelativeDate).as_deref(), Some("2 d"));
}

#[test]
fn a_tab_switch_or_inconsistent_change_resets() {
    let mut presenter = shown(&["a", "b"]);
    let before = presenter.view().clone();
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: TabId::new(1), location: Some(Location::root()), entries: files(&["a", "b"]), selection: Default::default(), row: None, scroll_hint: None, loading: false, error: None });
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);

    let mut presenter = shown(&["a", "b"]);
    let before = presenter.view().clone();
    reload(&mut presenter, TabId::new(0), Some(RowChange { row: 1, removed: 3, inserted: 1 }), &["a", "z"]);
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);
}

#[test]
fn a_listing_without_a_row_change_resets() {
    let mut presenter = shown(&["a", "b", "c"]);
    let before = presenter.view().clone();
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root().join(&EntryName::new("b").unwrap()), entries: files(&["x", "y"]), changes: None, scroll_hint: None });
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);

    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    let before = presenter.view().clone();
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries: files(&["x", "y"]), changes: None, scroll_hint: None });
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);
}

/// The row a movement selects in `rows` rows with the cursor at `cursor`.
fn moved(rows: &[&str], cursor: Option<usize>, movement: SelectionMovement) -> Option<usize> {
    let mut presenter = shown(rows);
    presenter.apply(&Output::SelectionChanged { browser: BrowserSide::Left, tab: TabId::new(0), selection: Selection::default(), row: cursor });
    match InputController::new().command(BrowserSide::Left, UiEvent::MoveSelection(movement), presenter.view()) {
        Some(Command::MoveSelection { row, name, .. }) => {
            assert_eq!(name.as_bytes(), rows[row].as_bytes());
            Some(row)
        }
        other => {
            assert_eq!(other, None);
            None
        }
    }
}

#[test]
fn selection_movement_follows_the_keyboard_rules() {
    use SelectionMovement::{First, Last, Next, PageDown, PageUp, Previous};
    let rows = ["a", "b", "c", "d", "e"];
    let cases = [
        // Without a cursor, End and Page Down select the last row and every other movement the first.
        (None, Previous, 0),
        (None, Next, 0),
        (None, First, 0),
        (None, PageUp { rows: 2 }, 0),
        (None, Last, 4),
        (None, PageDown { rows: 2 }, 4),
        // With a cursor, movement clamps at both ends.
        (Some(0), Previous, 0),
        (Some(2), Previous, 1),
        (Some(2), Next, 3),
        (Some(4), Next, 4),
        (Some(3), First, 0),
        (Some(1), Last, 4),
        (Some(3), PageUp { rows: 2 }, 1),
        (Some(1), PageUp { rows: 2 }, 0),
        (Some(1), PageDown { rows: 2 }, 3),
        (Some(3), PageDown { rows: 2 }, 4),
        // A page is at least one row.
        (Some(2), PageDown { rows: 0 }, 3),
    ];
    for (cursor, movement, expected) in cases {
        assert_eq!(moved(&rows, cursor, movement), Some(expected), "{cursor:?} {movement:?}");
    }
    for movement in [Previous, Next, First, Last, PageUp { rows: 1 }, PageDown { rows: 1 }] {
        assert_eq!(moved(&[], None, movement), None, "an empty list ignores {movement:?}");
    }
}

#[test]
fn activation_and_clearing_map_to_their_commands() {
    let controller = InputController::new();
    let mut presenter = shown(&["a", "b"]);
    assert_eq!(controller.command(BrowserSide::Left, UiEvent::FocusBrowser, presenter.view()), None);
    assert_eq!(controller.command(BrowserSide::Left, UiEvent::ClearSelection, presenter.view()), Some(Command::ClearSelection { browser: BrowserSide::Left, tab: TabId::new(0) }));
    assert_eq!(controller.command(BrowserSide::Left, UiEvent::ActivateSelection, presenter.view()), None);
    let mut selection = Selection::default();
    selection.select(EntryName::new("b").unwrap());
    presenter.apply(&Output::SelectionChanged { browser: BrowserSide::Left, tab: TabId::new(0), selection, row: Some(1) });
    assert_eq!(controller.command(BrowserSide::Left, UiEvent::ActivateSelection, presenter.view()), Some(Command::OpenEntry { browser: BrowserSide::Left, tab: TabId::new(0), row: 1, name: EntryName::new("b").unwrap() }));
}

#[test]
fn the_folder_name_is_the_last_location_component() {
    assert_eq!(BrowserPresenter::new(BrowserSide::Left).view().folder_name(), "/");
    assert_eq!(shown(&[]).view().folder_name(), "/");
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    let nested = Location::root().join(&EntryName::new("parent").unwrap()).join(&EntryName::new("child").unwrap());
    presenter.apply(&Output::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), location: nested, entries: files(&[]), changes: None, scroll_hint: None });
    assert_eq!(presenter.view().folder_name(), "child");
}
