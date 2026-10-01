use dual_pane_adapters::{BrowserPresenter, FolderItemsUpdate, InputController, UiEvent};
use dual_pane_application::{Command, Output, RowChange};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, TabId};
use std::sync::Arc;

#[test]
fn controller_uses_exact_row_identity() {
    let entry = Entry::new(EntryName::new("docs").unwrap(), EntryKind::Directory);
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    presenter.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Left, tab: TabId::new(0), location: Location::root(), entries: Arc::from(vec![entry.clone()]), scroll_hint: None });
    assert_eq!(InputController::new().command(BrowserSide::Left, UiEvent::ActivateRow { row: 0 }, presenter.view()), Some(Command::OpenEntry { browser: BrowserSide::Left, row: 0, name: entry.name().clone() }));
}

#[test]
fn inactive_tab_result_does_not_replace_the_visible_listing() {
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    let first = TabId::new(1);
    let second = TabId::new(2);
    let shown = Entry::new(EntryName::new("shown").unwrap(), EntryKind::File);
    let hidden = Entry::new(EntryName::new("hidden").unwrap(), EntryKind::File);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: first, location: Some(Location::root()), entries: Arc::from(vec![shown]), selection: Default::default(), row: None, scroll_hint: Some((EntryName::new("shown").unwrap(), 7)), loading: false, error: None });
    presenter.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Left, tab: second, location: Location::root(), entries: Arc::from(vec![hidden]), scroll_hint: Some((EntryName::new("hidden").unwrap(), 9)) });
    assert_eq!(presenter.view().row(0).unwrap().name, "shown");
    assert_eq!(presenter.view().scroll_hint(), Some(&(EntryName::new("shown").unwrap(), 7)));
}

fn files(names: &[&str]) -> Arc<[Entry]> {
    names.iter().map(|name| Entry::new(EntryName::new(*name).unwrap(), EntryKind::File)).collect()
}

/// Applies one reload of `tab` the way the workspace reports it.
fn reload(presenter: &mut BrowserPresenter, tab: TabId, change: Option<RowChange>, names: &[&str]) {
    presenter.apply(&Output::FolderItemsRowsChanged { browser: BrowserSide::Left, tab, changes: change.into_iter().collect() });
    presenter.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Left, tab, location: Location::root(), entries: files(names), scroll_hint: None });
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

fn folder(name: &str) -> Location {
    Location::root().join(&EntryName::new(name).unwrap())
}

#[test]
fn navigating_to_another_folder_or_the_first_listing_resets() {
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    presenter.apply(&Output::TabViewChanged { browser: BrowserSide::Left, tab: TabId::new(0), location: Some(folder("a")), entries: files(&["a", "b", "c"]), selection: Default::default(), row: None, scroll_hint: None, loading: false, error: None });
    let before = presenter.view().clone();
    presenter.apply(&Output::FolderItemsRowsChanged { browser: BrowserSide::Left, tab: TabId::new(0), changes: vec![RowChange { row: 0, removed: 3, inserted: 2 }] });
    presenter.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Left, tab: TabId::new(0), location: folder("b"), entries: files(&["x", "y"]), scroll_hint: None });
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);

    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    let before = presenter.view().clone();
    presenter.apply(&Output::FolderItemsRowsChanged { browser: BrowserSide::Left, tab: TabId::new(0), changes: vec![RowChange { row: 0, removed: 0, inserted: 2 }] });
    presenter.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Left, tab: TabId::new(0), location: folder("a"), entries: files(&["x", "y"]), scroll_hint: None });
    assert_eq!(presenter.view().update_from(&before), FolderItemsUpdate::Reset);
}
