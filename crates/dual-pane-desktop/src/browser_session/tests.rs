use std::sync::Arc;

use dual_pane_adapters::{FavoritesEvent, SelectionMovement};
use dual_pane_application::{Command, Event, WorkRequest, project_filter};
use dual_pane_domain::OperationCommand;
use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, RequestToken, Selection, SortSpec, TabId};

use super::*;

#[derive(Default)]
struct FakeRunner {
    dispatched: Vec<WorkRequest>,
    delivered: VecDeque<Event>,
}

impl WorkRunner for FakeRunner {
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
        self.dispatched.push(request);
        None
    }

    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
        let count = max.min(self.delivered.len());
        (self.delivered.drain(..count).collect(), !self.delivered.is_empty())
    }
}

fn entry(name: &str) -> Entry {
    Entry::new(EntryName::new(name).unwrap(), EntryKind::Directory)
}

fn path(name: &str) -> Location {
    Location::root().join(&EntryName::new(name).unwrap())
}

fn assert_view(session: &WorkspaceSession<FakeRunner>, browser: BrowserSide, expected: (&str, &[&str], Option<&str>, &str, bool, u64)) {
    let (path, rows, selected, status, loading, revision) = expected;
    let view = session.view(browser);
    assert_eq!(view.location_text(), path);
    assert_eq!(view.row_count(), rows.len());
    for (index, name) in rows.iter().enumerate() {
        assert_eq!(view.row(index).map(|row| row.name), Some((*name).to_owned()));
    }
    let expected_row = selected.and_then(|name| rows.iter().position(|row| *row == name));
    assert_eq!(view.cursor_row(), expected_row);
    let mut selection = Selection::default();
    if let Some(name) = selected {
        selection.select(EntryName::new(name).unwrap());
    }
    assert_eq!(view.selection(), &selection);
    assert_eq!(view.status_text(), status);
    assert_eq!(view.is_loading(), loading);
    assert_eq!(view.folder_items_revision(), revision);
}

#[test]
fn delayed_native_gestures_keep_the_revision_of_the_displayed_rows() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha"), entry("beta")]), changes: None });
    let old_view = session.view(BrowserSide::Left).clone();
    session.submit(Command::SetFilterText { browser: BrowserSide::Left, tab: TabId::new(0), text: "alpha".into() });
    let request = session
        .runner
        .dispatched
        .iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .expect("filter projection");
    session.submit(Event::FilterProjected { result: Ok(project_filter(&request)), request });

    session.submit_observed_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 }, old_view.clone());
    assert!(session.view(BrowserSide::Left).selection().entries().is_empty(), "an old row gesture cannot select after projection");
    session.submit_ui_at_observation(BrowserSide::Left, UiEvent::FileCommand(OperationCommand::Copy), old_view.active_tab().unwrap().value(), old_view.visible_revision());
    assert!(session.operations.view().panels().is_empty(), "an old selection command cannot start after projection");
}

#[test]
fn delayed_filter_edits_cannot_retarget_a_different_active_tab() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    let first = session.view(BrowserSide::Left).active_tab().unwrap();
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, token: RequestToken::first(), entries: Arc::from(vec![entry("first")]), changes: None });
    session.submit(Command::NewTab { browser: BrowserSide::Left });
    let second = session.view(BrowserSide::Left).active_tab().unwrap();
    assert_ne!(first, second);
    let token = session
        .runner
        .dispatched
        .iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, token, .. } if *tab == second => Some(*token),
            _ => None,
        })
        .expect("new tab read");
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: second, token, entries: Arc::from(vec![entry("alpha")]), changes: None });

    session.set_filter_text_observed(BrowserSide::Left, first.value(), "wrong tab".into());
    assert_eq!(session.view(BrowserSide::Left).active_tab(), Some(second));
    assert_eq!(session.view(BrowserSide::Left).filter_text(), "");

    session.set_filter_text_observed(BrowserSide::Left, second.value(), "right tab".into());
    assert_eq!(session.view(BrowserSide::Left).filter_text(), "right tab");
    session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: first });
    assert_eq!(session.view(BrowserSide::Left).filter_text(), "");
}

