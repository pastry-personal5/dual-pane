use super::*;

#[test]
fn worker_delivers_without_calling_on_submit_thread() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path).unwrap();
    assert!(worker.submit(SettingsJob::Save { revision: 1, snapshot: SettingsState::new().snapshot() }));
    assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Saved { revision: 1 }]));
}
#[test]
fn worker_remains_available_after_reset() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path).unwrap();
    assert!(worker.submit(SettingsJob::Reset));
    assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Reset { .. }]));
    assert!(worker.submit(SettingsJob::Save { revision: 2, snapshot: SettingsState::new().snapshot() }));
    assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Saved { revision: 2 }]));
}
fn sorted_snapshot() -> SettingsSnapshot {
    let mut snapshot = SettingsState::new().snapshot();
    snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Type, SortDirection::Descending)));
    snapshot
}
#[test]
fn a_load_after_a_queued_save_reports_the_saved_settings() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path).unwrap();
    let snapshot = sorted_snapshot();
    assert!(worker.submit(SettingsJob::Save { revision: 3, snapshot: snapshot.clone() }));
    assert!(worker.submit(SettingsJob::Load));
    let results = worker.shutdown(None, Duration::from_secs(10));
    assert!(matches!(results.as_slice(), [SettingsResult::Saved { revision: 3 }, SettingsResult::Loaded(loaded)] if loaded.snapshot.folder_sorts == snapshot.folder_sorts));
}
#[test]
fn a_reset_discards_saves_queued_before_it() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path).unwrap();
    assert!(worker.submit(SettingsJob::Save { revision: 3, snapshot: sorted_snapshot() }));
    assert!(worker.submit(SettingsJob::Reset));
    assert!(worker.submit(SettingsJob::Load));
    let results = worker.shutdown(None, Duration::from_secs(10));
    assert!(matches!(results.as_slice(), [SettingsResult::Reset { .. }, SettingsResult::Loaded(loaded)] if loaded.snapshot.folder_sorts.is_empty()));
}
fn stored_sorts(path: &Path) -> Vec<(Location, SortSpec)> {
    SettingsDatabase::open(path).unwrap().load().unwrap().folder_sorts
}
#[test]
fn failed_reset_restores_moved_files_and_later_saves_reach_disk() {
    let (directory, path) = temp_database();
    let backup = directory.path().join("settings.failed");
    let db = SettingsDatabase::open(&path).unwrap();
    drop(db);
    fs::write(with_suffix(&path, "-wal"), []).unwrap();
    // A directory where the WAL backup belongs makes that move fail after
    // the main database file has already moved.
    fs::create_dir(with_suffix(&backup, "-wal")).unwrap();
    let (database, result) = reset_database_to(&path, None, &backup);
    assert!(result.is_err());
    assert!(path.exists());
    assert!(!backup.exists());
    let mut database = database.expect("the original database reopens");
    let mut snapshot = SettingsState::new().snapshot();
    snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Size, SortDirection::Ascending)));
    database.save(&snapshot).unwrap();
    assert_eq!(stored_sorts(&path), snapshot.folder_sorts);
}
#[test]
fn worker_reports_a_failed_open_by_category_and_stays_available() {
    let (_directory, path) = temp_database();
    let connection = Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
    drop(connection);
    let worker = SettingsWorker::start(path.clone()).unwrap();
    assert!(worker.submit(SettingsJob::Load));
    assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::LoadFailed(error)] if error.failure() == SettingsFailure::UnsupportedSchema));
    assert!(worker.submit(SettingsJob::Reset));
    assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Reset { backup: Some(_) }]));
    assert!(worker.submit(SettingsJob::Load));
    assert!(matches!(wait_for_results(&worker).as_slice(), [SettingsResult::Loaded(_)]));
}
#[test]
fn shutdown_flushes_the_final_snapshot_before_returning() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path.clone()).unwrap();
    let mut snapshot = SettingsState::new().snapshot();
    snapshot.folder_sorts.push((non_utf8_location(), SortSpec::new(SortField::Modified, SortDirection::Descending)));
    let results = worker.shutdown(Some((9, snapshot.clone())), Duration::from_secs(10));
    assert!(results.iter().any(|result| matches!(result, SettingsResult::Saved { revision: 9 })));
    assert_eq!(stored_sorts(&path), snapshot.folder_sorts);
}
#[test]
fn shutdown_flushes_a_session_when_no_settings_snapshot_is_dirty() {
    let (_directory, path) = temp_database();
    let worker = SettingsWorker::start(path.clone()).unwrap();
    let location = Location::root().join(&EntryName::new("saved").unwrap());
    let session = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: location.clone() }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location }], active_tab: Some(0) }, active_browser: BrowserSide::Left, layout: WindowLayout { frame: WindowFrame { x: 0, y: 0, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 } };
    let results = worker.shutdown_with_session(None, Some((12, session.clone())), Duration::from_secs(10));
    assert!(results.iter().any(|result| matches!(result, SettingsResult::SessionSaved { revision: 12 })));
    assert_eq!(SettingsDatabase::open(&path).unwrap().load_with_session().unwrap().session, SessionSnapshot::Saved(session));
}
