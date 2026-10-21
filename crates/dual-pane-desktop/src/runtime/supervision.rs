//! Listing worker supervision, restarts, and event delivery.

use super::*;

#[derive(Clone)]
pub(super) struct Delivery {
    pub(super) events: Sender<Event>,
    pub(super) wake_pending: Arc<AtomicBool>,
    pub(super) wake: Arc<Mutex<Wake>>,
    /// The newest pending byte progress of each operation; older progress
    /// is replaced rather than queued.
    pub(super) progress: Arc<Mutex<HashMap<OperationId, Event>>>,
}

/// A completed listing and its row change against the job's previous rows.
pub(super) type ListingOutcome = Option<Result<(Arc<[Entry]>, Option<Vec<RowChange>>), ListingErrorKind>>;

pub(super) enum WorkerOutcome {
    Completed(ListingOutcome),
    Panicked,
}

pub(super) const WORKER_THREAD_NAME: &str = "listing-worker";

pub(super) const PROBE_THREAD_NAME: &str = "location-probe";

pub(super) const CLIPBOARD_THREAD_NAME: &str = "clipboard-worker";

pub(super) const INITIAL_RESTART_DELAY: Duration = Duration::from_millis(100);

pub(super) const MAX_RESTART_DELAY: Duration = Duration::from_secs(5);

pub(super) static INSTALL_PANIC_HOOK: Once = Once::new();

pub(super) fn install_panic_hook() {
    INSTALL_PANIC_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |information| {
            let message: Option<&[u8]> = match thread::current().name() {
                Some(WORKER_THREAD_NAME) => Some(b"Dual Pane listing worker stopped unexpectedly.\n"),
                Some(PROBE_THREAD_NAME) => Some(b"Dual Pane location probe stopped unexpectedly.\n"),
                _ => None,
            };
            if let Some(message) = message {
                io::stderr().write_all(message).ok();
            } else {
                previous(information);
            }
        }));
    });
}

pub(super) fn supervise(queue: impl IntoIterator<Item = Job>, source_factory: FolderItemsSourceFactory, delivery: Delivery) {
    supervise_with(queue, source_factory, delivery, |worker| thread::Builder::new().name(WORKER_THREAD_NAME.to_owned()).spawn(worker).map(|_| ()), thread::sleep);
}

pub(super) fn supervise_with(queue: impl IntoIterator<Item = Job>, source_factory: FolderItemsSourceFactory, delivery: Delivery, mut spawn_worker: impl FnMut(WorkerTask) -> io::Result<()>, mut sleep: impl FnMut(Duration)) {
    let mut delay = INITIAL_RESTART_DELAY;
    for job in queue {
        if job.state.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let worker_source_factory = Arc::clone(&source_factory);
        let worker_job = job.clone();
        let worker: WorkerTask = Box::new(move || {
            let outcome = match catch_unwind(AssertUnwindSafe(|| {
                let source = worker_source_factory();
                let entries = source(&worker_job.location, worker_job.sort, worker_job.visibility, &worker_job.state.cancelled)?;
                Some(entries.map(|entries| {
                    let changes = worker_job.previous.as_deref().map(|previous| listing_changes(previous, &entries));
                    (entries, changes)
                }))
            })) {
                Ok(outcome) => WorkerOutcome::Completed(outcome),
                Err(payload) => {
                    drop(payload);
                    WorkerOutcome::Panicked
                }
            };
            result_sender.send((worker_job, outcome)).ok();
        });
        if spawn_worker(worker).is_err() {
            finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: ListingErrorKind::Internal });
            sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        }
        let Ok((job, outcome)) = result_receiver.recv() else {
            finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: ListingErrorKind::Internal });
            sleep(delay);
            delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            continue;
        };
        match outcome {
            WorkerOutcome::Completed(Some(Ok((entries, changes)))) => {
                finish(&delivery, &job, Event::FolderItemsLoaded { browser: job.browser, tab: job.tab, token: job.token, entries, changes });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Completed(Some(Err(kind))) => {
                finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Completed(None) => {
                finish(&delivery, &job, Event::FolderItemsCancelled { browser: job.browser, tab: job.tab, token: job.token });
                delay = INITIAL_RESTART_DELAY;
            }
            WorkerOutcome::Panicked => {
                finish(&delivery, &job, Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: ListingErrorKind::Internal });
                sleep(delay);
                delay = delay.saturating_mul(2).min(MAX_RESTART_DELAY);
            }
        }
    }
}

pub(super) fn finish(delivery: &Delivery, job: &Job, event: Event) {
    if job.state.claim_terminal() {
        deliver_event(delivery, event);
    }
}

pub(super) fn deliver_event(delivery: &Delivery, event: Event) {
    if delivery.events.send(event).is_ok() {
        wake(delivery);
    }
}

/// Posts at most one GUI wake while a drain is pending.
pub(super) fn wake(delivery: &Delivery) {
    if !delivery.wake_pending.swap(true, Ordering::SeqCst) {
        (delivery.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))();
    }
}

/// The read a listing event completes. Other events carry no read token.
pub(super) fn event_address(event: &Event) -> Option<(BrowserSide, TabId, RequestToken)> {
    match event {
        Event::FolderItemsLoaded { browser, tab, token, .. } | Event::FolderItemsFailed { browser, tab, token, .. } | Event::FolderItemsCancelled { browser, tab, token, .. } => Some((*browser, *tab, *token)),
        Event::ClipboardPublished { .. } | Event::ClipboardChanged { .. } | Event::ClipboardRead { .. } | Event::ClipboardPreflighted { .. } | Event::FilterProjected { .. } | Event::PreviewFinished { .. } | Event::OperationScanned { .. } | Event::OperationStepped { .. } | Event::OperationFinalized { .. } | Event::OperationCleaned { .. } | Event::OperationExecutorUnavailable { .. } | Event::OperationProgress { .. } | Event::ItemResolved { .. } | Event::OpenFailed { .. } | Event::JournalStatus { .. } | Event::TemporariesSwept { .. } | Event::LocationInvalidated { .. } | Event::WatcherReady { .. } | Event::WatcherStatusChanged { .. } | Event::WatchedLocationInvalidated { .. } | Event::ApplicationActivityChanged { .. } | Event::ReadCapacityAvailable | Event::FavoriteTargetProbed { .. } | Event::ScreenshotsFolderProbed { .. } | Event::SettingsSaved { .. } | Event::SettingsSaveFailed { .. } | Event::SessionSaveFailed { .. } | Event::SettingsLoaded { .. } | Event::SettingsLoadedWithSession { .. } | Event::SettingsLoadFailed { .. } | Event::SettingsLoadFailedAtLaunch { .. } | Event::SettingsLoadTimedOut | Event::StartupPeriodElapsed | Event::SettingsReset { .. } | Event::SettingsResetFailed { .. } => None,
    }
}