#[test]
fn loading_failure_cancellation_and_late_results_preserve_the_other_browser() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 0));
    assert_view(&session, BrowserSide::Right, ("", &[], None, "Loading…", true, 0));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]), changes: None });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]), changes: None });
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("older") });
    let older = RequestToken::first().next().next();
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("missing") });
    let missing = older.next();
    assert!(session.runner.dispatched.contains(&WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: older }));
    session.submit(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: older });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: older, entries: Arc::from(vec![entry("late")]), changes: None });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: missing, entries: Arc::from(vec![entry("wrong")]), changes: None });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "Loading…", true, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: missing, kind: ListingErrorKind::ItemMissing });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "“/missing” no longer exists.", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("cancelled") });
    let cancelled = missing.next();
    session.submit(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: cancelled });
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("final") });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: cancelled.next(), entries: Arc::from(vec![entry("fresh")]), changes: None });
    assert_view(&session, BrowserSide::Left, ("/final", &["fresh"], None, "/final", false, 2));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
}

#[test]
fn bounded_drain_applies_one_browsers_result_without_touching_the_other() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), 1, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.runner.delivered.push_back(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]), changes: None });
    session.runner.delivered.push_back(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]), changes: None });
    assert!(session.drain());
    assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 0));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], None, "/base", false, 1));
    assert!(!session.drain());
    assert_view(&session, BrowserSide::Left, ("/base", &["left"], None, "/base", false, 1));
    assert_view(&session, BrowserSide::Right, ("/base", &["right"], None, "/base", false, 1));
}

#[test]
fn start_dispatches_distinct_reads_for_both_browsers() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(Location::root());
    let reads = session.runner.dispatched.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).cloned().collect::<Vec<_>>();
    assert_eq!(reads, vec![WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), location: Location::root(), sort: SortSpec::default(), visibility: Default::default(), previous: None }, WorkRequest::ReadDirectory { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), location: Location::root(), sort: SortSpec::default(), visibility: Default::default(), previous: None }]);
}

#[test]
fn clipboard_menu_availability_tracks_selection_destination_and_the_observed_offer() {
    use dual_pane_application::{ClipboardAction, ClipboardGeneration, ClipboardOffer};

    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha"), entry("beta")]), changes: None });
    let copy = Some(ClipboardAction::Copy);
    let cut = Some(ClipboardAction::Cut);
    assert!(!session.clipboard_command_available(BrowserSide::Left, copy) && !session.clipboard_command_available(BrowserSide::Left, cut), "Copy and Cut need a selection");
    assert!(!session.clipboard_command_available(BrowserSide::Left, None), "Paste needs a supported clipboard offer");

    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    assert!(session.clipboard_command_available(BrowserSide::Left, copy) && session.clipboard_command_available(BrowserSide::Left, cut));

    session.observe_clipboard(ClipboardGeneration::new(1), ClipboardOffer::Files);
    assert!(session.clipboard_command_available(BrowserSide::Left, None), "a ready destination and a file offer enable Paste");
    assert!(!session.clipboard_command_available(BrowserSide::Right, None), "a loading destination disables Paste");

    session.observe_clipboard(ClipboardGeneration::new(2), ClipboardOffer::Unsupported);
    assert!(!session.clipboard_command_available(BrowserSide::Left, None), "replacement by unsupported content disables Paste");
}

#[test]
fn an_observed_paste_reaches_the_clipboard_read() {
    use dual_pane_application::{ClipboardGeneration, ClipboardOffer};

    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("base"));
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("alpha")]), changes: None });
    session.observe_clipboard(ClipboardGeneration::new(1), ClipboardOffer::Files);
    session.submit_clipboard_observed(BrowserSide::Left, None, TabId::new(0), 0);
    assert!(session.runner.dispatched.iter().any(|request| matches!(request, WorkRequest::ReadClipboard { .. })), "Paste must start a clipboard read");
}

#[test]
fn folder_items_revisions_and_selection_are_browser_local() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(Location::root());
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]), changes: None });
    assert_eq!(session.view(BrowserSide::Left).folder_items_revision(), 1);
    assert_eq!(session.view(BrowserSide::Right).folder_items_revision(), 0);

    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]), changes: None });
    session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
    assert_eq!(session.view(BrowserSide::Left).folder_items_revision(), 1);
    assert_eq!(session.view(BrowserSide::Right).folder_items_revision(), 1);
    assert_eq!(session.view(BrowserSide::Left).cursor_row(), None);
    assert_eq!(session.view(BrowserSide::Right).cursor_row(), Some(0));
}

