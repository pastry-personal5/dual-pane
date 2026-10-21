use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dual_pane_adapters::{BrowserPresenter, BrowserViewModel, EditorOutcome, FavoritesEvent, InputController, OperationsPresenter, OperationsViewModel, UNREPRESENTABLE_NAME_TEXT, UiEvent, WorkspacePresenter, WorkspaceViewModel, name_rejection_text};
use dual_pane_application::{ClipboardAction, Command, ContextMenuKind, Event, FavoriteEdit, FavoriteRejection, FolderItemsMenuSnapshot, FolderItemsMenuTarget, Input, Output, SettingsFailure, WindowLayout, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, DecisionToken, EntryName, Location, OperationChoice, OperationCommand, OperationId, OperationRejection, TabId};

use crate::native_location::{location_from_path, path_from_location};
use crate::native_shell::{open_with_default_application, reveal_in_finder};
use crate::operation_journal::LaunchId;
use crate::runtime::{FolderItemsSourceFactory, LocationProbe, Runtime, WorkRunner};
use crate::settings_storage::{SettingsJob, SettingsResult, SettingsWorker};

mod browser_input;
mod context_menus;
mod favorites;
mod operations;
mod shortcuts;
#[cfg(test)]
mod tests;

/// How many delivered events one drain handles, so input and painting run
/// between slices.
pub const DRAIN_SLICE: usize = 32;

/// How long one drain may spend handling events before yielding to Qt.
pub const DRAIN_TIME_BUDGET: Duration = Duration::from_millis(4);

/// How long quitting may wait for the final settings save after the Qt event
/// loop has exited.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// How long quitting may wait for cancelled file operations to clean up.
/// Anything left is swept at the next launch.
pub const OPERATION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// What the desktop needs to start its workers and persistent settings.
pub struct BrowserStartup {
    pub home: Location,
    /// Where settings persist, or `None` when no user location is known.
    pub settings_path: Option<PathBuf>,
    pub source_factory: FolderItemsSourceFactory,
    pub location_probe: LocationProbe,
    /// This launch's identity, carried by every operation temporary.
    pub launch: LaunchId,
    /// The folder or file named on the command line.
    pub target: Option<crate::launch_target::LaunchTarget>,
    /// The per-profile receiver, absent for `--new-instance`.
    pub receiver: Option<crate::instance::Receiver>,
}

/// Opens an item with its default application on the GUI thread and reports
/// whether macOS accepted the request.
pub type Opener = Box<dyn FnMut(&Location) -> bool + Send>;

/// GUI-thread coordinator for the Standard Layout. It owns one workspace and
/// one runtime while keeping the two presentation models independent.
pub struct WorkspaceSession<R = Runtime> {
    workspace: Workspace,
    controller: InputController,
    left: BrowserPresenter,
    right: BrowserPresenter,
    sidebar: WorkspacePresenter,
    operations: OperationsPresenter,
    runner: R,
    settings_worker: Option<SettingsWorker>,
    drain_slice: usize,
    drain_time_budget: Duration,
    buffered_events: VecDeque<dual_pane_application::Event>,
    runner_more_pending: bool,
    opener: Opener,
    revealer: Opener,
    /// The latest native layout request emitted by the application. A nested
    /// option distinguishes no new output from a request for default layout.
    layout_request: Option<Option<WindowLayout>>,
    activation_requests: VecDeque<Option<BrowserSide>>,
}

impl<R: WorkRunner> WorkspaceSession<R> {
    #[cfg(test)]
    pub fn new(runner: R, home: Location, drain_slice: usize, drain_time_budget: Duration) -> Self {
        Self::with_settings(runner, home, None, drain_slice, drain_time_budget)
    }

    pub fn with_settings(runner: R, home: Location, settings_worker: Option<SettingsWorker>, drain_slice: usize, drain_time_budget: Duration) -> Self {
        Self::with_presenters(runner, home, settings_worker, drain_slice, drain_time_budget, BrowserPresenter::new)
    }

