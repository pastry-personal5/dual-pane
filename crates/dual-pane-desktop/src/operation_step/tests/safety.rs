use super::*;

#[test]
fn rename_refuses_collisions_and_hard_links_but_allows_a_case_only_change() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.write("from/b.txt", "b");
    let id = run.start(&["a.txt"], OperationKind::Rename { to: name("b.txt") });
    assert_eq!(run.status(id), OperationStatus::NameCollision { item: run.location("from/a.txt") });
    assert_eq!(run.read("from/b.txt"), "b");
    run.ws.handle(Command::CancelOperation { id }.into());

    fs::hard_link(run.path("from/a.txt"), run.path("from/link.txt")).unwrap();
    let id = run.start(&["a.txt"], OperationKind::Rename { to: name("link.txt") });
    assert_eq!(run.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().name_problem(), Some(NameProblem::Collision), "a hard link to the same file is still another item");
    run.ws.handle(Command::CancelOperation { id }.into());
    fs::remove_file(run.path("from/link.txt")).unwrap();

    let id = run.start(&["a.txt"], OperationKind::Rename { to: name("A.txt") });
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    let names = fs::read_dir(run.path("from")).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect::<HashSet<_>>();
    assert!(names.contains("A.txt") && !names.contains("a.txt"), "{names:?}");

    let long = "n".repeat(300);
    let id = run.start(&["A.txt"], OperationKind::Rename { to: name(&long) });
    assert_eq!(run.ws.operation_jobs().iter().find(|job| job.id() == id).unwrap().name_problem(), Some(NameProblem::TooLong));
}

#[test]
fn new_folder_creates_or_reports_a_name_problem() {
    let mut run = Run::new();
    run.write("from/taken/inner", "x");
    let id = run.start(&[], OperationKind::NewFolder { name: name("made") });
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(run.path("from/made").is_dir());
    let id = run.start(&[], OperationKind::NewFolder { name: name("taken") });
    assert_eq!(run.status(id), OperationStatus::NameCollision { item: run.location("from/taken") });
    assert!(run.path("from/taken/inner").exists());
}

#[test]
fn a_destination_inside_the_source_through_a_link_is_refused() {
    let mut run = Run::new();
    fs::create_dir_all(run.path("from/dir/inner")).unwrap();
    fs::remove_dir(run.path("to")).unwrap();
    symlink(run.path("from/dir/inner"), run.path("to")).unwrap();
    let id = run.start(&["dir"], OperationKind::Copy);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::DestinationWithinSource));
    assert!(fs::read_dir(run.path("from/dir/inner")).unwrap().next().is_none(), "nothing was copied into itself");
    run.decide(id, OperationChoice::Skip, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Partial));
}

#[test]
fn an_item_changed_since_the_scan_is_not_touched() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    run.show();
    let tab = run.ws.active_tab(BrowserSide::Left);
    run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("a.txt") }.into());
    let started = run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::DeletePermanently }.into());
    assert!(started.work.is_empty());
    let id = run.ws.operation_jobs()[0].id();
    let confirmed = run.ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work;
    let [WorkRequest::Operation(scan @ OperationEffect::Scan { .. })] = confirmed.as_slice() else { panic!() };
    let scanned = run.lane.run(scan.clone());
    let mut work = Vec::new();
    for event in scanned {
        work.extend(run.ws.handle(event.into()).work);
    }
    run.write("from/a.txt", "rewritten after the scan");
    run.drive(work);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::ChangedSinceScan));
    assert_eq!(run.read("from/a.txt"), "rewritten after the scan");
}

#[test]
fn permanent_deletion_unlinks_files_then_removes_folders_deepest_first() {
    let mut run = Run::new();
    run.write("from/dir/a/x.txt", "x");
    run.write("from/dir/b.txt", "b");
    symlink(run.path("to"), run.path("from/dir/outside")).unwrap();
    run.write("to/kept.txt", "kept");
    run.fs.failing_removals.lock().unwrap().push(run.path("from/dir/a"));
    run.show();
    let tab = run.ws.active_tab(BrowserSide::Left);
    run.ws.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("dir") }.into());
    run.ws.handle(Command::StartOperation { browser: BrowserSide::Left, tab, kind: OperationKind::DeletePermanently }.into());
    let id = run.ws.operation_jobs()[0].id();
    let work = run.ws.handle(Command::ConfirmPermanentDelete { id, targets: 1 }.into()).work;
    run.drive(work);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::PermissionDenied), "a folder that cannot be removed asks");
    assert!(!run.path("from/dir/a/x.txt").exists() && fs::symlink_metadata(run.path("from/dir/outside")).is_err());
    assert_eq!(run.read("to/kept.txt"), "kept", "a link is removed, never followed");
    run.fs.failing_removals.lock().unwrap().clear();
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(!run.path("from/dir").exists());
}

#[test]
fn trash_moves_each_root_or_reports_trash_unavailable_without_deleting() {
    let mut run = Run::new();
    run.write("from/a.txt", "a");
    let id = run.start(&["a.txt"], OperationKind::MoveToTrash);
    assert_eq!(run.waiting(id).1, Some(OperationErrorKind::TrashUnavailable));
    assert_eq!(run.read("from/a.txt"), "a", "never deleted instead");
    let trash = run.path("Trash");
    fs::create_dir(&trash).unwrap();
    *run.fs.trash.lock().unwrap() = Some(trash.clone());
    run.decide(id, OperationChoice::TryAgain, false);
    assert_eq!(run.status(id), OperationStatus::Finished(OperationOutcome::Succeeded));
    assert!(trash.join("a.txt").exists());
    assert!(run.events.contains(&Event::LocationInvalidated { location: location_from_path(&trash).unwrap() }));
}