#[test]
fn vertical_arrow_movement_obeys_empty_unselected_and_boundary_cases() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(Location::root());

    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Previous));
    assert_eq!(session.view(BrowserSide::Left).cursor_row(), None);

    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("first"), entry("second"), entry("third")]), changes: None });
    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Previous));
    assert_eq!(session.view(BrowserSide::Left).cursor_row(), Some(0));
    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Previous));
    assert_eq!(session.view(BrowserSide::Left).cursor_row(), Some(0));
    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Next));
    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Next));
    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Next));
    assert_eq!(session.view(BrowserSide::Left).cursor_row(), Some(2));
    assert_eq!(session.view(BrowserSide::Right).cursor_row(), None);
}

#[test]
fn movement_at_a_boundary_collapses_a_range_selection() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(Location::root());
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("first"), entry("second")]), changes: None });
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    session.submit_ui(BrowserSide::Left, UiEvent::SelectAll);
    assert_eq!(session.view(BrowserSide::Left).selection().entries().len(), 2);
    session.submit_ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Previous));
    assert_eq!(session.view(BrowserSide::Left).selection().entries(), &[EntryName::new("first").unwrap()]);
    assert_eq!(session.view(BrowserSide::Left).cursor_row(), Some(0));
}

#[test]
fn horizontal_arrow_actions_navigate_the_addressed_browser() {
    let folder = EntryName::new("folder").unwrap();
    let location = Location::root().join(&EntryName::new("parent").unwrap());
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(location.clone());
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("folder")]), changes: None });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("folder")]), changes: None });
    session.runner.dispatched.clear();

    session.submit_ui(BrowserSide::Left, UiEvent::GoToParent);
    session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
    session.submit_ui(BrowserSide::Right, UiEvent::ActivateSelection);

    let reads = session.runner.dispatched.iter().filter(|work| matches!(work, WorkRequest::ReadDirectory { .. })).cloned().collect::<Vec<_>>();
    assert_eq!(reads, vec![WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first().next().next(), location: Location::root(), sort: SortSpec::default(), visibility: Default::default(), previous: None }, WorkRequest::ReadDirectory { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next().next().next(), location: location.join(&folder), sort: SortSpec::default(), visibility: Default::default(), previous: None }]);
}

#[test]
fn switching_tabs_restores_the_visible_listing_and_ignores_inactive_results() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("first"));
    let first = session.workspace.active_tab(BrowserSide::Left);
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, token: RequestToken::first(), entries: Arc::from(vec![entry("one")]), changes: None });
    session.submit(Command::NewTab { browser: BrowserSide::Left });
    let second = session.workspace.active_tab(BrowserSide::Left);
    assert_ne!(first, second);
    assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 2));

    session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: first });
    assert_view(&session, BrowserSide::Left, ("/first", &["one"], None, "/first", false, 3));
    let token = match session.runner.dispatched.last() {
        Some(WorkRequest::ReadDirectory { token, .. }) => *token,
        _ => panic!("new tab read"),
    };
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: second, token, entries: Arc::from(vec![entry("two")]), changes: None });
    assert_view(&session, BrowserSide::Left, ("/first", &["one"], None, "/first", false, 3));

    session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: second });
    assert_view(&session, BrowserSide::Left, ("/first", &["two"], None, "/first", false, 4));

    session.submit_ui(BrowserSide::Left, UiEvent::Refresh);
    let refresh = match session.runner.dispatched.last() {
        Some(WorkRequest::ReadDirectory { token, .. }) => *token,
        _ => panic!("refresh read"),
    };
    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: second, token: refresh, kind: ListingErrorKind::PermissionDenied });
    session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: first });
    session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: second });
    assert_view(&session, BrowserSide::Left, ("/first", &["two"], None, "You don’t have permission to open “/first”.", false, 6));
}

