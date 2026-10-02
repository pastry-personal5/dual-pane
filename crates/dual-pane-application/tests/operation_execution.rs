//! The operation contract that native execution needs: recoverable error
//! kinds, folder finalization, name problems, availability, refresh,
//! selection after an edit, dismissal, byte progress, opening, Notices, the
//! journal status, and quitting.

use std::sync::Arc;

use dual_pane_application::{ByteProgress, CleanupResult, Command, Event, FinalizeResult, NameProblem, Notice, NoticeKind, OperationEffect, OperationErrorKind, OperationFailure, OperationOutcome, OperationProgress, OperationStatus, Output, PlannedItem, ResolvedTarget, ScanFailure, StepResult, Transition, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, OperationChoice, OperationCommand, OperationId, OperationIssue, OperationKind, OperationRejection, RequestToken, TabId};

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}
fn path(parts: &[&str]) -> Location {
    parts.iter().fold(Location::root(), |location, part| location.join(&name(part)))
}
fn item(text: &str, kind: EntryKind) -> Entry {
    Entry::new(name(text), kind)
}
fn progress(completed: usize, skipped: usize, failed: usize) -> OperationProgress {
    OperationProgress { completed, skipped, failed }
}
fn reads(work: &[WorkRequest]) -> Vec<(BrowserSide, TabId, RequestToken, Location)> {
    work.iter()
        .filter_map(|work| match work {
            WorkRequest::ReadDirectory { browser, tab, token, location, .. } => Some((*browser, *tab, *token, location.clone())),
            _ => None,
        })
        .collect()
}
fn deliver(ws: &mut Workspace, read: &(BrowserSide, TabId, RequestToken, Location), entries: Vec<Entry>) -> Transition {
    ws.handle(Event::FolderItemsLoaded { browser: read.0, tab: read.1, token: read.2, entries: Arc::from(entries), changes: None }.into())
}
fn show(ws: &mut Workspace, browser: BrowserSide, at: Location, entries: Vec<Entry>) -> TabId {
    let transition = ws.handle(Command::Navigate { browser, location: at }.into());
    let read = reads(&transition.work).remove(0);
    deliver(ws, &read, entries);
    read.1
}
fn setup(entries: Vec<Entry>) -> (Workspace, TabId) {
    let mut ws = Workspace::new();
    let tab = show(&mut ws, BrowserSide::Left, path(&["from"]), entries);
    show(&mut ws, BrowserSide::Right, path(&["to"]), vec![]);
    (ws, tab)
}
fn select(ws: &mut Workspace, tab: TabId, names: &[&str]) {
    let entries = ws.entries(BrowserSide::Left).to_vec();
    for (index, text) in names.iter().enumerate() {
        let row = entries.iter().position(|entry| entry.name() == &name(text)).unwrap();
        let command = if index == 0 { Command::SelectEntry { browser: BrowserSide::Left, tab, row, name: name(text) } } else { Command::ToggleEntry { browser: BrowserSide::Left, tab, row, name: name(text) } };
        ws.handle(command.into());
    }
}
fn effect(work: &[WorkRequest]) -> &OperationEffect {
    work.iter()
        .find_map(|work| match work {
            WorkRequest::Operation(effect) => Some(effect),
            _ => None,
        })
        .expect("an operation effect")
}
fn generation(work: &[WorkRequest]) -> u64 {
    match effect(work) {
        OperationEffect::Scan { generation, .. } | OperationEffect::Execute { generation, .. } | OperationEffect::Cancel { generation, .. } | OperationEffect::Finalize { generation, .. } => *generation,
        OperationEffect::Release { .. } => panic!("a release has no generation"),
    }
}
fn start(ws: &mut Workspace, tab: TabId, kind: OperationKind) -> (OperationId, u64) {
    let transition = ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind }.into());
    let Output::OperationChanged { job } = &transition.outputs[0] else { panic!("started: {:?}", transition.outputs) };
    (job.id(), job.generation())
}
/// A plan entry under `/from`, mapped under `/to` for Copy and Move.
fn planned(parts: &[&str], kind: EntryKind, mapped: bool) -> PlannedItem {
    let mut source = vec!["from"];
    source.extend_from_slice(parts);
    let mut destination = vec!["to"];
    destination.extend_from_slice(parts);
    PlannedItem { source: path(&source), destination: mapped.then(|| path(&destination)), kind }
}
fn scanned(ws: &mut Workspace, id: OperationId, generation: u64, plan: Vec<PlannedItem>) -> Transition {
    ws.handle(Event::OperationScanned { id, generation, plan: Ok(Arc::from(plan)) }.into())
}
fn stepped(ws: &mut Workspace, id: OperationId, generation: u64, result: StepResult) -> Transition {
    ws.handle(Event::OperationStepped { id, generation, result }.into())
}
fn finalized(ws: &mut Workspace, id: OperationId, generation: u64, result: FinalizeResult) -> Transition {
    ws.handle(Event::OperationFinalized { id, generation, result }.into())
}
fn status(ws: &Workspace, id: OperationId) -> OperationStatus {
    ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().status().clone()
}
fn decide(ws: &mut Workspace, id: OperationId, choice: OperationChoice) -> Transition {
    let job = ws.operation_jobs().iter().find(|job| job.id() == id).unwrap();
    let decision = job.decision().expect("waiting for a decision");
    let (token, item) = (decision.token, decision.item.clone());
    ws.handle(Command::DecideOperation { id, token, item, choice, apply_to_all: false }.into())
}
fn finalize_list(work: &[WorkRequest]) -> (Vec<Location>, usize) {
    let OperationEffect::Finalize { directories, at, .. } = effect(work) else { panic!("finalize: {work:?}") };
    (directories.to_vec(), *at)
}
fn notices(transition: &Transition) -> Vec<(NoticeKind, bool)> {
    transition
        .outputs
        .iter()
        .filter_map(|output| match output {
            Output::NoticeAdded { notice: Notice { kind, .. }, open } => Some((kind.clone(), *open)),
            _ => None,
        })
        .collect()
}

