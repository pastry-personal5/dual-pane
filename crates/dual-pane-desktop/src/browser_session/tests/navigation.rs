use super::*;

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