#[test]
fn activating_a_file_opens_it_and_a_refusal_opens_notices() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    let opened = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = Arc::clone(&opened);
    session.opener = Box::new(move |item| {
        record.lock().unwrap().push(item.clone());
        item.components().last().is_some_and(|name| name.as_bytes() != b"refused.txt")
    });
    session.start(path("items"));
    let file = |name: &str| Entry::new(EntryName::new(name).unwrap(), EntryKind::File);
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![file("opens.txt"), file("refused.txt")]), changes: None });
    session.submit_ui(BrowserSide::Left, UiEvent::ActivateRow { row: 0 });
    assert_eq!(session.workspace_view().open_requests(), 1, "the startup storage failure opens Notices while the default preference permits it");
    session.submit_ui(BrowserSide::Left, UiEvent::ActivateRow { row: 1 });
    assert_eq!(*opened.lock().unwrap(), vec![path("items").join(&EntryName::new("opens.txt").unwrap()), path("items").join(&EntryName::new("refused.txt").unwrap())]);
    assert_eq!(session.workspace_view().open_requests(), 2, "a refused open keeps its normal Notices behavior after the startup failure");
    assert!(session.workspace_view().notices().iter().any(|notice| notice.text == "Couldn’t open “/items/refused.txt”."));
}

#[test]
fn committing_a_name_starts_a_job_or_keeps_the_editor_open_with_an_error() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    let file = |name: &str| Entry::new(EntryName::new(name).unwrap(), EntryKind::File);
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![file("a")]), changes: None });
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    assert_eq!(session.commit_name(BrowserSide::Left, OperationCommand::Rename, "   ", None), Err("Enter a name."));
    assert_eq!(session.commit_name(BrowserSide::Left, OperationCommand::Rename, "a/b", None), Err(UNREPRESENTABLE_NAME_TEXT));
    assert_eq!(session.commit_name(BrowserSide::Left, OperationCommand::Rename, "a", None), Ok(None), "an unchanged name closes the editor");
    let Ok(Some(job)) = session.commit_name(BrowserSide::Left, OperationCommand::Rename, "b", None) else { panic!("a job") };
    assert_eq!(session.editor_outcome(job), EditorOutcome::Pending);
    let generation = session.workspace.operation_jobs()[0].generation();
    session.submit(Event::OperationScanned { id: job, generation, plan: Ok(Arc::from(vec![dual_pane_application::PlannedItem { source: path("items").join(&EntryName::new("a").unwrap()), destination: Some(path("items").join(&EntryName::new("b").unwrap())), kind: EntryKind::File }])) });
    let generation = session.workspace.operation_jobs()[0].generation();
    session.submit(Event::OperationStepped { id: job, generation, result: dual_pane_application::StepResult::NameCollision { item: path("items").join(&EntryName::new("a").unwrap()), progress: dual_pane_application::OperationProgress::default() } });
    assert_eq!(session.editor_outcome(job), EditorOutcome::Rejected("An item with this name already exists."));
    let Ok(Some(retry)) = session.commit_name(BrowserSide::Left, OperationCommand::Rename, "c", Some(job)) else { panic!("a retry") };
    assert_ne!(retry, job);
    assert!(session.workspace.operation_jobs().iter().all(|other| other.id() != job), "the refused job closed and was dismissed");
}

#[test]
fn a_job_that_finishes_before_its_panel_shows_is_dismissed_and_quitting_cancels_running_jobs() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    let file = |name: &str| Entry::new(EntryName::new(name).unwrap(), EntryKind::File);
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![file("a")]), changes: None });
    session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![]), changes: None });
    session.submit(Command::Navigate { browser: BrowserSide::Right, location: path("other") });
    deliver(&mut session, BrowserSide::Right, &[]);
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    assert!(session.file_command_available(BrowserSide::Left, OperationCommand::Copy));
    assert!(!session.file_command_available(BrowserSide::Right, OperationCommand::Copy));
    session.submit_ui(BrowserSide::Left, UiEvent::FileCommand(OperationCommand::Copy));
    let id = session.workspace.operation_jobs()[0].id();
    assert!(!session.request_quit(false), "a running copy needs confirmation");
    assert_eq!(session.operations_view().quit_prompt(), Some(1));
    assert!(session.request_quit(true));
    let generation = session.workspace.operation_jobs()[0].generation();
    session.submit(Event::OperationCleaned { id, generation, result: dual_pane_application::CleanupResult::Clean });
    assert!(session.workspace.operation_jobs().is_empty(), "the cancelled job, never shown, is dismissed with its Notice");
    assert!(session.runner.dispatched.contains(&WorkRequest::Operation(dual_pane_application::OperationEffect::Release { id })));
}

#[test]
fn closing_the_final_tab_reads_the_supplied_home() {
    let home = path("synthetic-home");
    let mut session = WorkspaceSession::new(FakeRunner::default(), home.clone(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("start"));
    let old = session.workspace.active_tab(BrowserSide::Left);
    session.submit(Command::CloseTab { browser: BrowserSide::Left, tab: old });
    let new = session.workspace.active_tab(BrowserSide::Left);
    assert_ne!(old, new);
    assert!(session.runner.dispatched.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, location, .. } if *tab == new && *location == home)));
}

