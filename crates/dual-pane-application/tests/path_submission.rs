use std::sync::Arc;

use dual_pane_application::{Command, Event, FilterStatus, Output, PathEntryFailure, PathRejection, PathSubmission, PathTarget, ResolvedPath, Transition, WorkRequest, Workspace, project_filter};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingErrorKind, Location, RequestToken, TabId};

const LEFT: BrowserSide = BrowserSide::Left;

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}
fn folder(text: &str) -> Location {
    Location::root().join(&name(text))
}
fn entries(names: &[&str]) -> Arc<[Entry]> {
    names.iter().map(|text| Entry::new(name(text), EntryKind::File)).collect()
}
fn read(transition: &Transition) -> Option<(BrowserSide, TabId, RequestToken, Location)> {
    transition.work.iter().find_map(|work| match work {
        WorkRequest::ReadDirectory { browser, tab, token, location, .. } => Some((*browser, *tab, *token, location.clone())),
        _ => None,
    })
}
fn resolve_request(transition: &Transition) -> (TabId, RequestToken, Location) {
    transition
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ResolvePath { tab, token, candidate, .. } => Some((*tab, *token, candidate.clone())),
            _ => None,
        })
        .expect("a resolve request")
}
fn loaded(workspace: &mut Workspace, browser: BrowserSide, location: Location, names: &[&str]) -> TabId {
    let transition = workspace.handle(Command::Navigate { browser, location }.into());
    let (_, tab, token, _) = read(&transition).unwrap();
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries(names), changes: None }.into());
    tab
}
fn submit(workspace: &mut Workspace, tab: TabId, text: &str) -> Transition {
    workspace.handle(Command::SubmitPath { browser: LEFT, tab, submission: PathSubmission::Typed(text.into()) }.into())
}
fn resolved(workspace: &mut Workspace, tab: TabId, token: RequestToken, location: Location, target: PathTarget) -> Transition {
    workspace.handle(Event::PathResolved { browser: LEFT, tab, token, result: Ok(ResolvedPath { location, target }) }.into())
}
fn has(transition: &Transition, wanted: impl Fn(&Output) -> bool) -> bool {
    transition.outputs.iter().any(wanted)
}

#[test]
fn an_absolute_path_is_checked_then_navigates_through_the_ordinary_read() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let submitted = submit(&mut workspace, tab, "/b/c");
    assert!(has(&submitted, |output| matches!(output, Output::PathEntryPending { .. })));
    assert!(read(&submitted).is_none(), "nothing is read before the check answers");
    let (_, token, candidate) = resolve_request(&submitted);
    assert_eq!(candidate, folder("b").join(&name("c")));
    let accepted = resolved(&mut workspace, tab, token, candidate.clone(), PathTarget::Folder);
    assert!(has(&accepted, |output| matches!(output, Output::PathEntryAccepted { .. })));
    assert_eq!(read(&accepted).unwrap().3, candidate);
}

#[test]
fn a_relative_path_resolves_against_the_confirmed_folder() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, _, candidate) = resolve_request(&submit(&mut workspace, tab, "sub/.."));
    assert_eq!(candidate, folder("a"));
}

#[test]
fn a_relative_path_without_a_confirmed_folder_is_refused() {
    let mut workspace = Workspace::new();
    let tab = workspace.browser_chrome(LEFT).active_tab;
    let rejected = submit(&mut workspace, tab, "sub");
    assert!(has(&rejected, |output| matches!(output, Output::PathEntryRejected { reason: PathEntryFailure::Rejected(PathRejection::NoConfirmedFolder), .. })));
    assert!(rejected.work.is_empty());
    let absolute = submit(&mut workspace, tab, "/sub");
    assert!(has(&absolute, |output| matches!(output, Output::PathEntryPending { .. })));
}

#[test]
fn malformed_and_undecodable_submissions_are_rejected_without_work() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    for submission in [PathSubmission::Typed("/a\nb".into()), PathSubmission::Undecodable] {
        let rejected = workspace.handle(Command::SubmitPath { browser: LEFT, tab, submission }.into());
        assert!(has(&rejected, |output| matches!(output, Output::PathEntryRejected { reason: PathEntryFailure::Rejected(PathRejection::Malformed), .. })));
        assert!(rejected.work.is_empty());
    }
    let rejected = submit(&mut workspace, tab, "/a\u{FFFD}");
    assert!(has(&rejected, |output| matches!(output, Output::PathEntryRejected { reason: PathEntryFailure::Rejected(PathRejection::UndecodableText), .. })));
}

