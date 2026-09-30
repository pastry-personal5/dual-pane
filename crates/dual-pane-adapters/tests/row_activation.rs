use std::sync::Arc;

use dual_pane_adapters::{InputController, PanePresenter, UiEvent};
use dual_pane_application::{Command, Output};
use dual_pane_domain::{Entry, EntryKind, EntryName, Location};

fn presenter_showing(entries: Vec<Entry>) -> PanePresenter {
    let mut presenter = PanePresenter::new();
    presenter.apply(&Output::ListingReplaced { location: Location::root(), entries: Arc::from(entries) });
    presenter
}

#[test]
fn activating_a_row_opens_that_entry_by_its_exact_name() {
    let exact = EntryName::new(b"a\xFF".to_vec()).unwrap();
    let alike = EntryName::new(b"a\xFE".to_vec()).unwrap();
    let presenter = presenter_showing(vec![Entry::new(alike, EntryKind::Directory), Entry::new(exact.clone(), EntryKind::Directory)]);
    let command = InputController::new().command(UiEvent::ActivateRow { row: 1 }, presenter.view());
    assert_eq!(command, Some(Command::OpenEntry { row: 1, name: exact }));
}

#[test]
fn activating_a_file_row_still_produces_a_command() {
    // Whether a file can be opened is the application's decision.
    let name = EntryName::new("notes.txt").unwrap();
    let presenter = presenter_showing(vec![Entry::new(name.clone(), EntryKind::File)]);
    assert_eq!(InputController::new().command(UiEvent::ActivateRow { row: 0 }, presenter.view()), Some(Command::OpenEntry { row: 0, name }));
}

#[test]
fn activating_a_row_that_is_not_shown_produces_no_command() {
    let presenter = presenter_showing(vec![Entry::new(EntryName::new("a").unwrap(), EntryKind::File)]);
    assert_eq!(InputController::new().command(UiEvent::ActivateRow { row: 1 }, presenter.view()), None);
    assert_eq!(InputController::new().command(UiEvent::ActivateRow { row: 0 }, PanePresenter::new().view()), None);
}

#[test]
fn going_to_the_parent_maps_to_its_command() {
    assert_eq!(InputController::new().command(UiEvent::GoToParent, PanePresenter::new().view()), Some(Command::GoToParent));
}

#[test]
fn selecting_a_row_carries_its_row_and_exact_name() {
    let exact = EntryName::new(b"a\xFF".to_vec()).unwrap();
    let presenter = presenter_showing(vec![Entry::new(exact.clone(), EntryKind::File)]);
    assert_eq!(InputController::new().command(UiEvent::SelectRow { row: 0 }, presenter.view()), Some(Command::SelectEntry { row: 0, name: exact }));
    assert_eq!(InputController::new().command(UiEvent::SelectRow { row: 1 }, presenter.view()), None);
}