/// A Move of `/from/dir` holding `a/x`, `b`, and the file `f`, run to the
/// end of its main pass with `skip` decided on the plan index it names.
fn move_tree(skip: Option<usize>) -> (Workspace, OperationId, Transition) {
    let (mut ws, tab) = setup(vec![item("dir", EntryKind::Directory)]);
    select(&mut ws, tab, &["dir"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Move);
    let plan = vec![planned(&["dir"], EntryKind::Directory, true), planned(&["dir", "a"], EntryKind::Directory, true), planned(&["dir", "a", "x"], EntryKind::File, true), planned(&["dir", "b"], EntryKind::Directory, true), planned(&["dir", "f"], EntryKind::File, true)];
    let mut work = scanned(&mut ws, id, generation, plan).work;
    let mut done = progress(0, 0, 0);
    if let Some(index) = skip {
        if index > 0 {
            done = progress(index, 0, 0);
            work = stepped(&mut ws, id, generation_of(&work), StepResult::Advanced { next: index, progress: done }).work;
        }
        stepped(&mut ws, id, generation_of(&work), StepResult::DecisionRequired { item: path(&["from", "dir", "a", "x"]), issue: OperationIssue::FileConflict, destination_kind: EntryKind::File, progress: done });
        work = decide(&mut ws, id, OperationChoice::Skip).work;
        let finished = stepped(&mut ws, id, generation_of(&work), StepResult::Finished { progress: progress(4, 1, 0) });
        return (ws, id, finished);
    }
    let finished = stepped(&mut ws, id, generation_of(&work), StepResult::Finished { progress: progress(5, 0, 0) });
    (ws, id, finished)
}
fn generation_of(work: &[WorkRequest]) -> u64 {
    generation(work)
}

#[test]
fn new_error_kinds_are_recoverable_with_try_again_skip_and_cancel() {
    let kinds = [OperationErrorKind::ReadOnly, OperationErrorKind::PrivacyRestricted, OperationErrorKind::ChangedSinceScan, OperationErrorKind::UnsupportedItem, OperationErrorKind::TrashUnavailable, OperationErrorKind::SourceRemovalFailed, OperationErrorKind::JournalUnavailable, OperationErrorKind::FolderNotEmpty, OperationErrorKind::DestinationWithinSource];
    for kind in kinds {
        let (mut ws, tab) = setup(vec![item("a", EntryKind::File)]);
        select(&mut ws, tab, &["a"]);
        let (id, generation) = start(&mut ws, tab, OperationKind::Move);
        let work = scanned(&mut ws, id, generation, vec![planned(&["a"], EntryKind::File, true)]).work;
        let failure = OperationFailure { item: path(&["from", "a"]), kind };
        stepped(&mut ws, id, generation_of(&work), StepResult::RecoverableError { failure: failure.clone(), progress: progress(0, 0, 0) });
        let job = ws.operation_jobs()[0].clone();
        let decision = job.decision().expect("a decision");
        assert_eq!((decision.issue, decision.failure), (OperationIssue::RecoverableError, Some(&failure)), "{kind:?}");
        for (choice, apply_to_all) in [(OperationChoice::Replace, false), (OperationChoice::Skip, true), (OperationChoice::TryAgain, true)] {
            assert!(ws.handle(Command::DecideOperation { id, token: decision.token, item: decision.item.clone(), choice, apply_to_all }.into()).outputs.is_empty(), "{kind:?} refuses {choice:?}");
        }
        assert_eq!(effect(&decide(&mut ws, id, OperationChoice::TryAgain).work), &OperationEffect::Execute { id, generation: generation_of(&work) + 1, plan: Arc::from(vec![planned(&["a"], EntryKind::File, true)]), skipped: Arc::from([]), at: 0, choice: Some(OperationChoice::TryAgain), progress: progress(0, 0, 0) });
    }
}

#[test]
fn a_lost_worker_ends_the_job_instead_of_offering_a_skip() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, &["a", "b"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, vec![planned(&["a"], EntryKind::File, true), planned(&["b"], EntryKind::File, true)]).work;
    let unavailable = OperationFailure { item: path(&["from", "a"]), kind: OperationErrorKind::ExecutorUnavailable };
    assert!(stepped(&mut ws, id, generation_of(&work), StepResult::RecoverableError { failure: unavailable, progress: progress(0, 0, 0) }).outputs.is_empty());
    let next = stepped(&mut ws, id, generation_of(&work), StepResult::Advanced { next: 1, progress: progress(1, 0, 0) });
    ws.handle(Event::OperationExecutorUnavailable { id, generation: generation_of(&next.work) }.into());
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(ws.operation_jobs()[0].failure().map(|failure| failure.kind), Some(OperationErrorKind::ExecutorUnavailable));
}

