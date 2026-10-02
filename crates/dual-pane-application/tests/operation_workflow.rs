use std::sync::Arc;

use dual_pane_application::{CleanupResult, Command, Event, OperationEffect, OperationErrorKind, OperationFailure, OperationOutcome, OperationProgress, OperationStatus, Output, PlannedItem, StepResult, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingErrorKind, Location, OperationChoice, OperationId, OperationIssue, OperationKind, OperationRejection, RequestToken, TabId};

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}
fn path(text: &str) -> Location {
    Location::root().join(&name(text))
}
fn item(text: &str, kind: EntryKind) -> Entry {
    Entry::new(name(text), kind)
}
fn read(work: &[WorkRequest]) -> (TabId, RequestToken) {
    work.iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { tab, token, .. } => Some((*tab, *token)),
            _ => None,
        })
        .unwrap()
}
fn show(ws: &mut Workspace, browser: BrowserSide, at: Location, entries: Vec<Entry>) -> TabId {
    let transition = ws.handle(Command::Navigate { browser, location: at }.into());
    let (tab, token) = read(&transition.work);
    ws.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(entries), changes: None }.into());
    tab
}
fn start(ws: &mut Workspace, browser: BrowserSide, tab: TabId, kind: OperationKind) -> (OperationId, Vec<WorkRequest>) {
    let transition = ws.handle(Command::StartOperation { browser, tab, kind }.into());
    let Output::OperationChanged { job } = &transition.outputs[0] else { panic!("operation started") };
    (job.id(), transition.work)
}
fn scan(work: &[WorkRequest]) -> (OperationId, u64) {
    let WorkRequest::Operation(OperationEffect::Scan { id, generation, .. }) = &work[0] else { panic!("scan") };
    (*id, *generation)
}
fn execute(work: &[WorkRequest]) -> (OperationId, u64, usize, Option<OperationChoice>) {
    let WorkRequest::Operation(OperationEffect::Execute { id, generation, at, choice, .. }) = &work[0] else { panic!("execute") };
    (*id, *generation, *at, *choice)
}
fn scanned(ws: &mut Workspace, id: OperationId, generation: u64, plan: Vec<PlannedItem>) -> Vec<WorkRequest> {
    ws.handle(Event::OperationScanned { id, generation, plan: Ok(Arc::from(plan)) }.into()).work
}
fn progress(completed: usize, skipped: usize, failed: usize) -> OperationProgress {
    OperationProgress { completed, skipped, failed }
}
fn setup_copy(entries: Vec<Entry>) -> (Workspace, TabId) {
    let mut ws = Workspace::new();
    let tab = show(&mut ws, BrowserSide::Left, path("from"), entries);
    show(&mut ws, BrowserSide::Right, path("to"), vec![]);
    (ws, tab)
}
fn select(ws: &mut Workspace, tab: TabId, row: usize, text: &str) {
    ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: name(text) }.into());
}

#[test]
fn commands_freeze_listing_order_and_destination_across_workspace_changes() {
    let (mut ws, tab) = setup_copy(vec![item("b", EntryKind::File), item("a", EntryKind::File)]);
    select(&mut ws, tab, 1, "a");
    ws.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab, row: 0, name: name("b") }.into());
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    let WorkRequest::Operation(OperationEffect::Scan { intent, .. }) = &work[0] else { panic!() };
    assert_eq!(intent.targets().iter().map(|target| &target.name).collect::<Vec<_>>(), vec![&name("b"), &name("a")]);
    assert_eq!(intent.destination(), Some(&path("to")));
    ws.handle(Command::ClearSelection { browser: BrowserSide::Left, tab }.into());
    show(&mut ws, BrowserSide::Right, path("elsewhere"), vec![]);
    show(&mut ws, BrowserSide::Left, path("changed"), vec![]);
    assert_eq!(ws.operation_jobs()[0].intent(), &intent.clone());
    let (_, generation) = scan(&work);
    let plan = vec![PlannedItem { source: path("from").join(&name("b")), destination: Some(path("to").join(&name("b"))), kind: EntryKind::File }, PlannedItem { source: path("from").join(&name("a")), destination: Some(path("to").join(&name("a"))), kind: EntryKind::File }];
    assert_eq!(execute(&scanned(&mut ws, id, generation, plan)).2, 0);
}

