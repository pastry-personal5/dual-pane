use std::sync::Arc;

use dual_pane_adapters::{PanePresenter, RowKind, RowViewModel};
use dual_pane_application::Output;
use dual_pane_domain::{Entry, EntryKind, EntryName, ListingError, ListingErrorKind, Location, Selection};

fn name(bytes: &[u8]) -> EntryName {
    EntryName::new(bytes.to_vec()).unwrap()
}

fn location(components: &[&str]) -> Location {
    Location::from_components(components.iter().map(|c| name(c.as_bytes())))
}

#[test]
fn an_empty_pane_shows_nothing() {
    let presenter = PanePresenter::new();
    assert_eq!(presenter.view().location_text(), "");
    assert!(!presenter.view().is_loading());
    assert_eq!(presenter.view().error(), None);
    assert_eq!(presenter.view().status_text(), "");
    assert_eq!(presenter.view().row_count(), 0);
    assert_eq!(presenter.view().selected_row(), None);
}

#[test]
fn selection_changes_only_from_explicit_application_output() {
    let mut presenter = PanePresenter::new();
    presenter.apply(&Output::ListingReplaced { location: Location::root(), entries: Arc::from(vec![Entry::new(name(b"first"), EntryKind::File)]) });
    let mut selection = Selection::default();
    selection.select(name(b"first"));
    presenter.apply(&Output::SelectionChanged { selection: selection.clone(), row: Some(0) });
    assert_eq!(presenter.view().selection(), &selection);
    assert_eq!(presenter.view().selected_row(), Some(0));

    presenter.apply(&Output::ListingReplaced { location: location(&["next"]), entries: Arc::from(Vec::new()) });
    assert_eq!(presenter.view().selection(), &selection);
    assert_eq!(presenter.view().selected_row(), Some(0));

    presenter.apply(&Output::SelectionChanged { selection: Selection::default(), row: None });
    assert_eq!(presenter.view().selected_row(), None);
}

#[test]
fn a_listing_shows_its_location_and_rows_on_demand() {
    let entries: Arc<[Entry]> = Arc::from(vec![Entry::new(name(b"docs"), EntryKind::Directory), Entry::new(name(b"link"), EntryKind::Symlink { points_to_directory: true }), Entry::new(name(b"broken"), EntryKind::Symlink { points_to_directory: false }), Entry::new(name(b"caf\xFF"), EntryKind::File), Entry::new(name(b"socket"), EntryKind::Other)]);
    let mut presenter = PanePresenter::new();
    presenter.apply(&Output::ListingReplaced { location: location(&["Users", "me"]), entries });
    let view = presenter.view();
    assert_eq!(view.location_text(), "/Users/me");
    assert_eq!(view.row_count(), 5);
    let rows: Vec<_> = (0..view.row_count()).filter_map(|i| view.row(i)).collect();
    let expected = [("docs", RowKind::Folder), ("link", RowKind::FolderLink), ("broken", RowKind::Link), ("caf\u{FFFD}", RowKind::File), ("socket", RowKind::Other)].map(|(name, kind)| RowViewModel { name: name.to_owned(), kind });
    assert_eq!(rows, expected);
    assert_eq!(view.row(5), None);
}

#[test]
fn the_root_is_shown_as_a_slash() {
    let mut presenter = PanePresenter::new();
    presenter.apply(&Output::ListingReplaced { location: Location::root(), entries: Arc::from(Vec::new()) });
    assert_eq!(presenter.view().location_text(), "/");
}

#[test]
fn loading_keeps_the_current_listing_and_clears_the_last_error() {
    let mut presenter = PanePresenter::new();
    presenter.apply(&Output::ListingReplaced { location: location(&["a"]), entries: Arc::from(vec![Entry::new(name(b"x"), EntryKind::File)]) });
    presenter.apply(&Output::ListingFailed { error: ListingError::new(location(&["a", "b"]), ListingErrorKind::Unknown) });
    presenter.apply(&Output::LoadingStarted { location: location(&["a", "c"]) });
    let view = presenter.view();
    assert!(view.is_loading());
    assert_eq!(view.error(), None);
    assert_eq!(view.location_text(), "/a");
    assert_eq!(view.row_count(), 1);
}

#[test]
fn a_failure_keeps_the_listing_and_explains_each_error_kind() {
    let cases = [(ListingErrorKind::ItemMissing, "“/a/b” no longer exists."), (ListingErrorKind::NotADirectory, "“/a/b” is not a folder."), (ListingErrorKind::PermissionDenied, "You don’t have permission to open “/a/b”."), (ListingErrorKind::PrivacyRestricted, "macOS privacy settings don’t allow Dual Pane to open “/a/b”."), (ListingErrorKind::Internal, "Dual Pane couldn’t finish reading this folder unexpectedly."), (ListingErrorKind::Unknown, "“/a/b” couldn’t be opened.")];
    for (kind, message) in cases {
        let mut presenter = PanePresenter::new();
        presenter.apply(&Output::ListingReplaced { location: location(&["a"]), entries: Arc::from(vec![Entry::new(name(b"x"), EntryKind::File)]) });
        presenter.apply(&Output::LoadingStarted { location: location(&["a", "b"]) });
        presenter.apply(&Output::ListingFailed { error: ListingError::new(location(&["a", "b"]), kind) });
        let view = presenter.view();
        assert!(!view.is_loading());
        assert_eq!(view.error(), Some(message));
        assert_eq!(view.location_text(), "/a");
        assert_eq!(view.row_count(), 1);
    }
}

#[test]
fn cancellation_clears_loading_without_a_message() {
    let mut presenter = PanePresenter::new();
    presenter.apply(&Output::ListingReplaced { location: location(&["a"]), entries: Arc::from(Vec::new()) });
    presenter.apply(&Output::LoadingStarted { location: location(&["a", "b"]) });
    presenter.apply(&Output::ListingCancelled);
    assert!(!presenter.view().is_loading());
    assert_eq!(presenter.view().error(), None);
    assert_eq!(presenter.view().status_text(), "/a");
}