#[test]
fn a_move_removes_its_source_folders_deepest_first_after_the_main_pass() {
    let (mut ws, id, finished) = move_tree(None);
    let (directories, at) = finalize_list(&finished.work);
    assert_eq!(directories, vec![path(&["from", "dir", "b"]), path(&["from", "dir", "a"]), path(&["from", "dir"])]);
    assert_eq!(at, 0);
    assert_eq!(status(&ws, id), OperationStatus::Finalizing { at: 0 });
    let generation = generation_of(&finished.work);
    assert!(finalized(&mut ws, id, generation + 1, FinalizeResult::Finished).outputs.is_empty(), "a stale result is ignored");
    let advanced = finalized(&mut ws, id, generation, FinalizeResult::Advanced { next: 2 });
    assert_eq!(finalize_list(&advanced.work).1, 2);
    assert_eq!(ws.operation_jobs()[0].finalization_progress(), Some((2, 3)));
    finalized(&mut ws, id, generation_of(&advanced.work), FinalizeResult::Finished);
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Succeeded));
}

#[test]
fn skipped_content_keeps_its_subtree_and_every_folder_above_it() {
    let (_, _, finished) = move_tree(Some(2));
    assert_eq!(finalize_list(&finished.work).0, vec![path(&["from", "dir", "b"])], "the skipped file keeps /dir/a and /dir");

    let (mut ws, tab) = setup(vec![item("dir", EntryKind::Directory)]);
    select(&mut ws, tab, &["dir"]);
    let (id, _) = start(&mut ws, tab, OperationKind::DeletePermanently);
    let scan_generation = generation_of(&ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work);
    let failure = ScanFailure { failure: OperationFailure { item: path(&["from", "dir", "a"]), kind: OperationErrorKind::PermissionDenied }, entry_kind: EntryKind::Directory };
    ws.handle(Event::OperationScanned { id, generation: scan_generation, plan: Err(failure) }.into());
    let rescan = decide(&mut ws, id, OperationChoice::Skip);
    let plan = vec![planned(&["dir"], EntryKind::Directory, false), planned(&["dir", "a"], EntryKind::Directory, false), planned(&["dir", "b"], EntryKind::Directory, false), planned(&["dir", "b", "c"], EntryKind::Directory, false)];
    let work = scanned(&mut ws, id, generation_of(&rescan.work), plan).work;
    let finished = stepped(&mut ws, id, generation_of(&work), StepResult::Finished { progress: progress(3, 1, 0) });
    assert_eq!(finalize_list(&finished.work).0, vec![path(&["from", "dir", "b", "c"]), path(&["from", "dir", "b"])], "the unread folder keeps itself and /dir");
}

#[test]
fn a_skipped_directory_keeps_its_empty_descendants() {
    let (mut ws, tab) = setup(vec![item("dir", EntryKind::Directory)]);
    select(&mut ws, tab, &["dir"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Move);
    let plan = vec![planned(&["dir"], EntryKind::Directory, true), planned(&["dir", "kept"], EntryKind::Directory, true), planned(&["dir", "kept", "empty"], EntryKind::Directory, true), planned(&["dir", "gone"], EntryKind::Directory, true)];
    let work = scanned(&mut ws, id, generation, plan).work;
    let next = stepped(&mut ws, id, generation_of(&work), StepResult::Advanced { next: 1, progress: progress(1, 0, 0) });
    stepped(&mut ws, id, generation_of(&next.work), StepResult::RecoverableError { failure: OperationFailure { item: path(&["from", "dir", "kept"]), kind: OperationErrorKind::PermissionDenied }, progress: progress(1, 0, 0) });
    let skip = decide(&mut ws, id, OperationChoice::Skip);
    let finished = stepped(&mut ws, id, generation_of(&skip.work), StepResult::Finished { progress: progress(2, 2, 0) });
    assert_eq!(finalize_list(&finished.work).0, vec![path(&["from", "dir", "gone"])]);
}

#[test]
fn a_folder_removal_failure_offers_try_again_skip_and_cancel() {
    let (mut ws, id, finished) = move_tree(None);
    let generation = generation_of(&finished.work);
    let failure = OperationFailure { item: path(&["from", "dir", "a"]), kind: OperationErrorKind::FolderNotEmpty };
    assert!(finalized(&mut ws, id, generation, FinalizeResult::RecoverableError { failure: failure.clone() }).outputs.is_empty(), "the error must name the current folder");
    let advanced = finalized(&mut ws, id, generation, FinalizeResult::Advanced { next: 1 });
    finalized(&mut ws, id, generation_of(&advanced.work), FinalizeResult::RecoverableError { failure: failure.clone() });
    let OperationStatus::WaitingFinalize { at: 1, failure: waiting, .. } = status(&ws, id) else { panic!("{:?}", status(&ws, id)) };
    assert_eq!(waiting, failure);
    let decision = ws.operation_jobs()[0].decision().map(|decision| (decision.issue, decision.item.clone()));
    assert_eq!(decision, Some((OperationIssue::RecoverableError, failure.item.clone())));

    let retry = decide(&mut ws, id, OperationChoice::TryAgain);
    assert_eq!(finalize_list(&retry.work), (vec![path(&["from", "dir", "b"]), path(&["from", "dir", "a"]), path(&["from", "dir"])], 1));
    finalized(&mut ws, id, generation_of(&retry.work), FinalizeResult::RecoverableError { failure: failure.clone() });
    let skip = decide(&mut ws, id, OperationChoice::Skip);
    assert!(!skip.work.iter().any(|work| matches!(work, WorkRequest::Operation(_))), "skipping /dir/a keeps /dir, so nothing is left to remove");
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Partial));

    let (mut ws, id, finished) = move_tree(None);
    finalized(&mut ws, id, generation_of(&finished.work), FinalizeResult::RecoverableError { failure: OperationFailure { item: path(&["from", "dir", "b"]), kind: OperationErrorKind::PermissionDenied } });
    let cancel = decide(&mut ws, id, OperationChoice::Cancel);
    let OperationEffect::Cancel { generation, .. } = effect(&cancel.work) else { panic!() };
    ws.handle(Event::OperationCleaned { id, generation: *generation, result: CleanupResult::Clean }.into());
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Partial), "the moved items remain moved");
}