    fn with_presenters(runner: R, home: Location, settings_worker: Option<SettingsWorker>, drain_slice: usize, drain_time_budget: Duration, presenter: impl Fn(BrowserSide) -> BrowserPresenter) -> Self {
        assert!(drain_slice > 0, "a drain must be able to handle at least one event");
        if let Some(worker) = &settings_worker {
            worker.submit(SettingsJob::Load);
        }
        let missing_worker = settings_worker.is_none();
        let mut session = Self { workspace: Workspace::with_home(home), controller: InputController::new(), left: presenter(BrowserSide::Left), right: presenter(BrowserSide::Right), sidebar: WorkspacePresenter::new(), operations: OperationsPresenter::new(), runner, settings_worker, drain_slice, drain_time_budget, buffered_events: VecDeque::new(), runner_more_pending: false, opener: Box::new(|item| open_with_default_application(&path_from_location(item))), revealer: Box::new(|item| reveal_in_finder(&path_from_location(item))), layout_request: None, activation_requests: VecDeque::new() };
        session.apply_chrome();
        if missing_worker {
            // Unit-test sessions intentionally use the legacy no-store event
            // so focused interaction tests can choose their own initial
            // folder. The actual desktop uses the launch fallback.
            if cfg!(test) {
                session.submit(Event::SettingsLoadFailed { failure: SettingsFailure::WorkerUnavailable });
            } else {
                session.submit(Event::SettingsLoadFailedAtLaunch { failure: SettingsFailure::WorkerUnavailable });
            }
        }
        session
    }

    /// Lets cancelled file operations clean up for a bounded time, flushes
    /// unsaved settings and waits at most `timeout` for them to reach
    /// storage. Dropping the runtime then cancels outstanding reads.
    pub fn shutdown(mut self, timeout: Duration) {
        self.runner.finish_operations(OPERATION_SHUTDOWN_TIMEOUT);
        let final_save = match self.workspace.final_settings_save() {
            Some(WorkRequest::SaveSettings { revision, snapshot }) => Some((revision, snapshot)),
            _ => None,
        };
        let final_session = match self.workspace.final_session_save() {
            Some(WorkRequest::SaveSession { revision, session }) => Some((revision, session)),
            _ => None,
        };
        if let Some(worker) = self.settings_worker {
            worker.shutdown_with_session(final_save, final_session, timeout);
        }
    }

    #[cfg(test)]
    pub fn start(&mut self, location: Location) {
        self.submit(Command::Navigate { browser: BrowserSide::Left, location: location.clone() });
        self.submit(Command::Navigate { browser: BrowserSide::Right, location });
    }

    pub fn submit(&mut self, input: impl Into<Input>) {
        self.handle_all(input.into());
    }

    /// Handles `input`, every terminal event its work returns at once, and
    /// the dismissals presentation asks for, then returns every output.
    fn handle_all(&mut self, input: Input) -> Vec<Output> {
        let mut outputs = Vec::new();
        let mut inputs = VecDeque::from([input]);
        while let Some(input) = inputs.pop_front() {
            let transition = self.workspace.handle(input);
            for output in &transition.outputs {
                match output {
                    Output::SessionRestored { layout, .. } => self.layout_request = Some(*layout),
                    Output::LayoutReset => self.layout_request = Some(None),
                    Output::ActivateWindow { focus_browser } => self.activation_requests.push_back(*focus_browser),
                    _ => {}
                }
                // Opening is a GUI-thread request; a refusal returns as an event.
                if let Output::OpenItem { item } = output
                    && !(self.opener)(item)
                {
                    inputs.push_back(Event::OpenFailed { item: item.clone() }.into());
                }
                if let Output::RevealInFinder { item } = output {
                    (self.revealer)(item);
                }
                self.left.apply(output);
                self.right.apply(output);
                self.sidebar.apply(output);
                self.operations.apply(output);
            }
            outputs.extend(transition.outputs);
            for request in transition.work {
                inputs.extend(self.dispatch(request).map(Input::from));
            }
            inputs.extend(self.operations.take_dismissals().into_iter().map(|id| Input::from(Command::DismissOperation { id })));
        }
        self.apply_chrome();
        outputs
    }

