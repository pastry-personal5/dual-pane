//! Path Edit Control state in the Browser view model, driven by outputs.

use dual_pane_adapters::BrowserPresenter;
use dual_pane_application::{Output, PathEntryFailure, PathRejection};
use dual_pane_domain::{BrowserSide, ListingErrorKind, Location, TabId};

const SIDE: BrowserSide = BrowserSide::Left;

fn showing(tab: u64) -> BrowserPresenter {
    let mut presenter = BrowserPresenter::new(SIDE);
    presenter.apply(&Output::LoadingStarted { browser: SIDE, tab: TabId::new(tab), location: Location::root() });
    presenter
}
fn submit(presenter: &mut BrowserPresenter, tab: u64, text: &str) {
    presenter.note_entered_path(text);
    presenter.apply(&Output::PathEntryPending { browser: SIDE, tab: TabId::new(tab) });
}

#[test]
fn pending_shows_loading_in_the_status_bar() {
    let mut presenter = showing(1);
    submit(&mut presenter, 1, "/x");
    assert!(presenter.view().path_entry().is_pending());
    assert_eq!(presenter.view().status_text(), "Loading…");
}

#[test]
fn a_failure_keeps_its_reason_until_discarded() {
    let mut presenter = showing(1);
    submit(&mut presenter, 1, "/x");
    let before = presenter.view().path_entry().revision();
    presenter.apply(&Output::PathEntryRejected { browser: SIDE, tab: TabId::new(1), reason: PathEntryFailure::Listing(ListingErrorKind::PermissionDenied) });
    let entry = presenter.view().path_entry();
    assert!(!entry.is_pending() && entry.revision() != before);
    assert_eq!(entry.error(), Some("You don’t have permission to open “/x”."));
    assert_eq!(presenter.view().status_text(), "You don’t have permission to open “/x”.");
    presenter.discard_path_entry();
    assert_eq!(presenter.view().path_entry().error(), None);
    assert_ne!(presenter.view().status_text(), "You don’t have permission to open “/x”.");
}

#[test]
fn parse_failures_have_distinct_wording() {
    let mut texts = Vec::new();
    for reason in [PathRejection::Malformed, PathRejection::NoConfirmedFolder, PathRejection::UndecodableText] {
        let mut presenter = showing(1);
        submit(&mut presenter, 1, "x");
        presenter.apply(&Output::PathEntryRejected { browser: SIDE, tab: TabId::new(1), reason: PathEntryFailure::Rejected(reason) });
        texts.push(presenter.view().path_entry().error().unwrap().to_owned());
    }
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), 3);
}

#[test]
fn accepted_and_cancelled_end_the_state() {
    for end in [Output::PathEntryAccepted { browser: SIDE, tab: TabId::new(1) }, Output::PathEntryCancelled { browser: SIDE, tab: TabId::new(1) }] {
        let mut presenter = showing(1);
        submit(&mut presenter, 1, "/x");
        presenter.apply(&end);
        assert!(!presenter.view().path_entry().is_pending());
        assert_eq!(presenter.view().path_entry().tab(), None);
        let accepted = matches!(end, Output::PathEntryAccepted { .. });
        assert_eq!(presenter.view().path_entry().accepted(), u64::from(accepted));
    }
}

#[test]
fn an_unedited_submission_is_accepted_without_a_pending_state() {
    let mut presenter = showing(1);
    let before = presenter.view().path_entry().revision();
    presenter.apply(&Output::PathEntryAccepted { browser: SIDE, tab: TabId::new(1) });
    assert_eq!(presenter.view().path_entry().accepted(), 1);
    assert_ne!(presenter.view().path_entry().revision(), before, "the editor is told to leave editing");
}

#[test]
fn results_for_a_tab_that_is_not_shown_never_touch_the_editor() {
    let mut presenter = showing(1);
    submit(&mut presenter, 1, "/x");
    presenter.apply(&Output::PathEntryRejected { browser: SIDE, tab: TabId::new(2), reason: PathEntryFailure::Listing(ListingErrorKind::ItemMissing) });
    presenter.apply(&Output::PathEntryAccepted { browser: SIDE, tab: TabId::new(2) });
    presenter.apply(&Output::PathEntryCancelled { browser: SIDE, tab: TabId::new(2) });
    assert!(presenter.view().path_entry().is_pending());
    assert_eq!(presenter.view().path_entry().tab(), Some(TabId::new(1)));
}

#[test]
fn a_tab_switch_discards_failed_text() {
    let mut presenter = showing(1);
    submit(&mut presenter, 1, "/x");
    presenter.apply(&Output::PathEntryRejected { browser: SIDE, tab: TabId::new(1), reason: PathEntryFailure::Listing(ListingErrorKind::ItemMissing) });
    presenter.apply(&Output::ActiveTabChanged { browser: SIDE, tab: TabId::new(2) });
    assert_eq!(presenter.view().path_entry().error(), None);
}

#[test]
fn another_browsers_outputs_are_ignored() {
    let mut presenter = showing(1);
    submit(&mut presenter, 1, "/x");
    presenter.apply(&Output::PathEntryCancelled { browser: BrowserSide::Right, tab: TabId::new(1) });
    assert!(presenter.view().path_entry().is_pending());
}