/// Completes the latest read `browser` requested with `names` as folders.
fn deliver(session: &mut WorkspaceSession<FakeRunner>, browser: BrowserSide, names: &[&str]) {
    let Some(WorkRequest::ReadDirectory { tab, token, .. }) = session.runner.dispatched.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { browser: requested, .. } if *requested == browser)).cloned() else { panic!("a read") };
    session.submit(Event::FolderItemsLoaded { browser, tab, token, entries: names.iter().map(|name| entry(name)).collect(), changes: None });
}

#[test]
fn a_sort_in_one_browser_rereads_inactive_tabs_at_that_location_in_both() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("shared"));
    deliver(&mut session, BrowserSide::Left, &["a"]);
    deliver(&mut session, BrowserSide::Right, &["a"]);
    session.submit_ui(BrowserSide::Left, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Left, &["a"]);
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("other") });
    deliver(&mut session, BrowserSide::Left, &[]);
    session.runner.dispatched.clear();
    let size = SortSpec::new(dual_pane_domain::SortField::Size, dual_pane_domain::SortDirection::Descending);
    session.submit_ui(BrowserSide::Right, UiEvent::Sort(size));
    let reads = session.runner.dispatched.iter().filter_map(|work| match work {
        WorkRequest::ReadDirectory { browser, location, sort, .. } if *sort == size => Some((*browser, location.clone())),
        _ => None,
    });
    assert_eq!(reads.collect::<Vec<_>>(), vec![(BrowserSide::Left, path("shared")), (BrowserSide::Right, path("shared"))], "the inactive Left tab rereads; the active one elsewhere does not");
    assert_eq!(session.view(BrowserSide::Right).toolbar().sort, Some(size));
    assert_eq!(session.view(BrowserSide::Left).toolbar().sort, Some(SortSpec::default()), "the Left active tab shows another folder");
}

#[test]
fn selection_cursor_and_scroll_return_with_their_tab() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    deliver(&mut session, BrowserSide::Left, &["a", "b", "c", "d"]);
    let first = session.view(BrowserSide::Left).active_tab().unwrap();
    session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 1 });
    session.submit_ui(BrowserSide::Left, UiEvent::ExtendToRow { row: 2 });
    session.submit_ui(BrowserSide::Left, UiEvent::Scrolled { row: Some(2), offset: 4 });
    session.submit_ui(BrowserSide::Left, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Left, &["a", "b", "c", "d"]);
    assert!(session.view(BrowserSide::Left).selected_rows().is_empty(), "a new tab starts clean");
    session.submit_ui(BrowserSide::Left, UiEvent::ActivateTab { tab: first });
    let view = session.view(BrowserSide::Left);
    assert_eq!((view.selected_rows(), view.cursor_row(), view.scroll_hint_row()), (vec![1, 2], Some(2), Some((2, 4))));
    assert!(session.view(BrowserSide::Right).selected_rows().is_empty(), "the other Browser is untouched");
}

#[test]
fn a_scroll_report_does_not_activate_its_browser() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    deliver(&mut session, BrowserSide::Left, &["a", "b"]);
    deliver(&mut session, BrowserSide::Right, &["a", "b"]);
    session.submit_ui(BrowserSide::Left, UiEvent::FocusBrowser);
    session.submit_view_state(BrowserSide::Right, UiEvent::Scrolled { row: Some(1), offset: 3 });
    assert_eq!(session.workspace.active_browser(), BrowserSide::Left);
    assert_eq!(session.workspace_view().active_browser(), BrowserSide::Left);
    let right = session.view(BrowserSide::Right).active_tab().unwrap();
    session.submit_ui(BrowserSide::Right, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Right, &[]);
    session.submit_ui(BrowserSide::Right, UiEvent::ActivateTab { tab: right });
    assert_eq!(session.view(BrowserSide::Right).scroll_hint_row(), Some((1, 3)), "the scroll is still recorded");
}

