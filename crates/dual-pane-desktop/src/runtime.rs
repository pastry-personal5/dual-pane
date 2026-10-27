use std::collections::{HashMap, VecDeque};
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::thread;
use std::time::Duration;

use dual_pane_application::{ClipboardAction, ClipboardRejection, ClipboardRequestId, DefaultApplicationRequest, Event, FavoriteProbeOutcome, FileAssociationLookup, FileAssociationRequest, FilterFailure, FilterRequest, OpenWithRequest, PreviewRequest, PreviewResult, ResolvedTarget, RowChange, SettingsFailure, VisibilityPolicy, WorkRequest, listing_changes, project_filter};
use dual_pane_domain::{BrowserSide, BrowserTabs, Entry, EntryKind, ListingErrorKind, Location, OperationId, OperationRoot, RequestToken, SortSpec, TabId};

use crate::clipboard_native;
use crate::location_probe::resolve_item;
use crate::location_watcher::WatcherHandle;
use crate::native_location::path_from_location;
use crate::native_shell::{change_default_application, open_with_application, resolve_file_associations};
use crate::operation_lane::{OperationLane, Publisher, Services};
use crate::operation_scan::{NativeKind, native_kind};
use crate::preview_reader::read_preview;

mod clipboard;
mod column_lane;
mod filter_lane;
mod job_queue;
mod probes;
mod supervision;
#[cfg(test)]
mod tests;

use clipboard::*;
use column_lane::*;
pub use column_lane::{FolderChoiceRequest, FolderChoiceResult};
use filter_lane::*;
pub use job_queue::READER_LANES_PER_BROWSER;
use job_queue::*;
use probes::*;
use supervision::*;

type FolderItemsOutcome = Option<Result<Arc<[Entry]>, ListingErrorKind>>;
pub type FolderItemsSource = Box<dyn Fn(&Location, SortSpec, VisibilityPolicy, &AtomicBool) -> FolderItemsOutcome + Send>;
pub type FolderItemsSourceFactory = Arc<dyn Fn() -> FolderItemsSource + Send + Sync>;
pub type Wake = Box<dyn Fn() + Send>;
/// Checks one location on the probe worker, never on the GUI thread.
pub type LocationProbe = Box<dyn Fn(&Location) -> FavoriteProbeOutcome + Send>;
type WorkerTask = Box<dyn FnOnce() + Send>;

pub trait WorkRunner {
    /// Dispatches outside work without waiting. A terminal event is returned
    /// directly when the request can finish before leaving the GUI thread.
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event>;
    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool);
    fn request_folder_choices(&mut self, request: FolderChoiceRequest) -> Option<FolderChoiceResult> {
        Some(FolderChoiceResult { request, outcome: Err(ListingErrorKind::Internal) })
    }
    fn cancel_folder_choices(&mut self, _browser: BrowserSide, _generation: u64) {}
    fn take_folder_choices(&mut self) -> Vec<FolderChoiceResult> {
        Vec::new()
    }
    /// After the event loop has exited: stops file operations at a safe
    /// boundary and waits at most `timeout` for their cleanup.
    fn finish_operations(&mut self, _timeout: Duration) {}
}