#[test]
fn availability_rejects_pending_failed_stale_and_subtree_targets() {
    let (mut ws, tab) = setup_copy(vec![item("dir", EntryKind::Directory), item("link", EntryKind::Symlink { points_to_directory: true })]);
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Copy), Err(OperationRejection::NoTargets));
    select(&mut ws, tab, 0, "dir");
    let (pending_tab, token) = read(&ws.handle(Command::Navigate { browser: BrowserSide::Right, location: path("new") }.into()).work);
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Copy), Err(OperationRejection::DestinationUnavailable));
    ws.handle(Event::FolderItemsFailed { browser: BrowserSide::Right, tab: pending_tab, token, kind: ListingErrorKind::PermissionDenied }.into());
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Copy), Err(OperationRejection::DestinationUnavailable));
    show(&mut ws, BrowserSide::Right, path("from").join(&name("dir")).join(&name("inside")), vec![]);
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Copy), Err(OperationRejection::DestinationWithinSource));
    select(&mut ws, tab, 1, "link");
    assert!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Copy).is_ok());
    let (source_tab, _) = read(&ws.handle(Command::Navigate { browser: BrowserSide::Left, location: path("pending") }.into()).work);
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash), Err(OperationRejection::SourceUnavailable));
    assert_eq!(source_tab, tab);
    let newer = ws.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    assert!(!newer.work.is_empty());
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash), Err(OperationRejection::StaleTab));
}

#[test]
fn delete_confirmation_and_cancellation_are_bound_to_frozen_count() {
    let (mut ws, tab) = setup_copy(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, 0, "a");
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::DeletePermanently);
    assert!(work.is_empty());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::AwaitingConfirmation { targets: 1 });
    assert!(ws.handle(Command::ConfirmPermanentDelete { id, targets: 2 }.into()).work.is_empty());
    assert!(ws.handle(Command::CancelOperation { id }.into()).work.is_empty());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Finished(OperationOutcome::Cancelled));
    assert!(ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work.is_empty());
    let (id, _) = start(&mut ws, BrowserSide::Left, tab, OperationKind::DeletePermanently);
    let confirmed = ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into());
    assert_eq!(scan(&confirmed.work).0, id);
}

#[test]
fn conflicts_tokens_progress_and_cleanup_are_validated() {
    let (mut ws, tab) = setup_copy(vec![item("folder", EntryKind::Directory)]);
    select(&mut ws, tab, 0, "folder");
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    let (_, scan_generation) = scan(&work);
    let root = path("from").join(&name("folder"));
    let child = root.join(&name("child"));
    let plan = vec![PlannedItem { source: root.clone(), destination: Some(path("to").join(&name("folder"))), kind: EntryKind::Directory }, PlannedItem { source: child.clone(), destination: Some(path("to").join(&name("folder")).join(&name("child"))), kind: EntryKind::File }];
    assert!(scanned(&mut ws, id, scan_generation, vec![plan[1].clone()]).is_empty());
    let execution = scanned(&mut ws, id, scan_generation, plan);
    let (_, mut generation, _, _) = execute(&execution);
    assert!(ws.handle(Event::OperationStepped { id, generation, result: StepResult::DecisionRequired { item: child.clone(), issue: OperationIssue::FileConflict, progress: progress(0, 0, 0) } }.into()).outputs.is_empty());
    let next = ws.handle(Event::OperationStepped { id, generation, result: StepResult::Advanced { next: 1, progress: progress(1, 0, 0) } }.into());
    generation = execute(&next.work).1;
    let waiting = ws.handle(Event::OperationStepped { id, generation, result: StepResult::DecisionRequired { item: child.clone(), issue: OperationIssue::FileConflict, progress: progress(1, 0, 0) } }.into());
    let Output::OperationChanged { job } = &waiting.outputs[0] else { panic!() };
    let OperationStatus::Waiting { token, .. } = job.status() else { panic!() };
    let token = *token;
    assert!(ws.handle(Command::DecideOperation { id, token, item: root.clone(), choice: OperationChoice::Replace, apply_to_all: true }.into()).outputs.is_empty());
    assert!(ws.handle(Command::DecideOperation { id, token, item: child.clone(), choice: OperationChoice::TryAgain, apply_to_all: false }.into()).outputs.is_empty());
    let retry = ws.handle(Command::DecideOperation { id, token, item: child.clone(), choice: OperationChoice::Replace, apply_to_all: true }.into());
    generation = execute(&retry.work).1;
    assert!(ws.handle(Command::DecideOperation { id, token, item: child.clone(), choice: OperationChoice::Skip, apply_to_all: false }.into()).outputs.is_empty());
    let repeat = ws.handle(Event::OperationStepped { id, generation, result: StepResult::DecisionRequired { item: child.clone(), issue: OperationIssue::FileConflict, progress: progress(1, 0, 0) } }.into());
    let Output::OperationChanged { job } = &repeat.outputs[0] else { panic!() };
    let OperationStatus::Waiting { token: new_token, .. } = job.status() else { panic!() };
    assert_ne!(token, *new_token);
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    let WorkRequest::Operation(OperationEffect::Cancel { generation: cleanup_generation, .. }) = cancel.work[0] else { panic!() };
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Cancelling);
    assert!(ws.handle(Event::OperationStepped { id, generation, result: StepResult::Finished { progress: progress(2, 0, 0) } }.into()).outputs.is_empty());
    assert!(ws.handle(Event::OperationCleaned { id, generation, result: CleanupResult::Clean }.into()).outputs.is_empty());
    ws.handle(Event::OperationCleaned { id, generation: cleanup_generation, result: CleanupResult::Clean }.into());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Finished(OperationOutcome::Partial));
}

