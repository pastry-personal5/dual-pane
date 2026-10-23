use super::*;

#[test]
fn a_copy_merges_folders_keeps_metadata_and_never_clobbers() {
    let mut run = Run::new();
    run.write("from/dir/keep.txt", "new");
    run.write("from/dir/sub/fresh.txt", "fresh");
    run.write("to/dir/keep.txt", "old");
    fs::set_permissions(run.path("from/dir/sub"), fs::Permissions::from_mode(0o750)).unwrap();
    fs::set_permissions(run.path("from/dir/sub/fresh.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    symlink("sub", run.path("from/dir/link")).unwrap();
    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id), (OperationIssue::FileConflict, None), "the existing folder merges; its file conflicts");
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/keep.txt"), "old", "a skipped conflict keeps the destination");
    assert_eq!(run.read("to/dir/sub/fresh.txt"), "fresh");
    assert_eq!(fs::metadata(run.path("to/dir/sub")).unwrap().permissions().mode() & 0o777, 0o750, "a created folder gets its source's metadata");
    assert_eq!(fs::metadata(run.path("to/dir/sub/fresh.txt")).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(fs::symlink_metadata(run.path("to/dir/link")).unwrap().file_type().is_symlink(), "a link is copied as a link");
    assert_eq!(run.read("from/dir/keep.txt"), "new", "the source is untouched");
    assert!(run.temporaries().is_empty());

    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    run.decide(id, OperationChoice::Replace, true);
    assert_eq!(run.waiting(id).0, OperationIssue::LinkCollision, "a link is never replaced");
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/keep.txt"), "new", "Replace swaps in the complete copy");
    assert!(run.temporaries().is_empty(), "no temporary is left over");
}

#[test]
fn an_unlisted_destination_child_still_requires_a_conflict_decision() {
    let mut run = Run::new();
    run.write("from/dir/.DS_Store", "new");
    run.write("to/dir/.DS_Store", "old");
    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.read("to/dir/.DS_Store"), "old");
}

#[test]
fn a_kind_mismatch_replaces_neither_side() {
    let mut run = Run::new();
    run.write("from/clash/inner.txt", "inner");
    run.write("to/clash", "a file");
    run.write("from/file.txt", "file");
    fs::create_dir(run.path("to/file.txt")).unwrap();
    let id = run.start(&["clash", "file.txt"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::KindMismatch);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.waiting(id).0, OperationIssue::KindMismatch);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/clash"), "a file");
    assert!(run.path("to/file.txt").is_dir());
}

#[test]
fn a_failed_replacement_leaves_the_destination_and_removes_only_its_temporary() {
    let mut run = Run::new();
    run.write("from/a.txt", "new content");
    run.write("to/a.txt", "old content");
    let foreign = run.root.path().join("to").join(crate::operation_journal::temporary_name(LaunchId::from_bytes([1; 16]), 1, 0));
    fs::write(&foreign, b"another launch").unwrap();
    let id = run.start(&["a.txt"], OperationKind::Copy);
    run.fs.failing_copies.lock().unwrap().push((run.path("to"), libc::ENOSPC));
    run.decide(id, OperationChoice::Replace, false);
    assert_eq!(run.waiting(id), (OperationIssue::RecoverableError, Some(OperationErrorKind::NoSpace)));
    assert_eq!(run.read("to/a.txt"), "old content", "the destination is untouched until the data is complete");
    assert_eq!(run.temporaries(), vec![foreign.file_name().unwrap().to_string_lossy().into_owned()], "only this operation's temporary is removed");
    run.fs.failing_copies.lock().unwrap().clear();
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict, "trying again rediscovers the conflict");
    run.decide(id, OperationChoice::Replace, false);
    assert_eq!(run.read("to/a.txt"), "new content");
}