#[test]
fn finalization_can_be_cancelled_and_survives_a_lost_worker() {
    let (mut ws, id, finished) = move_tree(None);
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    let OperationEffect::Cancel { generation, .. } = effect(&cancel.work) else { panic!() };
    assert!(finalized(&mut ws, id, generation_of(&finished.work), FinalizeResult::Finished).outputs.is_empty(), "a late result cannot finish a cancelled job");
    ws.handle(Event::OperationCleaned { id, generation: *generation, result: CleanupResult::Clean }.into());
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Partial));

    let (mut ws, id, finished) = move_tree(None);
    ws.handle(Event::OperationExecutorUnavailable { id, generation: generation_of(&finished.work) }.into());
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Partial));
}

#[test]
fn copy_and_trash_finish_without_finalization() {
    let (mut ws, tab) = setup(vec![item("dir", EntryKind::Directory)]);
    select(&mut ws, tab, &["dir"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, vec![planned(&["dir"], EntryKind::Directory, true), planned(&["dir", "a"], EntryKind::Directory, true)]).work;
    let finished = stepped(&mut ws, id, generation_of(&work), StepResult::Finished { progress: progress(2, 0, 0) });
    assert!(!finished.work.iter().any(|work| matches!(work, WorkRequest::Operation(_))));
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Succeeded));
}

#[test]
fn name_problems_keep_the_editor_open_and_close_without_cleanup() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, &["a"]);
    for problem in [NameProblem::TooLong, NameProblem::RejectedCharacters, NameProblem::Collision] {
        let (id, generation) = start(&mut ws, tab, OperationKind::Rename { to: name("b") });
        let work = scanned(&mut ws, id, generation, vec![PlannedItem { source: path(&["from", "a"]), destination: Some(path(&["from", "b"])), kind: EntryKind::File }]).work;
        stepped(&mut ws, id, generation_of(&work), StepResult::NameRejected { item: path(&["from", "a"]), problem, progress: progress(0, 0, 0) });
        let job = ws.operation_jobs().iter().find(|job| job.id() == id).unwrap();
        assert_eq!(job.name_problem(), Some(problem));
        assert!(!job.is_active());
        if problem == NameProblem::Collision {
            assert_eq!(status(&ws, id), OperationStatus::NameCollision { item: path(&["from", "a"]) });
        }
        let retry = ws.handle(Command::CancelOperation { id }.into());
        assert!(retry.work.is_empty() && notices(&retry).is_empty(), "closing a refused name leaves no cleanup or Notice");
        assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Cancelled));
    }
    let (new_id, _) = start(&mut ws, tab, OperationKind::Rename { to: name("c") });
    assert_eq!(status(&ws, new_id), OperationStatus::Scanning, "a retry is a new job");
}

#[test]
fn a_retry_after_a_refused_name_keeps_the_frozen_target() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, &["a"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Rename { to: name("b") });
    let work = scanned(&mut ws, id, generation, vec![PlannedItem { source: path(&["from", "a"]), destination: Some(path(&["from", "b"])), kind: EntryKind::File }]).work;
    stepped(&mut ws, id, generation_of(&work), StepResult::NameCollision { item: path(&["from", "a"]), progress: progress(0, 0, 0) });
    select(&mut ws, tab, &["b"]);
    assert!(matches!(ws.handle(Command::RetryName { id, name: name("a") }.into()).outputs.as_slice(), [Output::OperationRejected { reason: OperationRejection::UnchangedName, .. }]));
    assert_eq!(status(&ws, id), OperationStatus::NameCollision { item: path(&["from", "a"]) }, "an invalid retry keeps the refused job");
    let retry = ws.handle(Command::RetryName { id, name: name("c") }.into());
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Cancelled));
    let OperationEffect::Scan { intent, .. } = effect(&retry.work) else { panic!("a scan") };
    assert_eq!(intent.targets()[0].name, name("a"), "the retry renames the item the editor showed, not the new selection");
    assert_eq!(intent.kind(), &OperationKind::Rename { to: name("c") });
    assert!(ws.handle(Command::RetryName { id, name: name("d") }.into()).outputs.is_empty(), "only a refused job can be retried");
}