#[test]
fn a_closed_tab_cannot_receive_a_delayed_gesture() {
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    session.start(path("items"));
    deliver(&mut session, BrowserSide::Left, &["a", "b"]);
    let stale = session.controller.command(BrowserSide::Left, UiEvent::SelectRow { row: 0 }, session.view(BrowserSide::Left)).unwrap();
    session.submit_ui(BrowserSide::Left, UiEvent::NewTab);
    deliver(&mut session, BrowserSide::Left, &["a", "b"]);
    session.submit(stale);
    assert!(session.view(BrowserSide::Left).selected_rows().is_empty(), "a gesture observed on the first tab does not select in the second");
}

#[test]
fn stored_favorites_are_probed_on_the_runner_and_an_unavailable_item_adds_a_notice() {
    use dual_pane_application::{FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoritesRecords, SettingsSnapshot};
    let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
    let favorites = FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "Places".into(), position: 0 }], items: vec![FavoriteItemRecord { id: 3, group_id: 1, name: "Gone".into(), target: path("gone"), position: 0 }] };
    // A session without a worker settled as failed; this load stands in
    // for the worker's answer so the probe path can be followed.
    session.workspace = Workspace::with_home(Location::root());
    session.submit(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites, ..SettingsSnapshot::default() } });
    assert!(session.runner.dispatched.contains(&WorkRequest::ProbeFavoriteTarget { item_id: 3, target: path("gone") }));
    session.runner.delivered.push_back(Event::FavoriteTargetProbed { item_id: 3, target: path("gone"), outcome: FavoriteProbeOutcome::Unavailable });
    session.drain();
    assert!(session.workspace_view().groups()[0].items.is_empty());
    assert!(session.workspace_view().notices().iter().any(|notice| notice.text == "Removed Favorite “Gone” because “/gone” is unavailable."));
}

mod stored_settings {
    use super::*;
    use crate::settings_storage::{SettingsDatabase, SettingsWorker};
    use dual_pane_application::{FavoriteGroupRecord, FavoritesRecords, SettingsSnapshot, SettingsStatus, default_bindings};