/// One bounded listing queue per browser, served by a fixed number of reader
/// lanes so a slow or blocked folder in one tab cannot stall the Browser's
/// other tabs. A lane reads, sorts, and diffs against the request's previous
/// rows, so the GUI thread only applies the finished change. Each lane runs one job at a time; a panic is contained,
/// reported safely, and followed by a replacement after backoff.
pub struct Runtime {
    left_jobs: JobSender,
    right_jobs: JobSender,
    /// One serialized lane for short location probes, separate from reads.
    probes: SyncSender<ProbeJob>,
    clipboard: SyncSender<ClipboardJob>,
    associations: SyncSender<AssociationJob>,
    events: Receiver<Event>,
    next_event: Option<Event>,
    wake_pending: Arc<AtomicBool>,
    outstanding: HashMap<RequestToken, (BrowserSide, TabId, Arc<JobState>)>,
    preview_jobs: [SyncSender<PreviewJob>; 2],
    preview_pending: [Option<PreviewJob>; 2],
    preview_outstanding: HashMap<(BrowserSide, u64), Arc<AtomicBool>>,
    column_senders: [SyncSender<ColumnJob>; 2],
    column_results: Receiver<FolderChoiceResult>,
    column_outstanding: HashMap<u64, (BrowserSide, u64, Arc<AtomicBool>)>,
    filter_lanes: Option<[FilterLane; 2]>,
    /// Reads per Browser that may be queued, running, or awaiting delivery.
    read_capacity: usize,
    delivery: Delivery,
    /// The file-operation lane, once attached; without it operations fail
    /// closed.
    operations: Option<OperationLane>,
    watcher: WatcherHandle,
}

#[derive(Clone)]
struct Job {
    browser: BrowserSide,
    tab: TabId,
    token: RequestToken,
    location: Location,
    sort: SortSpec,
    visibility: VisibilityPolicy,
    /// The shown Folder Items this read's result is diffed against.
    previous: Option<Arc<[Entry]>>,
    /// True for an entered-path check: `location` is the candidate to resolve
    /// and `sort`, `visibility`, and `previous` are unused.
    resolve: bool,
    state: Arc<JobState>,
}

struct PreviewJob {
    request: PreviewRequest,
    cancelled: Arc<AtomicBool>,
}

enum AssociationJob {
    Resolve(FileAssociationRequest),
    Open(OpenWithRequest),
    SetDefault(DefaultApplicationRequest),
}

struct JobState {
    cancelled: AtomicBool,
    terminal_claimed: AtomicBool,
}

impl JobState {
    fn claim_terminal(&self) -> bool {
        self.terminal_claimed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }
}

impl Runtime {
    pub fn start(source_factory: FolderItemsSourceFactory, probe: LocationProbe, wake: Wake) -> io::Result<Self> {
        Self::start_with_lanes(source_factory, probe, wake, READER_LANES_PER_BROWSER)
    }

    fn start_with_lanes(source_factory: FolderItemsSourceFactory, probe: LocationProbe, wake: Wake, lanes: usize) -> io::Result<Self> {
        Self::start_with(source_factory, probe, wake, lanes, |browser, queue, source_factory, delivery| thread::Builder::new().name(format!("listing-supervisor-{browser:?}")).spawn(move || supervise(queue, source_factory, delivery)).map(|_| ()))
    }