#[test]
fn name_less_availability_matches_the_named_checks() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Rename), Err(OperationRejection::NoTargets));
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::NewFolder), Ok(()));
    select(&mut ws, tab, &["a"]);
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Rename), Ok(()), "no placeholder name is judged unchanged or invalid");
    assert_eq!(ws.operation_availability(BrowserSide::Left, tab, OperationKind::Rename { to: name("a") }), Err(OperationRejection::UnchangedName));
    select(&mut ws, tab, &["a", "b"]);
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Rename), Err(OperationRejection::InvalidTargetCount));
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Copy), Ok(()));
    let refused = ws.handle(Command::RequestNameEditor { browser: BrowserSide::Left, tab, command: OperationCommand::Rename }.into());
    assert_eq!(refused.outputs, vec![Output::OperationRejected { browser: BrowserSide::Left, tab, command: OperationCommand::Rename, reason: OperationRejection::InvalidTargetCount }]);
    select(&mut ws, tab, &["b"]);
    let opened = ws.handle(Command::RequestNameEditor { browser: BrowserSide::Left, tab, command: OperationCommand::Rename }.into());
    assert_eq!(opened.outputs, vec![Output::NameEditorOpened { browser: BrowserSide::Left, tab, command: OperationCommand::Rename, target: Some(name("b")) }]);
    let folder = ws.handle(Command::RequestNameEditor { browser: BrowserSide::Left, tab, command: OperationCommand::NewFolder }.into());
    assert_eq!(folder.outputs, vec![Output::NameEditorOpened { browser: BrowserSide::Left, tab, command: OperationCommand::NewFolder, target: None }]);
    show(&mut ws, BrowserSide::Right, path(&["from"]), vec![]);
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Move), Err(OperationRejection::SameSourceAndDestination));
    let rejected = ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Move }.into());
    assert_eq!(rejected.outputs, vec![Output::OperationRejected { browser: BrowserSide::Left, tab, command: OperationCommand::Move, reason: OperationRejection::SameSourceAndDestination }]);
}

#[test]
fn an_unavailable_journal_disables_copy_until_it_reopens() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, &["a"]);
    let unavailable = ws.handle(Event::JournalStatus { available: false }.into());
    assert_eq!(notices(&unavailable), vec![(NoticeKind::JournalUnavailable, true)]);
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Copy), Err(OperationRejection::JournalUnavailable));
    for command in [OperationCommand::Move, OperationCommand::Rename, OperationCommand::NewFolder, OperationCommand::MoveToTrash, OperationCommand::DeletePermanently] {
        assert_eq!(ws.command_availability(BrowserSide::Left, tab, command), Ok(()), "{command:?} needs no temporaries");
    }
    let rejected = ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Copy }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::OperationRejected { reason: OperationRejection::JournalUnavailable, .. }]));
    assert_eq!(ws.handle(Command::RetryJournal.into()).work, vec![WorkRequest::ReopenJournal]);
    assert!(notices(&ws.handle(Event::JournalStatus { available: true }.into())).is_empty());
    assert_eq!(ws.command_availability(BrowserSide::Left, tab, OperationCommand::Copy), Ok(()));
}

#[test]
fn a_finished_operation_rereads_every_affected_tab_in_both_browsers() {
    let (mut ws, tab) = setup(vec![item("dir", EntryKind::Directory)]);
    let first_destination = ws.active_tab(BrowserSide::Right);
    // Inactive tabs inside the copied folder's destination and at an
    // unrelated folder, then the destination back in front.
    ws.handle(Command::NewTab { browser: BrowserSide::Right }.into());
    let inside = show(&mut ws, BrowserSide::Right, path(&["to", "dir", "deep"]), vec![]);
    ws.handle(Command::NewTab { browser: BrowserSide::Right }.into());
    show(&mut ws, BrowserSide::Right, path(&["elsewhere"]), vec![]);
    ws.handle(Command::NewTab { browser: BrowserSide::Right }.into());
    let destination = show(&mut ws, BrowserSide::Right, path(&["to"]), vec![]);
    select(&mut ws, tab, &["dir"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, vec![planned(&["dir"], EntryKind::Directory, true)]).work;
    let finished = stepped(&mut ws, id, generation_of(&work), StepResult::Finished { progress: progress(1, 0, 0) });
    let mut refreshed = reads(&finished.work).into_iter().map(|read| (read.0, read.1, read.3)).collect::<Vec<_>>();
    refreshed.sort_by_key(|(browser, tab, _)| (*browser == BrowserSide::Right, format!("{tab:?}")));
    let mut expected = vec![(BrowserSide::Left, tab, path(&["from"])), (BrowserSide::Right, first_destination, path(&["to"])), (BrowserSide::Right, inside, path(&["to", "dir", "deep"])), (BrowserSide::Right, destination, path(&["to"]))];
    expected.sort_by_key(|(browser, tab, _)| (*browser == BrowserSide::Right, format!("{tab:?}")));
    assert_eq!(refreshed, expected);
    assert_eq!(notices(&finished).len(), 1);
}

#[test]
fn only_outcomes_with_completed_work_refresh() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, &["a", "b"]);
    let plan = vec![planned(&["a"], EntryKind::File, true), planned(&["b"], EntryKind::File, true)];
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    scanned(&mut ws, id, generation, plan.clone());
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    let cleaned = ws.handle(Event::OperationCleaned { id, generation: generation_of(&cancel.work), result: CleanupResult::Clean }.into());
    assert!(reads(&cleaned.work).is_empty(), "a clean cancellation changed nothing");
    assert_eq!(notices(&cleaned).iter().map(|(_, open)| *open).collect::<Vec<_>>(), vec![false]);

    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, plan.clone()).work;
    let failed = stepped(&mut ws, id, generation_of(&work), StepResult::Failed { failure: OperationFailure { item: path(&["from", "a"]), kind: OperationErrorKind::Other }, progress: progress(0, 0, 1) });
    assert!(reads(&failed.work).is_empty(), "a failure before any progress changed nothing");
    assert!(matches!(notices(&failed).as_slice(), [(NoticeKind::OperationFinished { outcome: OperationOutcome::Failed, .. }, true)]));

    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, plan).work;
    let next = stepped(&mut ws, id, generation_of(&work), StepResult::Advanced { next: 1, progress: progress(1, 0, 0) });
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    assert!(!next.work.is_empty());
    let cleaned = ws.handle(Event::OperationCleaned { id, generation: generation_of(&cancel.work), result: CleanupResult::Clean }.into());
    assert_eq!(reads(&cleaned.work).len(), 2, "cancelling after completed work rereads source and destination");
    assert!(matches!(notices(&cleaned).as_slice(), [(NoticeKind::OperationFinished { outcome: OperationOutcome::Partial, .. }, true)]));

    for read in reads(&cleaned.work) {
        deliver(&mut ws, &read, if read.0 == BrowserSide::Left { vec![item("a", EntryKind::File), item("b", EntryKind::File)] } else { vec![] });
    }
    select(&mut ws, tab, &["a", "b"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, vec![planned(&["a"], EntryKind::File, true), planned(&["b"], EntryKind::File, true)]).work;
    let next = stepped(&mut ws, id, generation_of(&work), StepResult::Advanced { next: 1, progress: progress(1, 0, 0) });
    let failed = stepped(&mut ws, id, generation_of(&next.work), StepResult::Failed { failure: OperationFailure { item: path(&["from", "b"]), kind: OperationErrorKind::Other }, progress: progress(1, 0, 1) });
    assert_eq!(reads(&failed.work).len(), 2, "a failure after progress rereads source and destination");
}

