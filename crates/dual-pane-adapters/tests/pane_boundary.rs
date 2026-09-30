use std::sync::Arc;

use dual_pane_adapters::{InputController, PanePresenter, UiEvent};
use dual_pane_application::{Command, Output};
use dual_pane_domain::{Entry, EntryKind, EntryName, Location, PaneSide};

#[test]
fn controller_addresses_the_requested_browser_and_presenters_stay_independent() {
    let entry = Entry::new(EntryName::new("docs").unwrap(), EntryKind::Directory);
    let mut presenter = PanePresenter::new(PaneSide::Left);
    presenter.apply(&Output::ListingReplaced { pane: PaneSide::Left, location: Location::root(), entries: Arc::from(vec![entry.clone()]) });
    assert_eq!(InputController::new().command(PaneSide::Left, UiEvent::ActivateRow { row: 0 }, presenter.view()), Some(Command::OpenEntry { pane: PaneSide::Left, row: 0, name: entry.name().clone() }));
    presenter.apply(&Output::LoadingStarted { pane: PaneSide::Left, location: Location::root() });
    assert!(presenter.view().is_loading());

    let mut right = PanePresenter::new(PaneSide::Right);
    right.apply(&Output::ListingReplaced { pane: PaneSide::Right, location: Location::root(), entries: Arc::from(vec![entry.clone()]) });
    assert_eq!(InputController::new().command(PaneSide::Right, UiEvent::ActivateRow { row: 0 }, right.view()), Some(Command::OpenEntry { pane: PaneSide::Right, row: 0, name: entry.name().clone() }));
    assert_eq!(right.view().listing_revision(), 1);
    assert_eq!(presenter.view().listing_revision(), 1);
}