#[test]
fn an_unchanged_submission_rereads_the_exact_location_without_a_check() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let transition = workspace.handle(Command::SubmitPath { browser: LEFT, tab, submission: PathSubmission::Unchanged(folder("a")) }.into());
    assert!(has(&transition, |output| matches!(output, Output::PathEntryAccepted { .. })));
    assert_eq!(read(&transition).unwrap().3, folder("a"));
    assert!(!transition.work.iter().any(|work| matches!(work, WorkRequest::ResolvePath { .. })));
}

#[test]
fn a_failure_changes_nothing_in_the_tab() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &["x"]);
    let (_, token, _) = resolve_request(&submit(&mut workspace, tab, "/missing"));
    let before = workspace.browser_chrome(LEFT);
    let failed = workspace.handle(Event::PathResolved { browser: LEFT, tab, token, result: Err(ListingErrorKind::Busy) }.into());
    assert_eq!(failed.outputs.len(), 1);
    assert!(matches!(failed.outputs[0], Output::PathEntryRejected { reason: PathEntryFailure::Listing(ListingErrorKind::Busy), .. }));
    assert!(failed.work.is_empty());
    assert_eq!(workspace.browser_chrome(LEFT).tabs[0].location, before.tabs[0].location);
}

#[test]
fn a_newer_submission_supersedes_without_leaving_pending() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, first, _) = resolve_request(&submit(&mut workspace, tab, "/one"));
    let second = submit(&mut workspace, tab, "/two");
    assert!(second.work.iter().any(|work| matches!(work, WorkRequest::CancelPathResolve { token, .. } if *token == first)));
    assert!(!has(&second, |output| matches!(output, Output::PathEntryCancelled { .. })));
    let (_, newer, _) = resolve_request(&second);
    let late = resolved(&mut workspace, tab, first, folder("one"), PathTarget::Folder);
    assert!(late.outputs.is_empty() && late.work.is_empty(), "a superseded result changes nothing");
    assert!(read(&resolved(&mut workspace, tab, newer, folder("two"), PathTarget::Folder)).is_some());
}

#[test]
fn escape_cancels_the_check_and_a_late_result_changes_nothing() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, _) = resolve_request(&submit(&mut workspace, tab, "/one"));
    let cancelled = workspace.handle(Command::CancelPathEntry { browser: LEFT, tab }.into());
    assert!(has(&cancelled, |output| matches!(output, Output::PathEntryCancelled { .. })));
    assert!(cancelled.work.iter().any(|work| matches!(work, WorkRequest::CancelPathResolve { .. })));
    let late = resolved(&mut workspace, tab, token, folder("one"), PathTarget::Folder);
    assert!(late.outputs.is_empty() && late.work.is_empty());
    assert!(workspace.handle(Command::CancelPathEntry { browser: LEFT, tab }.into()).outputs.is_empty(), "no check, nothing to announce");
}

#[test]
fn a_navigation_the_person_starts_supersedes_the_check() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    for navigate in [Command::Navigate { browser: LEFT, location: folder("other") }, Command::Refresh { browser: LEFT, tab }, Command::GoToParent { browser: LEFT, tab }, Command::GoBack { browser: LEFT, tab }] {
        let (_, token, _) = resolve_request(&submit(&mut workspace, tab, "/one"));
        let transition = workspace.handle(navigate.into());
        assert!(has(&transition, |output| matches!(output, Output::PathEntryCancelled { .. })));
        assert!(resolved(&mut workspace, tab, token, folder("one"), PathTarget::Folder).work.is_empty());
    }
}

#[test]
fn watcher_rereads_do_not_cancel_the_check() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, _) = resolve_request(&submit(&mut workspace, tab, "/one"));
    let invalidated = workspace.handle(Event::LocationInvalidated { location: folder("a") }.into());
    assert!(!has(&invalidated, |output| matches!(output, Output::PathEntryCancelled { .. })));
    assert!(read(&resolved(&mut workspace, tab, token, folder("one"), PathTarget::Folder)).is_some());
}