/// Renames `/from/a` to `b` and returns the listing read that follows.
fn renamed(ws: &mut Workspace, tab: TabId) -> (BrowserSide, TabId, RequestToken, Location) {
    select(ws, tab, &["a"]);
    let (id, generation) = start(ws, tab, OperationKind::Rename { to: name("b") });
    let work = scanned(ws, id, generation, vec![PlannedItem { source: path(&["from", "a"]), destination: Some(path(&["from", "b"])), kind: EntryKind::File }]).work;
    let finished = stepped(ws, id, generation_of(&work), StepResult::Finished { progress: progress(1, 0, 0) });
    reads(&finished.work).into_iter().find(|read| read.0 == BrowserSide::Left).expect("the source tab rereads")
}

#[test]
fn a_successful_rename_selects_the_renamed_item_once_it_is_listed() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("z", EntryKind::File)]);
    let read = renamed(&mut ws, tab);
    deliver(&mut ws, &read, vec![item("b", EntryKind::File), item("z", EntryKind::File)]);
    assert_eq!(ws.selection(BrowserSide::Left).entries(), &[name("b")]);
    let again = ws.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into());
    ws.handle(Command::ClearSelection { browser: BrowserSide::Left, tab }.into());
    deliver(&mut ws, &reads(&again.work)[0], vec![item("b", EntryKind::File), item("z", EntryKind::File)]);
    assert!(ws.selection(BrowserSide::Left).entries().is_empty(), "the selection is applied only once");
}

#[test]
fn navigation_a_selection_change_or_a_listing_without_the_item_forgets_it() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("z", EntryKind::File)]);
    let read = renamed(&mut ws, tab);
    deliver(&mut ws, &read, vec![item("z", EntryKind::File)]);
    let again = ws.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into());
    deliver(&mut ws, &reads(&again.work)[0], vec![item("b", EntryKind::File), item("z", EntryKind::File)]);
    assert!(ws.selection(BrowserSide::Left).entries().is_empty());

    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("z", EntryKind::File)]);
    let read = renamed(&mut ws, tab);
    ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 1, name: name("z") }.into());
    deliver(&mut ws, &read, vec![item("b", EntryKind::File), item("z", EntryKind::File)]);
    assert_eq!(ws.selection(BrowserSide::Left).entries(), &[name("z")], "the person's later selection wins");

    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("z", EntryKind::File)]);
    renamed(&mut ws, tab);
    let elsewhere = ws.handle(Command::Navigate { browser: BrowserSide::Left, location: path(&["other"]) }.into());
    deliver(&mut ws, &reads(&elsewhere.work)[0], vec![]);
    let back = ws.handle(Command::Navigate { browser: BrowserSide::Left, location: path(&["from"]) }.into());
    deliver(&mut ws, &reads(&back.work)[0], vec![item("b", EntryKind::File)]);
    assert!(ws.selection(BrowserSide::Left).entries().is_empty());
}

#[test]
fn only_finished_jobs_can_be_dismissed() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, &["a"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    assert!(ws.handle(Command::DismissOperation { id }.into()).outputs.is_empty(), "a scanning job stays");
    let work = scanned(&mut ws, id, generation, vec![planned(&["a"], EntryKind::File, true)]).work;
    stepped(&mut ws, id, generation_of(&work), StepResult::DecisionRequired { item: path(&["from", "a"]), issue: OperationIssue::FileConflict, destination_kind: EntryKind::File, progress: progress(0, 0, 0) });
    assert!(ws.handle(Command::DismissOperation { id }.into()).outputs.is_empty(), "a waiting job stays");
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    assert!(ws.handle(Command::DismissOperation { id }.into()).outputs.is_empty(), "a job cleaning up stays");
    ws.handle(Event::OperationCleaned { id, generation: generation_of(&cancel.work), result: CleanupResult::Clean }.into());
    let dismissed = ws.handle(Command::DismissOperation { id }.into());
    assert_eq!(dismissed.outputs, vec![Output::OperationDismissed { id }]);
    assert_eq!(dismissed.work, vec![WorkRequest::Operation(OperationEffect::Release { id })], "the runtime forgets the job too");
    assert!(ws.operation_jobs().is_empty());
}

