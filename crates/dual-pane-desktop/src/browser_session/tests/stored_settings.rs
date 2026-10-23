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