    /// Starts `lanes` supervisors per Browser, then the probe lane. If any
    /// cannot start, every queue closes so the threads that did start finish.
    fn start_with(source_factory: FolderItemsSourceFactory, probe: LocationProbe, wake: Wake, lanes: usize, mut spawn: impl FnMut(BrowserSide, JobReceiver, FolderItemsSourceFactory, Delivery) -> io::Result<()>) -> io::Result<Self> {
        install_panic_hook();
        let lanes = lanes.max(1);
        let (left_jobs, left_queue) = job_queue();
        let (right_jobs, right_queue) = job_queue();
        let (event_sender, events) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let delivery = Delivery { events: event_sender, wake_pending: Arc::clone(&wake_pending), wake: Arc::new(Mutex::new(wake)), progress: Arc::default() };
        for (browser, queue) in [(BrowserSide::Left, left_queue), (BrowserSide::Right, right_queue)] {
            for _ in 0..lanes {
                spawn(browser, queue.clone(), Arc::clone(&source_factory), delivery.clone())?;
            }
        }
        let (probes, probe_jobs) = mpsc::sync_channel(PENDING_PROBE_CAPACITY);
        let probe_delivery = delivery.clone();
        thread::Builder::new().name(PROBE_THREAD_NAME.to_owned()).spawn(move || run_probes(probe_jobs, probe, probe_delivery))?;
        let (clipboard, clipboard_jobs) = mpsc::sync_channel(8);
        let clipboard_delivery = delivery.clone();
        thread::Builder::new().name(CLIPBOARD_THREAD_NAME.to_owned()).spawn(move || run_clipboard(clipboard_jobs, clipboard_delivery))?;
        let (associations, association_jobs) = mpsc::sync_channel(32);
        let association_delivery = delivery.clone();
        thread::Builder::new().name("file-association-worker".to_owned()).spawn(move || {
            for job in association_jobs {
                let event = match job {
                    AssociationJob::Resolve(request) => {
                        let outcome = panic::catch_unwind(AssertUnwindSafe(|| resolve_file_associations(&request))).unwrap_or_else(|_| FileAssociationLookup::Rejected("The file-association service stopped unexpectedly.".to_owned()));
                        Event::FileAssociationsResolved { request, outcome }
                    }
                    AssociationJob::Open(request) => {
                        let result = panic::catch_unwind(AssertUnwindSafe(|| open_with_application(&request))).unwrap_or_else(|_| Err("The Open With service stopped unexpectedly.".to_owned()));
                        Event::OpenWithFinished { request, result }
                    }
                    AssociationJob::SetDefault(request) => {
                        let result = panic::catch_unwind(AssertUnwindSafe(|| change_default_application(&request))).unwrap_or_else(|_| Err("The default-application service stopped unexpectedly.".to_owned()));
                        Event::DefaultApplicationFinished { request, result }
                    }
                };
                deliver_event(&association_delivery, event);
            }
        })?;
        let watcher_delivery = delivery.clone();
        let watcher = WatcherHandle::start(Arc::new(move |event| deliver_event(&watcher_delivery, event))).unwrap_or_else(|_| WatcherHandle::unavailable());
        let mut preview_senders = Vec::with_capacity(2);
        for browser in [BrowserSide::Left, BrowserSide::Right] {
            let (sender, receiver) = mpsc::sync_channel::<PreviewJob>(PENDING_PREVIEW_CAPACITY);
            let preview_delivery = delivery.clone();
            thread::Builder::new().name(format!("preview-reader-{browser:?}")).spawn(move || {
                for job in receiver {
                    let result = panic::catch_unwind(AssertUnwindSafe(|| read_preview(&job.request, &job.cancelled))).unwrap_or(PreviewResult::DecodeFailed);
                    deliver_event(&preview_delivery, Event::PreviewFinished { request: job.request, result });
                }
            })?;
            preview_senders.push(sender);
        }
        let preview_jobs: [SyncSender<PreviewJob>; 2] = preview_senders.try_into().map_err(|_| io::Error::from(io::ErrorKind::Other))?;
        let filter_lanes = Some([FilterLane::start(BrowserSide::Left, delivery.clone())?, FilterLane::start(BrowserSide::Right, delivery.clone())?]);
        let (column_senders, column_results) = start_column_lanes(delivery.clone())?;
        Ok(Self { left_jobs, right_jobs, probes, clipboard, associations, events, next_event: None, wake_pending, outstanding: HashMap::new(), preview_jobs, preview_pending: [None, None], preview_outstanding: HashMap::new(), column_senders, column_results, column_outstanding: HashMap::new(), filter_lanes, read_capacity: PENDING_READ_CAPACITY + lanes, delivery, operations: None, watcher })
    }

    /// Starts the file-operation lane, which reports through this runtime.
    pub fn attach_operations(&mut self, services: Services) -> io::Result<()> {
        let events = self.delivery.clone();
        let progress = self.delivery.clone();
        let publisher = Publisher {
            event: Arc::new(move |event| deliver_event(&events, event)),
            progress: Arc::new(move |id, event| {
                progress.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(id, event);
                wake(&progress);
            }),
        };
        self.operations = Some(OperationLane::start(services, publisher)?);
        Ok(())
    }

