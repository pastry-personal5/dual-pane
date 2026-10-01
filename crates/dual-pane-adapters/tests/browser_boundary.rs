use std::sync::Arc;

use dual_pane_adapters::{BrowserPresenter, InputController, UiEvent};
use dual_pane_application::{Command, Output};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location};

#[test]
fn controller_addresses_the_requested_browser_and_presenters_stay_independent() {
    let entry = Entry::new(EntryName::new("docs").unwrap(), EntryKind::Directory);
    let mut presenter = BrowserPresenter::new(BrowserSide::Left);
    presenter.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Left, location: Location::root(), entries: Arc::from(vec![entry.clone()]) });
    assert_eq!(InputController::new().command(BrowserSide::Left, UiEvent::ActivateRow { row: 0 }, presenter.view()), Some(Command::OpenEntry { browser: BrowserSide::Left, row: 0, name: entry.name().clone() }));
    presenter.apply(&Output::LoadingStarted { browser: BrowserSide::Left, location: Location::root() });
    assert!(presenter.view().is_loading());

    let mut right = BrowserPresenter::new(BrowserSide::Right);
    right.apply(&Output::FolderItemsReplaced { browser: BrowserSide::Right, location: Location::root(), entries: Arc::from(vec![entry.clone()]) });
    assert_eq!(InputController::new().command(BrowserSide::Right, UiEvent::ActivateRow { row: 0 }, right.view()), Some(Command::OpenEntry { browser: BrowserSide::Right, row: 0, name: entry.name().clone() }));
    assert_eq!(right.view().folder_items_revision(), 1);
    assert_eq!(presenter.view().folder_items_revision(), 1);
}