#[test]
fn malformed_plan_and_executor_unavailability_fail_closed() {
    let (mut ws, tab) = setup_copy(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, 0, "a");
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    let (_, generation) = scan(&work);
    assert!(scanned(&mut ws, id, generation, vec![PlannedItem { source: path("outside"), destination: Some(path("to").join(&name("a"))), kind: EntryKind::File }]).is_empty());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Scanning);
    ws.handle(Event::OperationExecutorUnavailable { id, generation }.into());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Finished(OperationOutcome::Failed));
}

#[test]
fn concurrent_jobs_and_issue_specific_choices_stay_independent() {
    let (mut ws, tab) = setup_copy(vec![item("a", EntryKind::File), item("b", EntryKind::Symlink { points_to_directory: false })]);
    select(&mut ws, tab, 0, "a");
    let (first, first_work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    select(&mut ws, tab, 1, "b");
    let (second, second_work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    let (_, g1) = scan(&first_work);
    let (_, g2) = scan(&second_work);
    let a = path("from").join(&name("a"));
    let b = path("from").join(&name("b"));
    let work1 = scanned(&mut ws, first, g1, vec![PlannedItem { source: a.clone(), destination: Some(path("to").join(&name("a"))), kind: EntryKind::File }]);
    let work2 = scanned(&mut ws, second, g2, vec![PlannedItem { source: b.clone(), destination: Some(path("to").join(&name("b"))), kind: EntryKind::Symlink { points_to_directory: false } }]);
    let (_, e1, _, _) = execute(&work1);
    let (_, e2, _, _) = execute(&work2);
    ws.handle(Event::OperationStepped { id: first, generation: e1, result: StepResult::DecisionRequired { item: a.clone(), issue: OperationIssue::FileConflict, progress: progress(0, 0, 0) } }.into());
    ws.handle(Event::OperationStepped { id: second, generation: e2, result: StepResult::DecisionRequired { item: b.clone(), issue: OperationIssue::LinkCollision, progress: progress(0, 0, 0) } }.into());
    let OperationStatus::Waiting { token: t1, .. } = ws.operation_jobs()[0].status() else { panic!() };
    let OperationStatus::Waiting { token: t2, .. } = ws.operation_jobs()[1].status() else { panic!() };
    let (t1, t2) = (*t1, *t2);
    assert_ne!(t1, t2);
    assert!(ws.handle(Command::DecideOperation { id: second, token: t1, item: b.clone(), choice: OperationChoice::Skip, apply_to_all: false }.into()).outputs.is_empty());
    assert!(ws.handle(Command::DecideOperation { id: second, token: t2, item: b.clone(), choice: OperationChoice::Replace, apply_to_all: false }.into()).outputs.is_empty());
    assert!(ws.handle(Command::DecideOperation { id: second, token: t2, item: b.clone(), choice: OperationChoice::Skip, apply_to_all: true }.into()).outputs.is_empty());
    let retry = ws.handle(Command::DecideOperation { id: second, token: t2, item: b.clone(), choice: OperationChoice::TryAgain, apply_to_all: false }.into());
    assert_eq!(execute(&retry.work).3, Some(OperationChoice::TryAgain));
    assert!(matches!(ws.operation_jobs()[0].status(), OperationStatus::Waiting { .. }));
    let (_, retry_generation, _, _) = execute(&retry.work);
    ws.handle(Event::OperationStepped { id: second, generation: retry_generation, result: StepResult::DecisionRequired { item: b.clone(), issue: OperationIssue::RecoverableError, progress: progress(0, 0, 0) } }.into());
    let OperationStatus::Waiting { token: error_token, .. } = ws.operation_jobs()[1].status() else { panic!() };
    let error_token = *error_token;
    assert!(ws.handle(Command::DecideOperation { id: second, token: error_token, item: b.clone(), choice: OperationChoice::Replace, apply_to_all: false }.into()).outputs.is_empty());
    let skip = ws.handle(Command::DecideOperation { id: second, token: error_token, item: b, choice: OperationChoice::Skip, apply_to_all: false }.into());
    assert_eq!(execute(&skip.work).3, Some(OperationChoice::Skip));
    let replace = ws.handle(Command::DecideOperation { id: first, token: t1, item: a, choice: OperationChoice::Replace, apply_to_all: false }.into());
    assert_eq!(execute(&replace.work).3, Some(OperationChoice::Replace));
}

#[test]
fn directory_skip_accounts_for_descendants_and_name_collision_has_no_overwrite() {
    let (mut ws, tab) = setup_copy(vec![item("dir", EntryKind::Directory)]);
    select(&mut ws, tab, 0, "dir");
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    let (_, generation) = scan(&work);
    let root = path("from").join(&name("dir"));
    let child = root.join(&name("child"));
    let execution = scanned(&mut ws, id, generation, vec![PlannedItem { source: root.clone(), destination: Some(path("to").join(&name("dir"))), kind: EntryKind::Directory }, PlannedItem { source: child, destination: Some(path("to").join(&name("dir")).join(&name("child"))), kind: EntryKind::File }]);
    let (_, execution_generation, _, _) = execute(&execution);
    assert!(ws.handle(Event::OperationStepped { id, generation: execution_generation, result: StepResult::Finished { progress: progress(0, 1, 0) } }.into()).outputs.is_empty());
    ws.handle(Event::OperationStepped { id, generation: execution_generation, result: StepResult::Finished { progress: progress(0, 2, 0) } }.into());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Finished(OperationOutcome::Partial));
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Rename { to: name("new") });
    let (_, generation) = scan(&work);
    let execution = scanned(&mut ws, id, generation, vec![PlannedItem { source: root.clone(), destination: Some(path("from").join(&name("new"))), kind: EntryKind::Directory }]);
    let (_, generation, _, _) = execute(&execution);
    ws.handle(Event::OperationStepped { id, generation, result: StepResult::NameCollision { item: root.clone(), progress: progress(0, 0, 0) } }.into());
    assert_eq!(ws.operation_jobs()[1].status(), &OperationStatus::NameCollision { item: root });
}