    fn stored_database() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.sqlite3");
        let favorites = FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 7, name: "Work".into(), position: 0 }], items: vec![] };
        SettingsDatabase::open(&path).unwrap().save(&SettingsSnapshot { bindings: default_bindings(), folder_sorts: vec![], favorites, ..SettingsSnapshot::default() }).unwrap();
        (directory, path)
    }
    fn session(path: &std::path::Path) -> WorkspaceSession<FakeRunner> {
        WorkspaceSession::with_settings(FakeRunner::default(), Location::root(), Some(SettingsWorker::start(path.to_path_buf()).unwrap()), DRAIN_SLICE, DRAIN_TIME_BUDGET)
    }
    fn drain_until_settled(session: &mut WorkspaceSession<FakeRunner>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.workspace.settings_status() == SettingsStatus::Loading {
            assert!(Instant::now() < deadline, "settings never settled");
            session.drain();
            std::thread::yield_now();
        }
    }
    fn stored_groups(path: &std::path::Path) -> Vec<String> {
        SettingsDatabase::open(path).unwrap().load().unwrap().favorites.groups.into_iter().map(|group| group.name).collect()
    }
    /// Shows the root in the Left Browser, then sorts it by Size.
    fn sort_click(session: &mut WorkspaceSession<FakeRunner>) {
        session.submit(Command::Navigate { browser: BrowserSide::Left, location: Location::root() });
        let Some(WorkRequest::ReadDirectory { tab, token, .. }) = session.runner.dispatched.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })).cloned() else { panic!("a read") };
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from([]), changes: None });
        let tab = session.workspace.active_tab(BrowserSide::Left);
        session.submit(Command::SetSort { browser: BrowserSide::Left, tab, location: Location::root(), sort: SortSpec::new(dual_pane_domain::SortField::Size, dual_pane_domain::SortDirection::Ascending) });
    }

    #[test]
    fn a_failed_load_never_overwrites_the_stored_database() {
        let (_directory, path) = stored_database();
        rusqlite::Connection::open(&path).unwrap().execute("UPDATE action_binding SET command = 'damaged' WHERE action = 'NewFolder'", []).unwrap();
        let mut session = session(&path);
        drain_until_settled(&mut session);
        assert!(matches!(session.workspace.settings_status(), SettingsStatus::LoadFailed(SettingsFailure::Corrupt)));
        sort_click(&mut session);
        session.shutdown(Duration::from_secs(10));
        let damaged: String = rusqlite::Connection::open(&path).unwrap().query_row("SELECT command FROM action_binding WHERE action = 'NewFolder'", [], |row| row.get(0)).unwrap();
        assert_eq!(damaged, "damaged");
    }

    #[test]
    fn a_choice_made_before_the_load_result_is_saved_over_the_stored_settings() {
        let (_directory, path) = stored_database();
        let mut session = session(&path);
        sort_click(&mut session);
        drain_until_settled(&mut session);
        session.shutdown(Duration::from_secs(10));
        assert_eq!(stored_groups(&path), ["Work"]);
        let sorts = SettingsDatabase::open(&path).unwrap().load().unwrap().folder_sorts;
        assert_eq!(sorts, vec![(Location::root(), SortSpec::new(dual_pane_domain::SortField::Size, dual_pane_domain::SortDirection::Ascending))]);
    }

    #[test]
    fn a_fresh_profile_probes_screenshots_on_the_runner_before_seeding_favorites() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.sqlite3");
        let home = path_in(&["synthetic-home"]);
        let mut session = WorkspaceSession::with_settings(FakeRunner::default(), home.clone(), Some(SettingsWorker::start(path.clone()).unwrap()), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        drain_until_settled(&mut session);
        let screenshots = path_in(&["synthetic-home", "Documents", "Screenshots"]);
        assert!(session.runner.dispatched.contains(&WorkRequest::ProbeScreenshotsFolder { location: screenshots.clone() }));
        assert!(session.workspace.favorites().items.is_empty());
        session.submit(Event::ScreenshotsFolderProbed { location: screenshots, outcome: dual_pane_application::FavoriteProbeOutcome::Available });
        session.shutdown(Duration::from_secs(10));
        let stored = SettingsDatabase::open(&path).unwrap().load().unwrap().favorites.items.into_iter().map(|item| item.name).collect::<Vec<_>>();
        assert_eq!(stored, ["Applications", "Desktop", "Documents", "Screenshots", "Downloads"]);
    }
    fn path_in(names: &[&str]) -> Location {
        Location::from_components(names.iter().map(|name| EntryName::new(*name).unwrap()))
    }

    #[test]
    fn a_session_without_storage_settles_as_failed() {
        let session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        assert_eq!(session.workspace.settings_status(), SettingsStatus::LoadFailed(SettingsFailure::WorkerUnavailable));
        let notices = session.workspace_view().notices();
        assert_eq!(notices.len(), 1);
        assert!(!notices[0].offers_reset, "with no settings worker there is nothing to reset");
        assert_eq!(session.workspace_view().open_requests(), 1, "the startup storage failure opens Notices while the default preference permits it");
    }

    /// A stored database damaged so that loading it fails as corrupt.
    fn corrupt_database() -> (tempfile::TempDir, PathBuf) {
        let (directory, path) = stored_database();
        rusqlite::Connection::open(&path).unwrap().execute("UPDATE action_binding SET command = 'damaged' WHERE action = 'NewFolder'", []).unwrap();
        (directory, path)
    }
    fn drain_until(session: &mut WorkspaceSession<FakeRunner>, done: impl Fn(&WorkspaceSession<FakeRunner>) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(session) {
            assert!(Instant::now() < deadline, "the session never reached the expected state");
            session.drain();
            std::thread::yield_now();
        }
    }
    fn notice_texts(session: &WorkspaceSession<FakeRunner>) -> Vec<String> {
        session.workspace_view().notices().iter().map(|notice| notice.text.clone()).collect()
    }

    #[test]
    fn a_corrupt_or_newer_database_opens_notices_with_reset_offered() {
        let (_directory, path) = corrupt_database();
        let mut session = session(&path);
        drain_until_settled(&mut session);
        let view = session.workspace_view();
        assert_eq!(view.open_requests(), 1, "an actionable storage error opens Notices at launch");
        assert!(view.notices()[0].offers_reset);
        assert!(view.notices()[0].text.contains("damaged"));

        let (_directory, path) = stored_database();
        rusqlite::Connection::open(&path).unwrap().pragma_update(None, "user_version", 99).unwrap();
        let mut session = self::session(&path);
        drain_until_settled(&mut session);
        assert_eq!(session.workspace.settings_status(), SettingsStatus::LoadFailed(SettingsFailure::UnsupportedSchema));
        assert!(session.workspace_view().notices()[0].offers_reset);
    }

    #[test]
    fn a_confirmed_reset_preserves_the_failed_database_and_reloads_fresh_settings() {
        let (directory, path) = corrupt_database();
        let mut session = session(&path);
        drain_until_settled(&mut session);
        session.submit(Command::ResetSettings);
        assert!(!session.workspace_view().favorites_ready());
        drain_until(&mut session, |session| session.workspace.settings_status() == SettingsStatus::Loaded);
        let backups = std::fs::read_dir(directory.path()).unwrap().filter_map(Result::ok).map(|entry| entry.file_name().to_string_lossy().into_owned()).filter(|name| name.starts_with("settings.failed-") && !name.ends_with("-wal") && !name.ends_with("-shm")).collect::<Vec<_>>();
        assert_eq!(backups.len(), 1, "the failed database is preserved");
        let damaged: String = rusqlite::Connection::open(directory.path().join(&backups[0])).unwrap().query_row("SELECT command FROM action_binding WHERE action = 'NewFolder'", [], |row| row.get(0)).unwrap();
        assert_eq!(damaged, "damaged");
        let success = notice_texts(&session).into_iter().find(|text| text.starts_with("Settings were reset.")).expect("a success Notice");
        assert!(success.contains(&backups[0]), "the Notice names where the database was preserved: {success}");
        assert!(session.runner.dispatched.iter().any(|work| matches!(work, WorkRequest::ProbeScreenshotsFolder { .. })), "fresh Favorites are seeded again");
        assert!(session.workspace.favorites().groups.is_empty(), "the stored Work group was replaced");
    }

    #[test]
    fn a_reset_discards_a_save_queued_before_it() {
        let (_directory, path) = stored_database();
        let mut session = session(&path);
        drain_until_settled(&mut session);
        assert_eq!(session.submit_favorites(FavoritesEvent::CreateGroup { name: "Unsaved".into() }), None);
        session.submit(Command::ResetSettings);
        drain_until(&mut session, |session| session.workspace.settings_status() == SettingsStatus::Loaded);
        session.shutdown(Duration::from_secs(10));
        assert!(!stored_groups(&path).contains(&"Unsaved".to_owned()), "an older queued save cannot reach the fresh database");
    }

    #[test]
    fn a_failed_reset_keeps_the_error_and_leaves_the_database_untouched() {
        use std::os::unix::fs::PermissionsExt;
        let (directory, path) = corrupt_database();
        let mut session = session(&path);
        drain_until_settled(&mut session);
        // A read-only folder makes preserving the database fail.
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        session.submit(Command::ResetSettings);
        drain_until(&mut session, |session| session.workspace.settings_status() != SettingsStatus::Loading);
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(session.workspace.settings_status(), SettingsStatus::LoadFailed(SettingsFailure::Corrupt));
        let texts = notice_texts(&session);
        assert!(texts.iter().any(|text| text == "Settings couldn’t be reset. The stored settings were not changed."), "{texts:?}");
        assert!(!texts.iter().any(|text| text.starts_with("Settings were reset")));
        assert!(session.workspace_view().notices().iter().all(|notice| notice.offers_reset), "reset stays available");
        let damaged: String = rusqlite::Connection::open(&path).unwrap().query_row("SELECT command FROM action_binding WHERE action = 'NewFolder'", [], |row| row.get(0)).unwrap();
        assert_eq!(damaged, "damaged");
    }

    #[test]
    fn a_save_failure_keeps_the_edit_and_adds_one_notice() {
        let (directory, path) = stored_database();
        let mut session = session(&path);
        drain_until_settled(&mut session);
        // Removing the database and making its folder read-only makes
        // every later save fail.
        std::fs::remove_file(&path).unwrap();
        for sidecar in ["settings.sqlite3-wal", "settings.sqlite3-shm"] {
            std::fs::remove_file(directory.path().join(sidecar)).ok();
        }
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        session.submit_favorites(FavoritesEvent::CreateGroup { name: "Kept".into() });
        session.submit_favorites(FavoritesEvent::CreateGroup { name: "Also kept".into() });
        drain_until(&mut session, |session| !session.workspace_view().notices().is_empty());
        std::thread::sleep(Duration::from_millis(200));
        session.drain();
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(session.workspace.favorites().groups.iter().any(|group| group.name == "Kept"));
        let notices = notice_texts(&session);
        assert_eq!(notices, ["Dual Pane couldn’t save settings. Your changes stay in effect until you quit."]);
    }
}