#[test]
fn closing_the_tab_cancels_the_check_and_ignores_its_result() {
    let mut workspace = Workspace::new();
    workspace.handle(Command::NewTab { browser: LEFT }.into());
    let tab = workspace.browser_chrome(LEFT).active_tab;
    let (_, token, _) = resolve_request(&submit(&mut workspace, tab, "/one"));
    let closed = workspace.handle(Command::CloseTab { browser: LEFT, tab }.into());
    assert!(closed.work.iter().any(|work| matches!(work, WorkRequest::CancelPathResolve { .. })));
    assert!(!has(&closed, |output| matches!(output, Output::PathEntryCancelled { .. })));
    let late = resolved(&mut workspace, tab, token, folder("one"), PathTarget::Folder);
    assert!(late.outputs.is_empty() && late.work.is_empty());
}

#[test]
fn a_submission_for_a_tab_that_is_not_active_is_ignored() {
    let mut workspace = Workspace::new();
    let first = loaded(&mut workspace, LEFT, folder("a"), &[]);
    workspace.handle(Command::NewTab { browser: LEFT }.into());
    let ignored = submit(&mut workspace, first, "/one");
    assert!(ignored.outputs.is_empty() && ignored.work.is_empty());
}

#[test]
fn a_result_for_an_inactive_tab_navigates_that_tab_in_the_background() {
    let mut workspace = Workspace::new();
    let first = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, _) = resolve_request(&submit(&mut workspace, first, "/one"));
    workspace.handle(Command::NewTab { browser: LEFT }.into());
    let second = workspace.browser_chrome(LEFT).active_tab;
    let accepted = resolved(&mut workspace, first, token, folder("one"), PathTarget::Folder);
    let (_, read_tab, _, location) = read(&accepted).unwrap();
    assert_eq!((read_tab, location), (first, folder("one")));
    assert_eq!(workspace.browser_chrome(LEFT).active_tab, second);
}

#[test]
fn a_failure_for_an_inactive_tab_is_reported_for_that_tab_only() {
    let mut workspace = Workspace::new();
    let first = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, _) = resolve_request(&submit(&mut workspace, first, "/one"));
    workspace.handle(Command::NewTab { browser: LEFT }.into());
    let failed = workspace.handle(Event::PathResolved { browser: LEFT, tab: first, token, result: Err(ListingErrorKind::ItemMissing) }.into());
    assert!(matches!(failed.outputs.as_slice(), [Output::PathEntryRejected { tab, .. }] if *tab == first));
}

#[test]
fn a_file_opens_its_folder_with_the_file_selected() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, candidate) = resolve_request(&submit(&mut workspace, tab, "/b/wanted"));
    let accepted = resolved(&mut workspace, tab, token, candidate, PathTarget::File);
    let (browser, tab, read_token, location) = read(&accepted).unwrap();
    assert_eq!(location, folder("b"));
    let done = workspace.handle(Event::FolderItemsLoaded { browser, tab, token: read_token, entries: entries(&["other", "wanted"]), changes: None }.into());
    assert!(has(&done, |output| matches!(output, Output::SelectionChanged { row: Some(1), .. })));
}

#[test]
fn a_file_missing_from_its_folder_opens_it_with_nothing_selected() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, candidate) = resolve_request(&submit(&mut workspace, tab, "/b/wanted"));
    let (browser, tab, read_token, _) = read(&resolved(&mut workspace, tab, token, candidate, PathTarget::File)).unwrap();
    let done = workspace.handle(Event::FolderItemsLoaded { browser, tab, token: read_token, entries: entries(&["other"]), changes: None }.into());
    assert!(!has(&done, |output| matches!(output, Output::SelectionChanged { row: Some(_), .. })));
}

#[test]
fn a_file_hidden_by_item_visibility_opens_its_folder_without_selection() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, candidate) = resolve_request(&submit(&mut workspace, tab, "/b/.hidden"));
    let (browser, tab, read_token, location) = read(&resolved(&mut workspace, tab, token, candidate, PathTarget::File)).unwrap();
    assert_eq!(location, folder("b"));
    // The native reader applies Item Visibility before delivering entries.
    let done = workspace.handle(Event::FolderItemsLoaded { browser, tab, token: read_token, entries: entries(&["visible"]), changes: None }.into());
    assert!(has(&done, |output| matches!(output, Output::SelectionChanged { row: None, selection, .. } if selection.entries().is_empty())));
    assert_eq!(workspace.browser_chrome(LEFT).location, Some(folder("b")));
}