#[test]
fn byte_progress_is_display_only_and_bound_to_the_current_step() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, &["a", "b"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let work = scanned(&mut ws, id, generation, vec![planned(&["a"], EntryKind::File, true), planned(&["b"], EntryKind::File, true)]).work;
    let step = generation_of(&work);
    let bytes = ByteProgress { done: 10, total: 40 };
    assert!(ws.handle(Event::OperationProgress { id, generation: step - 1, bytes }.into()).outputs.is_empty());
    let shown = ws.handle(Event::OperationProgress { id, generation: step, bytes }.into());
    assert!(matches!(shown.outputs.as_slice(), [Output::OperationChanged { job }] if job.bytes() == Some(bytes)));
    assert!(shown.work.is_empty());
    assert_eq!(status(&ws, id), OperationStatus::Running { at: 0 });
    let next = stepped(&mut ws, id, step, StepResult::Advanced { next: 1, progress: progress(1, 0, 0) });
    assert_eq!(ws.operation_jobs()[0].bytes(), None, "a new step starts without bytes");
    assert!(ws.handle(Event::OperationProgress { id, generation: step, bytes }.into()).outputs.is_empty(), "the old step's progress is stale");
    assert!(!next.work.is_empty());
}

#[test]
fn activation_opens_files_and_packages_and_resolves_links() {
    let package = Entry::new(name("App.app"), EntryKind::Directory).with_package(true);
    let entries = vec![item("folder", EntryKind::Directory), package, item("file.txt", EntryKind::File), item("link", EntryKind::Symlink { points_to_directory: true })];
    let (mut ws, tab) = setup(entries);
    let open = |ws: &mut Workspace, row: usize, text: &str| ws.handle(Command::OpenEntry { browser: BrowserSide::Left, tab, row, name: name(text) }.into());
    assert_eq!(open(&mut ws, 2, "file.txt").outputs, vec![Output::OpenItem { item: path(&["from", "file.txt"]) }]);
    assert_eq!(open(&mut ws, 1, "App.app").outputs, vec![Output::OpenItem { item: path(&["from", "App.app"]) }]);
    let contents = ws.handle(Command::ShowPackageContents { browser: BrowserSide::Left, tab, row: 1, name: name("App.app") }.into());
    assert_eq!(reads(&contents.work)[0].3, path(&["from", "App.app"]));
    assert!(ws.handle(Command::ShowPackageContents { browser: BrowserSide::Left, tab, row: 0, name: name("folder") }.into()).work.is_empty(), "only a package has contents to show");
    let (mut ws, tab) = setup(vec![item("link", EntryKind::Symlink { points_to_directory: true }), item("other", EntryKind::Symlink { points_to_directory: false })]);
    let resolve = ws.handle(Command::OpenEntry { browser: BrowserSide::Left, tab, row: 0, name: name("link") }.into());
    let [WorkRequest::ResolveItem { token, item, .. }] = resolve.work.as_slice() else { panic!("{:?}", resolve.work) };
    assert!(resolve.outputs.is_empty(), "a link waits for its target");
    let (token, link) = (*token, item.clone());
    let stale = Event::ItemResolved { browser: BrowserSide::Left, tab, token: token.next(), item: link.clone(), target: ResolvedTarget::Folder };
    assert!(ws.handle(stale.into()).work.is_empty());
    let entered = ws.handle(Event::ItemResolved { browser: BrowserSide::Left, tab, token, item: link.clone(), target: ResolvedTarget::Folder }.into());
    assert_eq!(reads(&entered.work)[0].3, link, "a link to a folder navigates through the link");
    assert!(ws.handle(Event::ItemResolved { browser: BrowserSide::Left, tab, token, item: link, target: ResolvedTarget::Folder }.into()).work.is_empty(), "a result applies once");
}

#[test]
fn a_resolved_link_opens_its_target_or_reports_a_failure() {
    let (mut ws, tab) = setup(vec![item("link", EntryKind::Symlink { points_to_directory: false })]);
    for (target, expected) in [(ResolvedTarget::File, true), (ResolvedTarget::Package, true), (ResolvedTarget::Unavailable, false)] {
        let resolve = ws.handle(Command::OpenEntry { browser: BrowserSide::Left, tab, row: 0, name: name("link") }.into());
        let [WorkRequest::ResolveItem { token, item, .. }] = resolve.work.as_slice() else { panic!() };
        let result = ws.handle(Event::ItemResolved { browser: BrowserSide::Left, tab, token: *token, item: item.clone(), target }.into());
        if expected {
            assert_eq!(result.outputs, vec![Output::OpenItem { item: path(&["from", "link"]) }]);
        } else {
            assert_eq!(notices(&result), vec![(NoticeKind::OpenFailed { item: path(&["from", "link"]) }, true)]);
        }
    }
    let refused = ws.handle(Event::OpenFailed { item: path(&["from", "x"]) }.into());
    assert_eq!(notices(&refused), vec![(NoticeKind::OpenFailed { item: path(&["from", "x"]) }, true)]);
}

