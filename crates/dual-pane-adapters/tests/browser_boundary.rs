use dual_pane_adapters::{BrowserPresenter, InputController, UiEvent};
use dual_pane_application::{Command, Output};
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