    fn receive(&mut self) -> Option<Event> {
        let event = self.next_event.take().or_else(|| self.events.try_recv().ok())?;
        if let Event::PreviewFinished { request, .. } = &event {
            self.preview_outstanding.remove(&(request.browser, request.token));
            self.start_pending_preview(request.browser);
        }
        if let Some((browser, tab, token)) = event_address(&event)
            && self.outstanding.get(&token).is_some_and(|(owner, owner_tab, _)| *owner == browser && *owner_tab == tab)
        {
            self.outstanding.remove(&token);
        }
        Some(event)
    }
    fn start_pending_preview(&mut self, browser: BrowserSide) {
        let index = preview_index(browser);
        let Some(job) = self.preview_pending[index].take() else { return };
        match self.preview_jobs[index].try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(job)) => self.preview_pending[index] = Some(job),
            Err(TrySendError::Disconnected(job)) => deliver_event(&self.delivery, Event::PreviewFinished { request: job.request, result: PreviewResult::Unavailable }),
        }
    }
}

impl WorkRunner for Runtime {
    fn request_folder_choices(&mut self, request: FolderChoiceRequest) -> Option<FolderChoiceResult> {
        let browser = request.browser;
        if self.column_outstanding.values().filter(|(owner, _, _)| *owner == browser).count() > PENDING_COLUMN_CAPACITY {
            return Some(FolderChoiceResult { request, outcome: Err(ListingErrorKind::Busy) });
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.column_outstanding.insert(request.id, (browser, request.generation, Arc::clone(&cancelled)));
        let id = request.id;
        let sender = &self.column_senders[preview_index(browser)];
        match sender.try_send(ColumnJob { request, cancelled }) {
            Ok(()) => None,
            Err(TrySendError::Full(job)) => {
                self.column_outstanding.remove(&id);
                Some(FolderChoiceResult { request: job.request, outcome: Err(ListingErrorKind::Busy) })
            }
            Err(TrySendError::Disconnected(job)) => {
                self.column_outstanding.remove(&id);
                Some(FolderChoiceResult { request: job.request, outcome: Err(ListingErrorKind::Internal) })
            }
        }
    }
    fn cancel_folder_choices(&mut self, browser: BrowserSide, generation: u64) {
        self.column_outstanding.retain(|_, (owner, owner_generation, cancelled)| {
            if *owner == browser && *owner_generation == generation {
                cancelled.store(true, Ordering::Relaxed);
                false
            } else {
                true
            }
        });
    }
    fn take_folder_choices(&mut self) -> Vec<FolderChoiceResult> {
        let mut delivered = Vec::new();
        while let Ok(result) = self.column_results.try_recv() {
            if self.column_outstanding.remove(&result.request.id).is_some() {
                delivered.push(result);
            }
        }
        delivered
    }
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
        match request {
            WorkRequest::ResolveFileAssociations { request } => self.associations.try_send(AssociationJob::Resolve(request)).err().map(|error| match error {
                TrySendError::Full(AssociationJob::Resolve(request)) | TrySendError::Disconnected(AssociationJob::Resolve(request)) => Event::FileAssociationsResolved { request, outcome: FileAssociationLookup::Rejected("The file-association service is busy or unavailable.".to_owned()) },
                _ => unreachable!("association queue preserves the job variant"),
            }),
            WorkRequest::ScheduleFileAssociationRetry { folder, generation, delay_ms } => {
                let delivery = self.delivery.clone();
                let retry_folder = folder.clone();
                match thread::Builder::new().name("file-association-retry".to_owned()).spawn(move || {
                    thread::sleep(Duration::from_millis(delay_ms));
                    deliver_event(&delivery, Event::FileAssociationRetryDue { folder: retry_folder, generation });
                }) {
                    Ok(_) => None,
                    Err(_) => Some(Event::FileAssociationRetryDue { folder, generation }),
                }
            }
            WorkRequest::OpenWithApplication { request } => self.associations.try_send(AssociationJob::Open(request)).err().map(|error| match error {
                TrySendError::Full(AssociationJob::Open(request)) | TrySendError::Disconnected(AssociationJob::Open(request)) => Event::OpenWithFinished { request, result: Err("The Open With service is busy or unavailable.".to_owned()) },
                _ => unreachable!("association queue preserves the job variant"),
            }),
            WorkRequest::SetDefaultApplication { request } => self.associations.try_send(AssociationJob::SetDefault(request)).err().map(|error| match error {
                TrySendError::Full(AssociationJob::SetDefault(request)) | TrySendError::Disconnected(AssociationJob::SetDefault(request)) => Event::DefaultApplicationFinished { request, result: Err("The default-application service is busy or unavailable.".to_owned()) },
                _ => unreachable!("association queue preserves the job variant"),
            }),
            WorkRequest::PublishClipboard { request, transfer, action, finder_roots } => match self.clipboard.try_send(ClipboardJob::Publish { request, transfer, action, roots: finder_roots }) {
                Ok(()) => None,
                Err(TrySendError::Full(ClipboardJob::Publish { request, transfer, .. }) | TrySendError::Disconnected(ClipboardJob::Publish { request, transfer, .. })) => Some(Event::ClipboardPublished { request, transfer, generation: clipboard_native::generation_and_offer().0, succeeded: false }),
                Err(_) => unreachable!("clipboard queue preserves the job variant"),
            },
            WorkRequest::ReadClipboard { request } => match self.clipboard.try_send(ClipboardJob::Read { request }) {
                Ok(()) => None,
                Err(TrySendError::Full(ClipboardJob::Read { request }) | TrySendError::Disconnected(ClipboardJob::Read { request })) => Some(Event::ClipboardRead { request, generation: clipboard_native::generation_and_offer().0, snapshot: dual_pane_application::ClipboardSnapshot::Unavailable }),
                Err(_) => unreachable!("clipboard queue preserves the job variant"),
            },
            WorkRequest::PreflightClipboard { request, generation, sources } => {
                let job = ProbeJob::ClipboardPreflight { request, generation, sources };
                self.probes.try_send(job).err().map(|(TrySendError::Full(job) | TrySendError::Disconnected(job))| job.fail())
            }
            WorkRequest::PreflightDrop { preflight } => {
                let job = ProbeJob::DropPreflight { preflight };
                self.probes.try_send(job).err().map(|(TrySendError::Full(job) | TrySendError::Disconnected(job))| job.fail())
            }
            WorkRequest::ProjectFilter { request } => {
                let index = preview_index(request.browser);
                if self.filter_lanes.as_ref().is_some_and(|lanes| lanes[index].enqueue(request.clone())) { None } else { Some(Event::FilterProjected { request, result: Err(FilterFailure::WorkerUnavailable) }) }
            }
            WorkRequest::CancelFilter { browser, tab } => {
                if let Some(lanes) = &self.filter_lanes {
                    lanes[preview_index(browser)].cancel(tab);
                }
                None
            }
            WorkRequest::ReadPreview { request } => {
                let browser = request.browser;
                let token = request.token;
                let cancelled = Arc::new(AtomicBool::new(false));
                self.preview_outstanding.insert((browser, token), Arc::clone(&cancelled));
                let index = preview_index(browser);
                match self.preview_jobs[index].try_send(PreviewJob { request, cancelled }) {
                    Ok(()) => None,
                    Err(TrySendError::Full(job)) => {
                        if let Some(replaced) = self.preview_pending[index].replace(job) {
                            self.preview_outstanding.remove(&(browser, replaced.request.token));
                            deliver_event(&self.delivery, Event::PreviewFinished { request: replaced.request, result: PreviewResult::Cancelled });
                        }
                        None
                    }
                    Err(TrySendError::Disconnected(job)) => {
                        self.preview_outstanding.remove(&(browser, token));
                        Some(Event::PreviewFinished { request: job.request, result: PreviewResult::Unavailable })
                    }
                }
            }
            WorkRequest::CancelPreview { browser, token } => {
                if let Some(cancelled) = self.preview_outstanding.get(&(browser, token)) {
                    cancelled.store(true, Ordering::Relaxed);
                }
                let index = preview_index(browser);
                if self.preview_pending[index].as_ref().is_some_and(|job| job.request.token == token) {
                    if let Some(job) = self.preview_pending[index].take() {
                        deliver_event(&self.delivery, Event::PreviewFinished { request: job.request, result: PreviewResult::Cancelled });
                    }
                    self.preview_outstanding.remove(&(browser, token));
                }
                None
            }
            WorkRequest::Operation(effect) => {
                if let Some(operations) = &self.operations {
                    return operations.submit(effect);
                }
                let (id, generation) = match effect {
                    dual_pane_application::OperationEffect::Scan { id, generation, .. } | dual_pane_application::OperationEffect::Execute { id, generation, .. } | dual_pane_application::OperationEffect::Cancel { id, generation } | dual_pane_application::OperationEffect::Finalize { id, generation, .. } => (id, generation),
                    dual_pane_application::OperationEffect::Release { .. } => return None,
                };
                Some(Event::OperationExecutorUnavailable { id, generation })
            }
            WorkRequest::WatchLocation { location, generation } => {
                if self.watcher.watch(location.clone(), generation) {
                    None
                } else {
                    Some(Event::WatcherReady { location, generation, status: dual_pane_application::MonitoringStatus::Unavailable })
                }
            }
            WorkRequest::UnwatchLocation { location, generation } => {
                self.watcher.unwatch(&location, generation);
                None
            }
            WorkRequest::SetWatchActivity { active } => {
                self.watcher.set_active(active);
                None
            }
            WorkRequest::UpdateWatchVisibility { visible } => {
                self.watcher.set_visible(visible);
                None
            }
            WorkRequest::WatchReadCompleted { location, succeeded } => {
                self.watcher.read_completed(location, succeeded);
                None
            }
            WorkRequest::ReadDirectory { browser, tab, token, location, sort, visibility, previous } => {
                if self.outstanding.values().filter(|(owner, _, _)| *owner == browser).count() >= self.read_capacity {
                    return Some(Event::FolderItemsFailed { browser, tab, token, kind: ListingErrorKind::Busy });
                }
                let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
                self.outstanding.insert(token, (browser, tab, Arc::clone(&state)));
                let jobs = match browser {
                    BrowserSide::Left => &self.left_jobs,
                    BrowserSide::Right => &self.right_jobs,
                };
                if let Err((job, closed)) = jobs.send(Job { browser, tab, token, location, sort, visibility, previous, resolve: false, state })
                    && job.state.claim_terminal()
                {
                    self.outstanding.remove(&job.token);
                    return Some(Event::FolderItemsFailed { browser: job.browser, tab: job.tab, token: job.token, kind: if closed { ListingErrorKind::Internal } else { ListingErrorKind::Busy } });
                }
                None
            }
            WorkRequest::ResolvePath { browser, tab, token, candidate } => {
                if self.outstanding.values().filter(|(owner, _, _)| *owner == browser).count() >= self.read_capacity {
                    return Some(Event::PathResolved { browser, tab, token, result: Err(ListingErrorKind::Busy) });
                }
                let state = Arc::new(JobState { cancelled: AtomicBool::new(false), terminal_claimed: AtomicBool::new(false) });
                self.outstanding.insert(token, (browser, tab, Arc::clone(&state)));
                let jobs = match browser {
                    BrowserSide::Left => &self.left_jobs,
                    BrowserSide::Right => &self.right_jobs,
                };
                if let Err((job, closed)) = jobs.send(Job { browser, tab, token, location: candidate, sort: SortSpec::default(), visibility: VisibilityPolicy::default(), previous: None, resolve: true, state })
                    && job.state.claim_terminal()
                {
                    self.outstanding.remove(&job.token);
                    return Some(Event::PathResolved { browser, tab, token, result: Err(if closed { ListingErrorKind::Internal } else { ListingErrorKind::Busy }) });
                }
                None
            }
            WorkRequest::CancelPathResolve { browser, tab, token } => {
                // The application already forgot the check, so no event answers it.
                if let Some((owner, owner_tab, state)) = self.outstanding.get(&token)
                    && *owner == browser
                    && *owner_tab == tab
                {
                    state.cancelled.store(true, Ordering::Relaxed);
                    state.claim_terminal();
                    self.outstanding.remove(&token);
                }
                None
            }
            WorkRequest::Cancel { browser, tab, token } => {
                if let Some((owner, owner_tab, state)) = self.outstanding.get(&token) {
                    if *owner != browser || *owner_tab != tab {
                        return None;
                    }
                    state.cancelled.store(true, Ordering::Relaxed);
                    if !state.claim_terminal() {
                        return None;
                    }
                    self.outstanding.remove(&token);
                    return Some(Event::FolderItemsCancelled { browser, tab, token });
                }
                None
            }
            // Settings work belongs to the settings worker; reaching the
            // listing runtime means there is none.
            WorkRequest::SaveSettings { revision, .. } => Some(Event::SettingsSaveFailed { revision, failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::SaveSession { revision, .. } => Some(Event::SessionSaveFailed { revision, failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::LoadSettings => Some(Event::SettingsLoadFailedAtLaunch { failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::ResetSettings => Some(Event::SettingsResetFailed { failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::ProbeScreenshotsFolder { location } => self.probe(location.clone(), Box::new(move |outcome| Event::ScreenshotsFolderProbed { location, outcome })),
            WorkRequest::ProbeFavoriteTarget { item_id, target } => self.probe(target.clone(), Box::new(move |outcome| Event::FavoriteTargetProbed { item_id, target, outcome })),
            WorkRequest::ResolveItem { browser, tab, token, item } => {
                let job = ProbeJob::Resolve { location: item.clone(), event: Box::new(move |target| Event::ItemResolved { browser, tab, token, item, target }) };
                self.probes.try_send(job).err().map(|(TrySendError::Full(job) | TrySendError::Disconnected(job))| job.fail())
            }
            WorkRequest::ReopenJournal => match &self.operations {
                Some(operations) => {
                    operations.reopen_journal();
                    None
                }
                None => Some(Event::JournalStatus { available: false }),
            },
        }
    }

    /// Delivers results first, then the newest byte progress of each
    /// operation, which a full slice may leave for the next drain.
    fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
        self.wake_pending.swap(false, Ordering::SeqCst);
        let mut events: Vec<Event> = std::iter::from_fn(|| self.receive()).take(max).collect();
        if self.next_event.is_none() {
            self.next_event = self.events.try_recv().ok();
        }
        let mut progress = self.delivery.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        while events.len() < max
            && let Some(id) = progress.keys().next().copied()
        {
            events.extend(progress.remove(&id));
        }
        let more = self.next_event.is_some() || !progress.is_empty();
        (events, more)
    }

    fn finish_operations(&mut self, timeout: Duration) {
        if let Some(operations) = &self.operations {
            operations.shut_down(timeout);
        }
    }
}

impl Runtime {
    /// Queues a probe without waiting. If the probe queue is full or the lane
    /// has stopped, the outcome is `Failed` at once so no request is left
    /// pending; a failed probe proves nothing about its location.
    fn probe(&self, location: Location, event: Box<dyn FnOnce(FavoriteProbeOutcome) -> Event + Send>) -> Option<Event> {
        self.probes.try_send(ProbeJob::Probe { location, event }).err().map(|(TrySendError::Full(job) | TrySendError::Disconnected(job))| job.fail())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for (_, _, state) in self.outstanding.values() {
            state.cancelled.store(true, Ordering::Relaxed);
        }
        for cancelled in self.preview_outstanding.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}