#[test]
fn a_link_result_after_leaving_its_folder_is_ignored() {
    let (mut ws, tab) = setup(vec![item("link", EntryKind::Symlink { points_to_directory: true })]);
    let resolve = ws.handle(Command::OpenEntry { browser: BrowserSide::Left, tab, row: 0, name: name("link") }.into());
    let [WorkRequest::ResolveItem { token, item, .. }] = resolve.work.as_slice() else { panic!() };
    let (token, item) = (*token, item.clone());
    show(&mut ws, BrowserSide::Left, path(&["elsewhere"]), vec![]);
    assert!(ws.handle(Event::ItemResolved { browser: BrowserSide::Left, tab, token, item, target: ResolvedTarget::Folder }.into()).work.is_empty());
}

#[test]
fn quick_successes_are_recorded_and_sweep_failures_open_notices() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File)]);
    select(&mut ws, tab, &["a"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::MoveToTrash);
    let work = scanned(&mut ws, id, generation, vec![planned(&["a"], EntryKind::File, false)]).work;
    let finished = stepped(&mut ws, id, generation_of(&work), StepResult::Finished { progress: progress(1, 0, 0) });
    assert!(matches!(notices(&finished).as_slice(), [(NoticeKind::OperationFinished { outcome: OperationOutcome::Succeeded, .. }, false)]));
    assert!(notices(&ws.handle(Event::TemporariesSwept { removed: 0, failed: vec![] }.into())).is_empty());
    assert_eq!(notices(&ws.handle(Event::TemporariesSwept { removed: 2, failed: vec![] }.into())), vec![(NoticeKind::TemporariesSwept { removed: 2, failed: vec![] }, false)]);
    assert_eq!(notices(&ws.handle(Event::TemporariesSwept { removed: 0, failed: vec![path(&["vol"])] }.into())), vec![(NoticeKind::TemporariesSwept { removed: 0, failed: vec![path(&["vol"])] }, true)]);
}

#[test]
fn quitting_with_running_jobs_needs_confirmation_and_then_cancels_them() {
    let (mut idle, _) = setup(vec![]);
    assert_eq!(idle.handle(Command::Quit { confirmed: false }.into()).outputs, vec![Output::QuitAccepted]);
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File)]);
    select(&mut ws, tab, &["a"]);
    let (copy, _) = start(&mut ws, tab, OperationKind::Copy);
    select(&mut ws, tab, &["b"]);
    let (delete, _) = start(&mut ws, tab, OperationKind::DeletePermanently);
    assert_eq!(ws.handle(Command::Quit { confirmed: false }.into()).outputs, vec![Output::QuitConfirmationRequired { running: 1 }], "a pending confirmation is not running");
    assert_eq!(status(&ws, copy), OperationStatus::Scanning, "asking changes nothing");
    let quit = ws.handle(Command::Quit { confirmed: true }.into());
    assert_eq!(quit.outputs.last(), Some(&Output::QuitAccepted));
    assert!(quit.work.iter().any(|work| matches!(work, WorkRequest::Operation(OperationEffect::Cancel { id, .. }) if *id == copy)));
    assert_eq!(status(&ws, copy), OperationStatus::Cancelling);
    assert_eq!(status(&ws, delete), OperationStatus::Finished(OperationOutcome::Cancelled));
    assert!(ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::MoveToTrash }.into()).outputs.is_empty(), "nothing new starts while quitting");
}

#[test]
fn a_cancel_that_interrupts_a_step_keeps_the_entries_it_completed() {
    let (mut ws, tab) = setup(vec![item("a", EntryKind::File), item("b", EntryKind::File), item("c", EntryKind::File)]);
    select(&mut ws, tab, &["a", "b", "c"]);
    let (id, generation) = start(&mut ws, tab, OperationKind::Copy);
    let plan = vec![planned(&["a"], EntryKind::File, true), planned(&["b"], EntryKind::File, true), planned(&["c"], EntryKind::File, true)];
    let step = generation_of(&scanned(&mut ws, id, generation, plan).work);
    let cancel = ws.handle(Command::CancelOperation { id }.into());
    assert!(stepped(&mut ws, id, step, StepResult::DecisionRequired { item: path(&["from", "a"]), issue: OperationIssue::FileConflict, destination_kind: EntryKind::File, progress: progress(0, 0, 0) }).outputs.is_empty(), "a decision cannot reopen a cancelled job");
    assert!(stepped(&mut ws, id, step, StepResult::Advanced { next: 3, progress: progress(3, 0, 0) }).outputs.is_empty(), "an inconsistent report is ignored");
    assert!(stepped(&mut ws, id, step, StepResult::Advanced { next: 2, progress: progress(2, 0, 0) }).outputs.is_empty(), "the interrupted step's progress is taken silently");
    assert_eq!(status(&ws, id), OperationStatus::Cancelling);
    assert!(stepped(&mut ws, id, step, StepResult::Advanced { next: 2, progress: progress(2, 0, 0) }).outputs.is_empty());
    let cleaned = ws.handle(Event::OperationCleaned { id, generation: generation_of(&cancel.work), result: CleanupResult::Clean }.into());
    assert_eq!(status(&ws, id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(ws.operation_jobs()[0].progress(), progress(2, 0, 0));
    assert!(!reads(&cleaned.work).is_empty(), "the completed copies refresh their listings");
}