#[test]
fn apply_to_all_runs_once_per_later_file_conflict() {
    let (mut ws, tab) = setup_copy(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, 0, "a");
    ws.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab, row: 1, name: name("b") }.into());
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::Copy);
    let (_, generation) = scan(&work);
    let a = path("from").join(&name("a"));
    let b = path("from").join(&name("b"));
    let execution = scanned(&mut ws, id, generation, vec![PlannedItem { source: a.clone(), destination: Some(path("to").join(&name("a"))), kind: EntryKind::File }, PlannedItem { source: b.clone(), destination: Some(path("to").join(&name("b"))), kind: EntryKind::File }]);
    let (_, mut generation, _, _) = execute(&execution);
    ws.handle(Event::OperationStepped { id, generation, result: StepResult::DecisionRequired { item: a.clone(), issue: OperationIssue::FileConflict, progress: progress(0, 0, 0) } }.into());
    let OperationStatus::Waiting { token, .. } = ws.operation_jobs()[0].status() else { panic!() };
    let token = *token;
    generation = execute(&ws.handle(Command::DecideOperation { id, token, item: a, choice: OperationChoice::Skip, apply_to_all: true }.into()).work).1;
    generation = execute(&ws.handle(Event::OperationStepped { id, generation, result: StepResult::Advanced { next: 1, progress: progress(0, 1, 0) } }.into()).work).1;
    let auto = ws.handle(Event::OperationStepped { id, generation, result: StepResult::DecisionRequired { item: b.clone(), issue: OperationIssue::FileConflict, progress: progress(0, 1, 0) } }.into());
    let (_, generation, _, choice) = execute(&auto.work);
    assert_eq!(choice, Some(OperationChoice::Skip));
    let repeat = ws.handle(Event::OperationStepped { id, generation, result: StepResult::DecisionRequired { item: b, issue: OperationIssue::FileConflict, progress: progress(0, 1, 0) } }.into());
    assert!(repeat.work.is_empty());
    assert!(matches!(ws.operation_jobs()[0].status(), OperationStatus::Waiting { .. }));
}