#[test]
fn the_journal_records_a_folder_before_its_first_temporary_and_forgets_it_after() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    let journal = run.lane.services.journal.clone();
    let folder = run.path("to");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&seen);
    *run.fs.before_copy.lock().unwrap() = Some(Box::new(move |_| observed.lock().unwrap().push(journal.folders(1))));
    let id = run.start(&["a.txt"], OperationKind::Copy);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert_eq!(seen.lock().unwrap().as_slice(), &[vec![folder]], "recorded before the copy wrote its temporary");
    assert!(run.lane.services.journal.folders(id.get()).is_empty(), "cleared once nothing is left");
}

#[test]
fn a_same_volume_move_renames_a_whole_folder_once() {
    let mut run = Run::new();
    run.write("from/dir/a/x.txt", "x");
    run.write("from/dir/b.txt", "b");
    let id = run.start(&["dir"], OperationKind::Move);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert_eq!(*run.fs.exclusive_renames.lock().unwrap(), 1);
    assert_eq!(run.ws.operation_jobs()[0].progress().completed, 4);
    assert_eq!(run.read("to/dir/a/x.txt"), "x");
    assert!(!run.path("from/dir").exists(), "finalization finds the folder already gone");
}

#[test]
fn a_merging_move_moves_entries_one_by_one_and_keeps_folders_with_skipped_content() {
    let mut run = Run::new();
    run.write("from/dir/a/x.txt", "new x");
    run.write("from/dir/b/y.txt", "y");
    run.write("to/dir/a/x.txt", "old x");
    let id = run.start(&["dir"], OperationKind::Move);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/b/y.txt"), "y");
    assert!(!run.path("from/dir/b").exists(), "an emptied folder is removed");
    assert_eq!(run.read("from/dir/a/x.txt"), "new x", "the skipped file and the folders above it stay");
    assert_eq!(run.read("to/dir/a/x.txt"), "old x");
}

#[test]
fn a_cross_volume_move_copies_first_and_removes_each_source_after() {
    let mut run = Run::new();
    run.write("from/dir/a.txt", "a");
    run.write("from/b.txt", "b");
    *run.fs.other_volume.lock().unwrap() = Some(run.path("to"));
    run.fs.failing_removals.lock().unwrap().push(run.path("from/b.txt"));
    let id = run.start(&["dir", "b.txt"], OperationKind::Move);
    assert_eq!(*run.fs.exclusive_renames.lock().unwrap(), 2, "each file is placed from a temporary, never renamed across volumes");
    assert_eq!(run.waiting(id), (OperationIssue::RecoverableError, Some(OperationErrorKind::SourceRemovalFailed)));
    assert_eq!((run.read("to/b.txt"), run.read("from/b.txt")), ("b".to_owned(), "b".to_owned()), "the copy is kept with its source");
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::SourceRemovalFailed), "Try Again retries only the removal");
    run.fs.failing_removals.lock().unwrap().clear();
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(!run.path("from/b.txt").exists() && !run.path("from/dir").exists());
    assert_eq!(run.read("to/dir/a.txt"), "a");
    assert!(run.temporaries().is_empty());

    run.write("from/c.txt", "c");
    run.fs.failing_removals.lock().unwrap().push(run.path("from/c.txt"));
    let id = run.start(&["c.txt"], OperationKind::Move);
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!((run.read("to/c.txt"), run.read("from/c.txt")), ("c".to_owned(), "c".to_owned()), "Skip keeps both copies");
}

#[test]
fn a_cross_volume_move_without_a_journal_stops_at_the_item_until_it_reopens() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    *run.fs.other_volume.lock().unwrap() = Some(run.path("to"));
    let blocked = run.path("blocked");
    fs::write(&blocked, b"").unwrap();
    run.lane.services.journal = Journal::new(Some(blocked.join(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
    let id = run.start(&["a.txt"], OperationKind::Move);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::JournalUnavailable));
    assert!(run.temporaries().is_empty() && run.path("from/a.txt").exists());
    run.lane.services.journal = Journal::new(Some(run.path(JOURNAL_FILE_NAME)), LaunchId::from_bytes([9; 16]));
    run.decide(id, OperationChoice::TryAgain, false);
    assert!(run.events.contains(&Event::JournalStatus { available: true }), "Try Again reopens the journal");
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
}
