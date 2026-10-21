//! The background thread that saves settings.

use super::*;

pub(super) const SAVE_DEBOUNCE: Duration = Duration::from_millis(50);

pub enum SettingsJob {
    Load,
    Save {
        revision: u64,
        snapshot: SettingsSnapshot,
    },
    SaveSession {
        revision: u64,
        session: WorkspaceSnapshot,
    },
    /// Saves this snapshot without waiting for more changes, then stops.
    Quit {
        revision: u64,
        snapshot: SettingsSnapshot,
    },
    /// Flushes independent settings and workspace snapshots before stopping.
    QuitWithSession {
        settings: Option<(u64, SettingsSnapshot)>,
        session: Option<(u64, WorkspaceSnapshot)>,
    },
    Reset,
}

pub enum SettingsResult {
    Loaded(Box<LoadedSettings>),
    LoadFailed(SettingsStorageError),
    Saved { revision: u64 },
    SaveFailed { revision: u64, error: SettingsStorageError },
    SessionSaved { revision: u64 },
    SessionSaveFailed { revision: u64, error: SettingsStorageError },
    Reset { backup: Option<PathBuf> },
    ResetFailed(SettingsStorageError),
}

/// One ordered worker. Replaceable saves are coalesced before I/O; quit always
/// flushes the most recent snapshot without blocking the GUI caller.
pub struct SettingsWorker {
    jobs: Sender<SettingsJob>,
    results: Receiver<SettingsResult>,
}

impl SettingsWorker {
    pub fn start(path: PathBuf) -> Result<Self, std::io::Error> {
        Self::start_with_wake(path, Box::new(|| {}))
    }
    pub fn start_with_wake(path: PathBuf, wake: Box<dyn Fn() + Send>) -> Result<Self, std::io::Error> {
        let (jobs, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        thread::Builder::new().name("settings-worker".to_owned()).spawn(move || worker_loop(WorkerState { path, database: None }, receiver, sender, wake))?;
        Ok(Self { jobs, results })
    }
    pub fn submit(&self, job: SettingsJob) -> bool {
        self.jobs.send(job).is_ok()
    }
    pub fn take_results(&self) -> Vec<SettingsResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.results.try_recv() {
            results.push(result);
        }
        results
    }
    /// Flushes queued saves and `final_save`, then waits at most `timeout` for
    /// the worker to stop. A worker that is still busy is left to finish
    /// during process teardown. Returns the results delivered while waiting.
    pub fn shutdown(self, final_save: Option<(u64, SettingsSnapshot)>, timeout: Duration) -> Vec<SettingsResult> {
        self.shutdown_with_session(final_save, None, timeout)
    }
    pub fn shutdown_with_session(self, final_settings: Option<(u64, SettingsSnapshot)>, final_session: Option<(u64, WorkspaceSnapshot)>, timeout: Duration) -> Vec<SettingsResult> {
        let Self { jobs, results } = self;
        if final_settings.is_some() || final_session.is_some() {
            jobs.send(SettingsJob::QuitWithSession { settings: final_settings, session: final_session }).ok();
        }
        drop(jobs);
        let deadline = Instant::now() + timeout;
        let mut delivered = Vec::new();
        while let Ok(result) = results.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            delivered.push(result);
        }
        delivered
    }
}

struct WorkerState {
    path: PathBuf,
    /// Opened on first use and after a reset; `None` after a failed open.
    database: Option<SettingsDatabase>,
}

impl WorkerState {
    fn database(&mut self) -> Result<&mut SettingsDatabase, SettingsStorageError> {
        let database = match self.database.take() {
            Some(database) => database,
            None => SettingsDatabase::open(&self.path)?,
        };
        Ok(self.database.insert(database))
    }
    fn save(&mut self, revision: u64, snapshot: &SettingsSnapshot) -> SettingsResult {
        self.database().and_then(|database| database.save(snapshot)).map_or_else(|error| SettingsResult::SaveFailed { revision, error }, |()| SettingsResult::Saved { revision })
    }
    fn save_session(&mut self, revision: u64, session: &WorkspaceSnapshot) -> SettingsResult {
        self.database().and_then(|database| database.save_session(session)).map_or_else(|error| SettingsResult::SessionSaveFailed { revision, error }, |()| SettingsResult::SessionSaved { revision })
    }
}

fn worker_loop(mut state: WorkerState, jobs: Receiver<SettingsJob>, results: Sender<SettingsResult>, wake: Box<dyn Fn() + Send>) {
    let send = |result| {
        results.send(result).ok();
        wake();
    };
    while let Ok(job) = jobs.recv() {
        let mut latest = None;
        let mut latest_session = None;
        let mut quitting = false;
        let mut next = Some(job);
        while let Some(job) = next.take() {
            match job {
                SettingsJob::Load => {
                    // A load reports what storage holds, so a save requested
                    // before it must reach storage first.
                    if let Some((revision, snapshot)) = latest.take() {
                        send(state.save(revision, &snapshot));
                    }
                    if let Some((revision, session)) = latest_session.take() {
                        send(state.save_session(revision, &session));
                    }
                    send(state.database().and_then(|database| database.load_with_session()).map_or_else(SettingsResult::LoadFailed, |loaded| SettingsResult::Loaded(Box::new(loaded))));
                }
                SettingsJob::Save { revision, snapshot } => latest = Some((revision, snapshot)),
                SettingsJob::SaveSession { revision, session } => latest_session = Some((revision, session)),
                SettingsJob::Quit { revision, snapshot } => {
                    latest = Some((revision, snapshot));
                    quitting = true;
                }
                SettingsJob::QuitWithSession { settings, session } => {
                    if let Some(settings) = settings {
                        latest = Some(settings)
                    }
                    if let Some(session) = session {
                        latest_session = Some(session)
                    }
                    quitting = true;
                }
                SettingsJob::Reset => {
                    let (database, result) = reset_database(&state.path, state.database.take());
                    state.database = database;
                    match result {
                        Ok(backup) => {
                            // A confirmed reset supersedes unsaved replaceable state
                            // that was queued before it.
                            latest = None;
                            latest_session = None;
                            send(SettingsResult::Reset { backup });
                        }
                        Err(error) => send(SettingsResult::ResetFailed(error)),
                    }
                }
            }
            if !quitting {
                next = jobs.recv_timeout(SAVE_DEBOUNCE).ok();
            }
        }
        if let Some((revision, snapshot)) = latest {
            send(state.save(revision, &snapshot));
        }
        if let Some((revision, session)) = latest_session {
            send(state.save_session(revision, &session));
        }
        if quitting {
            return;
        }
    }
}