#[test]
fn new_folder_needs_no_selection_and_uncertain_cleanup_is_terminal() {
    let (mut ws, tab) = setup_copy(vec![]);
    assert!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::NewFolder { name: name("new") }).is_ok());
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::NewFolder { name: name(" ") }), Err(OperationRejection::InvalidName));
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::NewFolder { name: name("new") });
    let (_, generation) = scan(&work);
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    let WorkRequest::Operation(OperationEffect::Cancel { generation: cleanup_generation, .. }) = cancel.work[0] else { panic!() };
    assert!(ws.handle(Event::OperationScanned { id, generation, plan: Err(OperationFailure { item: path("from").join(&name("new")), kind: OperationErrorKind::PermissionDenied }) }.into()).outputs.is_empty());
    ws.handle(Event::OperationCleaned { id, generation: cleanup_generation, result: CleanupResult::Uncertain }.into());
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Finished(OperationOutcome::CleanupUncertain));
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::NewFolder { name: name("another") });
    let (_, generation) = scan(&work);
    ws.handle(Event::OperationExecutorUnavailable { id, generation }.into());
    assert_eq!(ws.operation_jobs()[1].status(), &OperationStatus::Finished(OperationOutcome::Failed));
}

#[test]
fn source_errors_same_destination_and_rename_count_reject_before_work() {
    let (mut ws, tab) = setup_copy(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, 0, "a");
    show(&mut ws, BrowserSide::Right, path("from"), vec![]);
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Move), Err(OperationRejection::SameSourceAndDestination));
    ws.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab, row: 1, name: name("b") }.into());
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Rename { to: name("c") }), Err(OperationRejection::InvalidTargetCount));
    select(&mut ws, tab, 0, "a");
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Rename { to: name("a") }), Err(OperationRejection::UnchangedName));
    let (source_tab, token) = read(&ws.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into()).work);
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash), Err(OperationRejection::SourceUnavailable));
    ws.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: source_tab, token, kind: ListingErrorKind::PermissionDenied }.into());
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash), Err(OperationRejection::SourceUnavailable));
}

#[test]
fn failure_preserves_item_and_kind_for_terminal_reporting() {
    let (mut ws, tab) = setup_copy(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, 0, "a");
    let (id, work) = start(&mut ws, BrowserSide::Left, tab, OperationKind::MoveToTrash);
    let (_, generation) = scan(&work);
    let item = path("from").join(&name("a"));
    let execution = scanned(&mut ws, id, generation, vec![PlannedItem { source: item.clone(), destination: None, kind: EntryKind::File }]);
    let (_, generation, _, _) = execute(&execution);
    let failure = OperationFailure { item, kind: OperationErrorKind::NoSpace };
    ws.handle(Event::OperationStepped { id, generation, result: StepResult::Failed { failure: failure.clone(), progress: progress(0, 0, 1) } }.into());
    assert_eq!(ws.operation_jobs()[0].failure(), Some(&failure));
    assert_eq!(ws.operation_jobs()[0].status(), &OperationStatus::Finished(OperationOutcome::Failed));
}
