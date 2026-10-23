use super::*;

#[test]
fn cancelling_mid_copy_keeps_completed_work_and_removes_only_its_temporaries() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    fs::write(run.path("from/large.bin"), vec![1_u8; 4 * 1024 * 1024]).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    run.lane.cancelled.insert(OperationId::new(1), Arc::clone(&cancelled));
    let flag = Arc::clone(&cancelled);
    *run.fs.before_copy.lock().unwrap() = Some(Box::new(move |to| {
        if !to.to_string_lossy().contains("-1-0.") {
            flag.store(true, Ordering::Relaxed);
        }
    }));
    let id = run.start(&["a.txt", "large.bin"], OperationKind::Copy);
    assert_eq!(id, OperationId::new(1));
    assert_eq!(run.status(id), OperationStatus::Running { at: 1 }, "the step reported the copy it completed, and the next one stopped at once");
    assert_eq!(run.read("to/a.txt"), "a");
    assert!(run.temporaries().is_empty(), "the interrupted copy discards its temporary at once");
    let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
    run.drive(work);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert!(run.temporaries().is_empty());
    assert!(!run.path("to/large.bin").exists());
    assert_eq!(run.read("to/a.txt"), "a", "completed work stays");
}

#[test]
fn a_cancel_with_nothing_in_flight_still_reports_its_cleanup() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.write("to/a.txt", "old");
    let id = run.start(&["a.txt"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
    run.drive(work);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Cancelled));
    let mut lost = Run::new();
    assert_eq!(lost.lane.run(OperationEffect::Cancel { id: OperationId::new(42), generation: 3 }), vec![Event::OperationCleaned { id: OperationId::new(42), generation: 3, result: CleanupResult::Clean }], "a cancel with no state still reports");
}

#[test]
fn cleanup_removes_a_temporary_the_step_could_not_and_reports_what_it_cannot() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.write("to/a.txt", "old");
    let id = run.start(&["a.txt"], OperationKind::Copy);
    // A temporary that this operation recorded but could not discard.
    let leftover = run.path("to").join(crate::operation_journal::temporary_name(LaunchId::from_bytes([9; 16]), id.get(), 7));
    fs::write(&leftover, b"partial").unwrap();
    run.lane.services.journal.record(id.get(), &run.path("to"), fs::metadata(run.path("to")).unwrap().dev()).unwrap();
    run.fs.failing_removals.lock().unwrap().push(leftover.clone());
    let work = run.ws.handle(Command::CancelOperation { id }.into()).work;
    run.drive(work);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::CleanupUncertain), "a temporary that cannot be shown removed is reported");
    assert_eq!(run.read("to/a.txt"), "old");
}

#[test]
fn the_write_primitive_decides_a_conflict_the_fast_path_missed() {
    let mut run = Run::new();
    run.write("from/a.txt", "mine");
    *run.fs.racing_destination.lock().unwrap() = Some(run.path("to/a.txt"));
    let id = run.start(&["a.txt"], OperationKind::Copy);
    assert_eq!(run.waiting(id).0, OperationIssue::FileConflict);
    assert_eq!(run.read("to/a.txt"), "racer", "the item that appeared first is kept");
    assert!(run.temporaries().is_empty());
}

#[test]
fn a_moved_folder_holding_skipped_content_moves_entry_by_entry() {
    let mut run = Run::new();
    run.write("from/dir/a.txt", "a");
    run.write("from/dir/locked/secret.txt", "secret");
    fs::set_permissions(run.path("from/dir/locked"), fs::Permissions::from_mode(0o000)).unwrap();
    let id = run.start(&["dir"], OperationKind::Move);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::PermissionDenied), "the unreadable folder asks during the scan");
    run.decide(id, OperationChoice::Skip, false);
    fs::set_permissions(run.path("from/dir/locked"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
    assert_eq!(run.read("to/dir/a.txt"), "a");
    assert_eq!(run.read("from/dir/locked/secret.txt"), "secret", "the skipped content and the folder above it stay");
    assert!(!run.path("to/dir/locked").exists());
}

#[test]
fn a_destination_inside_the_source_by_another_spelling_is_refused() {
    let mut run = Run::new();
    fs::create_dir_all(run.path("from/dir/inner")).unwrap();
    let variant = run.path("FROM/DIR/inner");
    if !variant.is_dir() {
        // A case-sensitive volume has no second spelling to test.
        return;
    }
    fs::remove_dir(run.path("to")).unwrap();
    run.show();
    let destination = location_from_path(&variant).unwrap();
    let read = run.ws.handle(Command::Navigate { browser: BrowserSide::Right, location: destination }.into()).work;
    run.drive(read);
    let tab = run.ws.active_tab(BrowserSide::Left);
    run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("dir") }.into());
    let started = run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::Copy }.into());
    let id = run.ws.operation_jobs()[0].id();
    run.drive(started.work);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::DestinationWithinSource), "device and inode catch what name comparison cannot");
}