    /// Returns one pending layout request for the native shell.
    pub fn take_layout_request(&mut self) -> Option<Option<WindowLayout>> {
        self.layout_request.take()
    }

    /// Returns one request to bring the workspace forward. `Some(None)` is
    /// raise-only; a Browser asks for its Folder Items List to take focus.
    pub fn take_activation_request(&mut self) -> Option<Option<BrowserSide>> {
        self.activation_requests.pop_front()
    }

    pub fn is_quitting(&self) -> bool {
        self.workspace.is_quitting()
    }

    /// Sends settings work to the settings worker and everything else to the
    /// runtime. A request that cannot start returns its failure at once.
    fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
        let (job, failed) = match request {
            WorkRequest::SaveSettings { revision, snapshot } => (SettingsJob::Save { revision, snapshot }, Event::SettingsSaveFailed { revision, failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::SaveSession { revision, session } => (SettingsJob::SaveSession { revision, session }, Event::SessionSaveFailed { revision, failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::LoadSettings => (SettingsJob::Load, Event::SettingsLoadFailedAtLaunch { failure: SettingsFailure::WorkerUnavailable }),
            WorkRequest::ResetSettings => (SettingsJob::Reset, Event::SettingsResetFailed { failure: SettingsFailure::WorkerUnavailable }),
            request => return self.runner.dispatch(request),
        };
        if self.settings_worker.as_ref().is_some_and(|worker| worker.submit(job)) { None } else { Some(failed) }
    }

    fn apply_chrome(&mut self) {
        self.left.apply_chrome(&self.workspace.browser_chrome(BrowserSide::Left));
        self.right.apply_chrome(&self.workspace.browser_chrome(BrowserSide::Right));
        self.sidebar.apply_chrome(&self.workspace.workspace_chrome());
    }

    pub fn drain(&mut self) -> bool {
        let results = self.settings_worker.as_ref().map(SettingsWorker::take_results).unwrap_or_default();
        for result in results {
            self.submit(match result {
                SettingsResult::Loaded(loaded) => Event::SettingsLoadedWithSession { snapshot: loaded.snapshot, session: loaded.session },
                SettingsResult::LoadFailed(error) => Event::SettingsLoadFailedAtLaunch { failure: error.failure() },
                SettingsResult::Saved { revision } => Event::SettingsSaved { revision },
                SettingsResult::SaveFailed { revision, error } => Event::SettingsSaveFailed { revision, failure: error.failure() },
                SettingsResult::SessionSaved { .. } => continue,
                SettingsResult::SessionSaveFailed { revision, error } => Event::SessionSaveFailed { revision, failure: error.failure() },
                SettingsResult::Reset { backup } => Event::SettingsReset { backup: backup.as_deref().and_then(location_from_path) },
                SettingsResult::ResetFailed(error) => Event::SettingsResetFailed { failure: error.failure() },
            });
        }
        let started = Instant::now();
        if self.buffered_events.is_empty() {
            let (events, more) = self.runner.take_events(self.drain_slice);
            self.buffered_events.extend(events);
            self.runner_more_pending = more;
        }
        let mut handled = 0;
        while handled < self.drain_slice && !self.buffered_events.is_empty() {
            if handled > 0 && started.elapsed() >= self.drain_time_budget {
                break;
            }
            if let Some(event) = self.buffered_events.pop_front() {
                self.submit(event);
                handled += 1;
            }
        }
        if handled > 0 {
            self.submit(Event::ReadCapacityAvailable);
        }
        if self.buffered_events.is_empty() {
            let (events, more) = self.runner.take_events(self.drain_slice);
            self.buffered_events.extend(events);
            self.runner_more_pending = more;
        }
        !self.buffered_events.is_empty() || self.runner_more_pending
    }

    pub fn view(&self, browser: BrowserSide) -> &BrowserViewModel {
        match browser {
            BrowserSide::Left => self.left.view(),
            BrowserSide::Right => self.right.view(),
        }
    }

    pub fn workspace_view(&self) -> &WorkspaceViewModel {
        self.sidebar.view()
    }
}