#[test]
fn a_background_file_result_selects_in_its_own_tab_after_another_writer_ran() {
    let mut workspace = Workspace::new();
    let first = loaded(&mut workspace, LEFT, folder("a"), &[]);
    let (_, token, candidate) = resolve_request(&submit(&mut workspace, first, "/b/wanted"));
    workspace.handle(Command::NewTab { browser: LEFT }.into());
    let accepted = resolved(&mut workspace, first, token, candidate, PathTarget::File);
    let (browser, tab, read_token, _) = read(&accepted).unwrap();
    assert_eq!(tab, first);
    let done = workspace.handle(Event::FolderItemsLoaded { browser, tab, token: read_token, entries: entries(&["wanted"]), changes: None }.into());
    assert!(has(&done, |output| matches!(output, Output::SelectionChanged { row: Some(0), .. })));
}

#[test]
fn simultaneous_file_paths_keep_a_pending_selection_per_tab() {
    let mut workspace = Workspace::new();
    let left_tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &[]);
    let right_tab = loaded(&mut workspace, BrowserSide::Right, folder("c"), &[]);
    let left_submission = submit(&mut workspace, left_tab, "/b/left-file");
    let (_, left_token, left_candidate) = resolve_request(&left_submission);
    let right_submission = workspace.handle(Command::SubmitPath { browser: BrowserSide::Right, tab: right_tab, submission: PathSubmission::Typed("/d/right-file".into()) }.into());
    let (_, right_token, right_candidate) = resolve_request(&right_submission);

    let left_accepted = resolved(&mut workspace, left_tab, left_token, left_candidate, PathTarget::File);
    let (_, _, left_read, _) = read(&left_accepted).unwrap();
    let right_accepted = workspace.handle(Event::PathResolved { browser: BrowserSide::Right, tab: right_tab, token: right_token, result: Ok(ResolvedPath { location: right_candidate, target: PathTarget::File }) }.into());
    let (_, _, right_read, _) = read(&right_accepted).unwrap();

    let right_done = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: right_tab, token: right_read, entries: entries(&["right-file"]), changes: None }.into());
    let left_done = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: left_tab, token: left_read, entries: entries(&["left-file"]), changes: None }.into());
    assert!(has(&right_done, |output| matches!(output, Output::SelectionChanged { row: Some(0), .. })));
    assert!(has(&left_done, |output| matches!(output, Output::SelectionChanged { row: Some(0), .. })));
}

#[test]
fn a_filter_that_hides_the_file_is_cleared_but_one_that_shows_it_stays() {
    for (filter_text, cleared) in [("zzz", true), ("wan", false)] {
        let mut workspace = Workspace::new();
        let tab = loaded(&mut workspace, LEFT, folder("a"), &["wanted", "other"]);
        let filtered = workspace.handle(Command::SetFilterText { browser: LEFT, tab, text: filter_text.into() }.into());
        let request = filtered.work.iter().find_map(|work| if let WorkRequest::ProjectFilter { request } = work { Some(request.clone()) } else { None }).unwrap();
        let result = project_filter(&request);
        workspace.handle(Event::FilterProjected { request, result: Ok(result) }.into());
        let (_, token, candidate) = resolve_request(&submit(&mut workspace, tab, "/a/wanted"));
        let accepted = resolved(&mut workspace, tab, token, candidate, PathTarget::File);
        let shows_all = accepted.outputs.iter().any(|output| matches!(output, Output::VisibleItemsChanged { filter_status: FilterStatus::Ready, total_count: 2, entries, .. } if entries.len() == 2));
        assert_eq!(shows_all, cleared, "filter {filter_text:?}");
    }
}

#[test]
fn an_error_tab_accepts_an_absolute_path_and_refuses_a_relative_one() {
    let mut workspace = Workspace::new();
    let transition = workspace.handle(Command::Navigate { browser: LEFT, location: folder("gone") }.into());
    let (_, tab, token, _) = read(&transition).unwrap();
    workspace.handle(Event::FolderItemsFailed { browser: LEFT, tab, token, kind: ListingErrorKind::ItemMissing }.into());
    let relative = submit(&mut workspace, tab, "inside");
    assert!(has(&relative, |output| matches!(output, Output::PathEntryRejected { reason: PathEntryFailure::Rejected(PathRejection::NoConfirmedFolder), .. })));
    assert!(relative.work.is_empty());
    let absolute = submit(&mut workspace, tab, "/b");
    let (_, check, candidate) = resolve_request(&absolute);
    let accepted = resolved(&mut workspace, tab, check, candidate.clone(), PathTarget::Folder);
    assert_eq!(read(&accepted).unwrap().3, candidate, "typing an absolute path is the way out of an error");
}
