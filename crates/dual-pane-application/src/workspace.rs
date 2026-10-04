use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::operations::OperationCoordinator;
use crate::{ActionBinding, BrowserChrome, BrowserSnapshot, Command, Event, FavoriteEdit, FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoriteRejection, FavoritesRecords, Input, Notice, NoticeKind, OperationEffect, OperationJob, OperationOutcome, OperationStatus, Output, ResolvedTarget, RowChange, SessionSnapshot, SettingsFailure, SettingsState, SettingsStatus, TabSnapshot, TabSummary, WindowLayout, WorkRequest, WorkspaceChrome, WorkspaceSnapshot, fresh_profile_favorites, fresh_profile_screenshots};
use dual_pane_domain::{Activation, BrowserSide, BrowserTabs, Entry, EntryName, ListingError, ListingErrorKind, Location, OperationCommand, OperationId, OperationIntent, OperationKind, OperationRejection, OperationTarget, RequestToken, ScrollAnchor, Selection, SortSpec, TabHistory, TabId, Visit, VisitState, starts_with, valid_favorite_name};

/// The most Notices a session keeps; older ones are dropped first.
pub const NOTICE_LIMIT: usize = 200;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Transition {
    pub outputs: Vec<Output>,
    pub work: Vec<WorkRequest>,
}

impl Transition {
    fn output(output: Output) -> Self {
        Self { outputs: vec![output], work: vec![] }
    }
    fn append(&mut self, mut other: Self) {
        self.outputs.append(&mut other.outputs);
        self.work.append(&mut other.work);
    }
}

#[derive(Debug)]
pub struct Workspace {
    left: BrowserState,
    right: BrowserState,
    active_browser: BrowserSide,
    home: Location,
    settings: SettingsState,
    settings_status: SettingsStatus,
    /// False once the session learned that no settings service exists, so
    /// nothing can be reset.
    settings_worker: bool,
    /// The status before a confirmed reset, restored if the reset fails.
    reset_from: Option<SettingsStatus>,
    /// Save results for this revision or older predate the latest reset.
    stale_save_floor: Option<u64>,
    /// Sort choices made before the stored settings loaded. They are replayed
    /// over the loaded snapshot so the person's latest choice wins.
    pending_sorts: Vec<(Location, SortSpec)>,
    /// The Screenshots folder being probed before uninitialized Favorites are
    /// seeded. Favorites edits wait for the probe, as they wait for a load.
    screenshots_probe: Option<Location>,
    /// Favorite Items whose launch probe has not answered. A reset forgets
    /// them, so an earlier probe cannot prune the replacement collection.
    probing: HashSet<i64>,
    notices: Arc<[Notice]>,
    next_notice_id: u64,
    last_token: Option<RequestToken>,
    next_tab: u64,
    next_favorite_group_id: i64,
    next_favorite_item_id: i64,
    operations: OperationCoordinator,
    /// The Browser and tab each running job started from.
    operation_origins: HashMap<OperationId, (BrowserSide, TabId)>,
    /// Whether the runtime can record temporaries, which Copy needs. It
    /// counts as available until the runtime reports otherwise.
    journal_available: bool,
    /// The Item to select once a refreshed listing shows it, after a
    /// successful Rename or New Folder.
    pending_selection: Option<PendingSelection>,
    /// The latest link activation waiting for its target to resolve.
    pending_open: Option<(BrowserSide, TabId, RequestToken)>,
    /// Set once quitting was accepted; no new operation starts.
    quitting: bool,
    layout: Option<WindowLayout>,
    launch_completed: bool,
    session_write_protected: bool,
    session_revision: u64,
    startup_period: bool,
    startup_notices_opened: bool,
    startup_notice_pending: bool,
}
#[derive(Debug, Clone)]
struct PendingSelection {
    browser: BrowserSide,
    tab: TabId,
    folder: Location,
    name: EntryName,
}
/// One Browser: the domain's ordered tab identities and active tab, plus the
/// application's per-tab state, which is stored in no particular order.
#[derive(Debug)]
struct BrowserState {
    order: BrowserTabs,
    tabs: Vec<TabState>,
}
/// One tab: its domain history of visited folders, the Folder Items it shows,
/// and its outstanding read.
#[derive(Debug)]
struct TabState {
    id: TabId,
    history: TabHistory,
    folder_items: Option<FolderItems>,
    pending: Option<PendingRead>,
    error: Option<ListingError>,
    /// Latest requested folder, retained until it has been confirmed so a
    /// failed first read remains labelled, refreshable, and saveable.
    requested: Option<Location>,
    /// True until the first restoration read reaches a terminal result. A
    /// read caused by an invalidation keeps this mark; a person's navigation
    /// deliberately clears it.
    restoring: bool,
}
#[derive(Debug)]
struct FolderItems {
    location: Location,
    entries: Arc<[Entry]>,
}
#[derive(Debug)]
struct PendingRead {
    token: RequestToken,
    location: Location,
    history_target: Option<usize>,
    /// The shown Folder Items the read was asked to diff against.
    base: Option<Arc<[Entry]>>,
    restoring: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::with_home(Location::root())
    }
}
impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_home(home: Location) -> Self {
        let left_id = TabId::new(0);
        let right_id = TabId::new(1);
        Self { left: BrowserState::new(left_id), right: BrowserState::new(right_id), active_browser: BrowserSide::Left, home, settings: SettingsState::new(), settings_status: SettingsStatus::Loading, settings_worker: true, reset_from: None, stale_save_floor: None, pending_sorts: Vec::new(), screenshots_probe: None, probing: HashSet::new(), notices: Arc::from([]), next_notice_id: 1, last_token: None, next_tab: 2, next_favorite_group_id: 1, next_favorite_item_id: 1, operations: OperationCoordinator::new(), operation_origins: HashMap::new(), journal_available: true, pending_selection: None, pending_open: None, quitting: false, layout: None, launch_completed: false, session_write_protected: false, session_revision: 0, startup_period: true, startup_notices_opened: false, startup_notice_pending: false }
    }
    pub fn handle(&mut self, input: Input) -> Transition {
        let save_session = matches!(&input, Input::Command(Command::ActivateBrowser { .. } | Command::ActivateTab { .. } | Command::NewTab { .. } | Command::CloseTab { .. } | Command::ReorderTab { .. } | Command::Navigate { .. }) | Input::Event(Event::FolderItemsLoaded { .. }));
        let mut transition = match input {
            Input::Command(command) => self.command(command),
            Input::Event(event) => self.event(event),
        };
        if save_session {
            self.session_revision = self.session_revision.wrapping_add(1);
            if let Some(session) = self.session_snapshot() {
                transition.work.push(WorkRequest::SaveSession { revision: self.session_revision, session });
            }
        }
        transition.append(self.flush_startup_notice_open());
        transition
    }
    fn command(&mut self, command: Command) -> Transition {
        match command {
            Command::StartOperation { browser, tab, kind } => self.start_operation(browser, tab, kind),
            Command::RequestNameEditor { browser, tab, command } => self.request_name_editor(browser, tab, command),
            Command::RetryName { id, name } => self.retry_name(id, name),
            Command::ConfirmPermanentDelete { id, targets } => self.operation_transition(id, |operations| operations.confirm_delete(id, targets)),
            Command::DecideOperation { id, token, item, choice, apply_to_all } => self.operation_transition(id, |operations| operations.decide(id, token, &item, choice, apply_to_all)),
            Command::CancelOperation { id } => self.operation_transition(id, |operations| operations.cancel(id)),
            Command::DismissOperation { id } => self.dismiss_operation(id),
            Command::RetryJournal => Transition { outputs: vec![], work: vec![WorkRequest::ReopenJournal] },
            Command::Quit { confirmed } => self.quit(confirmed),
            Command::ActivateBrowser { browser } => self.activate_browser(browser),
            Command::ActivateTab { browser, tab } => self.activate_tab(browser, tab),
            Command::NewTab { browser } => self.new_tab(browser),
            Command::CloseTab { browser, tab } => self.close_tab(browser, tab),
            Command::ReorderTab { browser, tab, position } => self.reorder_tab(browser, tab, position),
            Command::Navigate { browser, location } => self.navigate(browser, location, None),
            Command::OpenFavoriteItem { browser, id } => self.open_favorite_item(browser, id),
            Command::SetSort { browser, tab, location, sort } => self.set_sort(browser, tab, location, sort),
            Command::CreateFavoriteGroup { name } => self.favorites_edit(FavoriteEdit::CreateGroup, |workspace| workspace.create_favorite_group(name)),
            Command::RenameFavoriteGroup { id, name } => self.favorites_edit(FavoriteEdit::Group(id), |workspace| workspace.rename_favorite_group(id, name)),
            Command::ReorderFavoriteGroup { id, position } => self.favorites_edit(FavoriteEdit::Group(id), |workspace| workspace.reorder_favorite_group(id, position)),
            Command::DeleteFavoriteGroup { id } => self.favorites_edit(FavoriteEdit::Group(id), |workspace| workspace.delete_favorite_group(id)),
            Command::CreateFavoriteItem { group_id, name, target } => self.favorites_edit(FavoriteEdit::CreateItem { group_id }, |workspace| workspace.create_favorite_item(group_id, name, target)),
            Command::RenameFavoriteItem { id, name } => self.favorites_edit(FavoriteEdit::Item(id), |workspace| workspace.rename_favorite_item(id, name)),
            Command::MoveFavoriteItem { id, group_id, position } => self.favorites_edit(FavoriteEdit::Item(id), |workspace| workspace.move_favorite_item(id, group_id, position)),
            Command::DeleteFavoriteItem { id } => self.favorites_edit(FavoriteEdit::Item(id), |workspace| workspace.delete_favorite_item(id)),
            Command::ResetSettings => self.reset_settings(),
            Command::SetHideNoticesAtStartup { hide } => self.set_hide_notices_at_startup(hide),
            Command::UpdateWindowLayout { layout } => self.update_window_layout(layout),
            Command::GoBack { browser, tab } => self.on_active(browser, tab, |workspace| workspace.travel(browser, false)),
            Command::GoForward { browser, tab } => self.on_active(browser, tab, |workspace| workspace.travel(browser, true)),
            Command::Refresh { browser, tab } => self.on_active(browser, tab, |workspace| workspace.refresh_tab(browser, tab)),
            Command::GoToParent { browser, tab } => self.on_active(browser, tab, |workspace| workspace.go_to_parent(browser)),
            Command::SelectEntry { browser, tab, row, name } | Command::MoveSelection { browser, tab, row, name } => self.on_active(browser, tab, |workspace| workspace.change_visit(browser, Some(row), |visit, entries| visit.select(entries, row, &name))),
            Command::ToggleEntry { browser, tab, row, name } => self.on_active(browser, tab, |workspace| workspace.change_visit(browser, Some(row), |visit, entries| visit.toggle(entries, row, &name))),
            Command::SelectRange { browser, tab, row, name } => self.on_active(browser, tab, |workspace| workspace.change_visit(browser, Some(row), |visit, entries| visit.select_range(entries, row, &name))),
            Command::SecondarySelect { browser, tab, row, name } => self.on_active(browser, tab, |workspace| workspace.change_visit(browser, Some(row), |visit, entries| visit.select_secondary(entries, row, &name))),
            Command::SelectAll { browser, tab } => self.on_active(browser, tab, |workspace| workspace.change_visit(browser, None, VisitState::select_all)),
            Command::ClearSelection { browser, tab } => self.on_active(browser, tab, |workspace| workspace.clear_selection(browser)),
            Command::OpenEntry { browser, tab, row, name } => self.on_active(browser, tab, |workspace| workspace.open_entry(browser, row, &name)),
            Command::ShowPackageContents { browser, tab, row, name } => self.on_active(browser, tab, |workspace| workspace.show_package_contents(browser, row, &name)),
            Command::UpdateScrollHint { browser, tab, scroll } => self.on_active(browser, tab, |workspace| workspace.scroll(browser, scroll)),
        }
    }
    fn event(&mut self, event: Event) -> Transition {
        match event {
            Event::OperationScanned { id, generation, plan } => self.operation_transition(id, |operations| operations.scanned(id, generation, plan)),
            Event::OperationStepped { id, generation, result } => self.operation_transition(id, |operations| operations.step_result(id, generation, result)),
            Event::OperationFinalized { id, generation, result } => self.operation_transition(id, |operations| operations.finalized(id, generation, result)),
            Event::OperationCleaned { id, generation, result } => self.operation_transition(id, |operations| operations.cleaned(id, generation, result)),
            Event::OperationExecutorUnavailable { id, generation } => self.operation_transition(id, |operations| operations.unavailable(id, generation)),
            Event::OperationProgress { id, generation, bytes } => self.operation_transition(id, |operations| operations.bytes_copied(id, generation, bytes)),
            Event::ItemResolved { browser, tab, token, item, target } => self.item_resolved(browser, tab, token, item, target),
            Event::OpenFailed { item } => self.add_notice(NoticeKind::OpenFailed { item }),
            Event::JournalStatus { available } => {
                self.journal_available = available;
                if available { Transition::default() } else { self.add_notice(NoticeKind::JournalUnavailable) }
            }
            Event::TemporariesSwept { removed, failed } => {
                if removed == 0 && failed.is_empty() {
                    Transition::default()
                } else {
                    self.add_notice(NoticeKind::TemporariesSwept { removed, failed })
                }
            }
            Event::LocationInvalidated { location } => self.refresh_location(&location),
            Event::FolderItemsLoaded { browser, tab, token, entries, changes } => self.loaded(browser, tab, token, entries, changes),
            Event::FolderItemsFailed { browser, tab, token, kind } => self.failed(browser, tab, token, kind),
            Event::FolderItemsCancelled { browser, tab, token } => self.cancelled(browser, tab, token),
            Event::FavoriteTargetProbed { item_id, target, outcome } => self.favorite_target_probed(item_id, target, outcome),
            Event::ScreenshotsFolderProbed { location, outcome } => self.screenshots_folder_probed(&location, outcome),
            Event::SettingsSaved { revision } => {
                if !self.is_stale_save(revision) {
                    self.settings.mark_saved(revision);
                }
                Transition::default()
            }
            Event::SettingsSaveFailed { revision, failure } => self.settings_save_failed(revision, failure),
            // Compatibility event for embedders that do not have a session
            // store. The desktop always uses the atomic session-bearing form.
            Event::SettingsLoaded { snapshot } => {
                self.launch_completed = true;
                self.settings_loaded(snapshot, SessionSnapshot::Absent)
            }
            Event::SettingsLoadedWithSession { snapshot, session } => self.settings_loaded(snapshot, session),
            Event::SettingsLoadFailed { failure } => self.settings_load_failed_legacy(failure),
            Event::SettingsLoadFailedAtLaunch { failure } => self.settings_load_failed(failure),
            Event::SettingsLoadTimedOut => self.settings_load_timed_out(),
            Event::StartupPeriodElapsed => {
                self.startup_period = false;
                Transition::default()
            }
            Event::SettingsReset { backup } => self.settings_reset(backup),
            Event::SettingsResetFailed { failure } => self.settings_reset_failed(failure),
        }
    }
    pub fn active_browser(&self) -> BrowserSide {
        self.active_browser
    }
    pub fn active_tab(&self, side: BrowserSide) -> TabId {
        self.browser(side).order.active()
    }
    /// The Browser's tabs in display order.
    pub fn tabs(&self, side: BrowserSide) -> impl ExactSizeIterator<Item = TabId> + '_ {
        self.browser(side).order.tabs().iter().copied()
    }
    /// Whether Back would move the active tab, counting a Back or Forward that
    /// is still loading as already taken.
    pub fn can_go_back(&self, side: BrowserSide) -> bool {
        self.history_step(side, false).is_some()
    }
    /// Whether Forward would move the active tab; see [`Self::can_go_back`].
    pub fn can_go_forward(&self, side: BrowserSide) -> bool {
        self.history_step(side, true).is_some()
    }
    pub fn location(&self, side: BrowserSide) -> Option<&Location> {
        self.tab(side, self.active_tab(side)).and_then(|t| t.folder_items.as_ref().map(|x| &x.location))
    }
    pub fn entries(&self, side: BrowserSide) -> &[Entry] {
        self.tab(side, self.active_tab(side)).and_then(|t| t.folder_items.as_ref()).map_or(&[], |x| &x.entries)
    }
    pub fn loading_location(&self, side: BrowserSide) -> Option<&Location> {
        self.tab(side, self.active_tab(side)).and_then(|t| t.pending.as_ref().map(|x| &x.location))
    }
    pub fn selection(&self, side: BrowserSide) -> &Selection {
        static NONE: Selection = Selection::new();
        self.tab(side, self.active_tab(side)).and_then(|tab| tab.history.current()).map_or(&NONE, |visit| visit.state().selection())
    }
    pub fn operation_jobs(&self) -> &[OperationJob] {
        self.operations.jobs()
    }
    /// The same validation drives command enablement and command acceptance.
    pub fn operation_availability(&self, browser: BrowserSide, tab: TabId, kind: OperationKind) -> Result<OperationIntent, OperationRejection> {
        let (source, targets, destination) = self.capture(browser, tab, kind.command())?;
        let intent = OperationIntent::new(kind, source, targets, destination)?;
        self.journal_check(intent.kind().command())?;
        Ok(intent)
    }
    /// Whether a file command could start from `tab` before any name is
    /// typed, for menu enablement. It applies every check of
    /// [`Self::operation_availability`] except the name's.
    pub fn command_availability(&self, browser: BrowserSide, tab: TabId, command: OperationCommand) -> Result<(), OperationRejection> {
        let (source, targets, destination) = self.capture(browser, tab, command)?;
        OperationIntent::check(command, &source, &targets, destination.as_ref())?;
        self.journal_check(command)
    }
    fn journal_check(&self, command: OperationCommand) -> Result<(), OperationRejection> {
        if command == OperationCommand::Copy && !self.journal_available { Err(OperationRejection::JournalUnavailable) } else { Ok(()) }
    }
    /// The confirmed source folder, the selected targets in listing order,
    /// and for Copy and Move the other Browser's confirmed folder.
    fn capture(&self, browser: BrowserSide, tab: TabId, command: OperationCommand) -> Result<(Location, Vec<OperationTarget>, Option<Location>), OperationRejection> {
        if self.active_tab(browser) != tab {
            return Err(OperationRejection::StaleTab);
        }
        let source_tab = self.tab(browser, tab).ok_or(OperationRejection::StaleTab)?;
        if source_tab.pending.is_some() || source_tab.error.is_some() {
            return Err(OperationRejection::SourceUnavailable);
        }
        let items = source_tab.folder_items.as_ref().ok_or(OperationRejection::SourceUnavailable)?;
        let selected = source_tab.history.current().map(|visit| visit.state().selection().entries()).unwrap_or(&[]);
        let targets = if command == OperationCommand::NewFolder {
            vec![]
        } else {
            let selected_names: HashSet<_> = selected.iter().collect();
            if selected_names.len() != selected.len() {
                return Err(OperationRejection::StaleSelection);
            }
            let targets = items.entries.iter().filter(|entry| selected_names.contains(entry.name())).map(|entry| OperationTarget { name: entry.name().clone(), kind: entry.kind() }).collect::<Vec<_>>();
            if targets.len() != selected.len() {
                return Err(OperationRejection::StaleSelection);
            }
            targets
        };
        let destination = if matches!(command, OperationCommand::Copy | OperationCommand::Move) {
            let other = match browser {
                BrowserSide::Left => BrowserSide::Right,
                BrowserSide::Right => BrowserSide::Left,
            };
            let tab = self.tab(other, self.active_tab(other)).ok_or(OperationRejection::DestinationUnavailable)?;
            if tab.pending.is_some() || tab.error.is_some() {
                return Err(OperationRejection::DestinationUnavailable);
            }
            Some(tab.folder_items.as_ref().ok_or(OperationRejection::DestinationUnavailable)?.location.clone())
        } else {
            None
        };
        Ok((items.location.clone(), targets, destination))
    }
    fn start_operation(&mut self, browser: BrowserSide, tab: TabId, kind: OperationKind) -> Transition {
        if self.quitting {
            return Transition::default();
        }
        let command = kind.command();
        match self.operation_availability(browser, tab, kind) {
            Ok(intent) => {
                let (id, effects) = self.operations.start(intent);
                self.operation_origins.insert(id, (browser, tab));
                self.operation_output(id, effects)
            }
            Err(reason) => Transition::output(Output::OperationRejected { browser, tab, command, reason }),
        }
    }
    fn request_name_editor(&self, browser: BrowserSide, tab: TabId, command: OperationCommand) -> Transition {
        if !matches!(command, OperationCommand::Rename | OperationCommand::NewFolder) {
            return Transition::default();
        }
        match self.capture(browser, tab, command).and_then(|(source, targets, destination)| OperationIntent::check(command, &source, &targets, destination.as_ref()).map(|()| targets)) {
            Ok(targets) => Transition::output(Output::NameEditorOpened { browser, tab, command, target: targets.into_iter().next().map(|target| target.name) }),
            Err(reason) => Transition::output(Output::OperationRejected { browser, tab, command, reason }),
        }
    }
    fn retry_name(&mut self, id: OperationId, name: EntryName) -> Transition {
        let Some(job) = self.operations.job(id).filter(|job| job.name_problem().is_some()) else { return Transition::default() };
        let intent = job.intent().clone();
        let kind = match intent.kind() {
            OperationKind::Rename { .. } => OperationKind::Rename { to: name },
            OperationKind::NewFolder { .. } => OperationKind::NewFolder { name },
            _ => return Transition::default(),
        };
        let command = kind.command();
        let origin = self.operation_origins.get(&id).copied();
        match OperationIntent::new(kind, intent.source().clone(), intent.targets().to_vec(), intent.destination().cloned()) {
            Err(reason) => origin.map_or_else(Transition::default, |(browser, tab)| Transition::output(Output::OperationRejected { browser, tab, command, reason })),
            Ok(retry) => {
                let mut transition = self.operation_transition(id, |operations| operations.cancel(id));
                let (new_id, effects) = self.operations.start(retry);
                if let Some(origin) = origin {
                    self.operation_origins.insert(new_id, origin);
                }
                transition.append(self.operation_output(new_id, effects));
                transition
            }
        }
    }
    /// Applies one operation change. A job that just reached its terminal
    /// outcome also refreshes affected tabs and records its Notice.
    fn operation_transition(&mut self, id: OperationId, change: impl FnOnce(&mut OperationCoordinator) -> Option<Vec<OperationEffect>>) -> Transition {
        let before = self.operations.job(id).map(|job| job.status().clone());
        let Some(effects) = change(&mut self.operations) else { return Transition::default() };
        let mut transition = self.operation_output(id, effects);
        if let Some(before) = before
            && !matches!(before, OperationStatus::Finished(_))
            && let Some(OperationStatus::Finished(outcome)) = self.operations.job(id).map(|job| job.status().clone())
        {
            transition.append(self.operation_finished(id, &before, outcome));
        }
        transition.append(self.flush_startup_notice_open());
        transition
    }
    fn operation_output(&self, id: OperationId, effects: Vec<OperationEffect>) -> Transition {
        let Some(job) = self.operations.job(id) else { return Transition::default() };
        Transition { outputs: vec![Output::OperationChanged { job: Box::new(job.clone()) }], work: effects.into_iter().map(WorkRequest::Operation).collect() }
    }
    fn operation_finished(&mut self, id: OperationId, before: &OperationStatus, outcome: OperationOutcome) -> Transition {
        let origin = self.operation_origins.remove(&id);
        // A pending confirmation or a refused name closes without having
        // changed anything, so it leaves no summary.
        if matches!(before, OperationStatus::AwaitingConfirmation { .. } | OperationStatus::NameCollision { .. } | OperationStatus::NameRejected { .. }) {
            return Transition::default();
        }
        let Some(job) = self.operations.job(id).cloned() else { return Transition::default() };
        let mut transition = Transition::default();
        if outcome == OperationOutcome::Succeeded
            && let OperationKind::Rename { to: name } | OperationKind::NewFolder { name } = job.intent().kind()
            && let Some((browser, tab)) = origin
            && self.tab(browser, tab).and_then(|state| state.pending.as_ref().map(|pending| &pending.location).or(state.folder_items.as_ref().map(|items| &items.location))) == Some(job.intent().source())
        {
            self.pending_selection = Some(PendingSelection { browser, tab, folder: job.intent().source().clone(), name: name.clone() });
        }
        // Only completed entries change listings; this launch's temporaries,
        // even ones cleanup could not remove, are never listed.
        if job.progress().completed > 0 {
            transition.append(self.refresh_affected(job.intent()));
        }
        transition.append(self.add_notice(NoticeKind::OperationFinished { intent: job.intent().clone(), outcome, progress: job.progress(), failure: job.failure().cloned() }));
        transition
    }
    /// Rereads every open tab, in either Browser, whose location is the
    /// operation's source or destination folder or lies at or below one of
    /// its source or destination roots.
    fn refresh_affected(&mut self, intent: &OperationIntent) -> Transition {
        let source = intent.source();
        let mut folders = vec![source.clone()];
        folders.extend(intent.destination().cloned());
        let mut roots = match intent.kind() {
            OperationKind::NewFolder { name } => vec![source.join(name)],
            OperationKind::Rename { to } => vec![source.join(to)],
            _ => vec![],
        };
        for target in intent.targets() {
            roots.push(source.join(&target.name));
            roots.extend(intent.destination().map(|destination| destination.join(&target.name)));
        }
        let affected = |location: &Location| folders.contains(location) || roots.iter().any(|root| starts_with(location, root));
        let targets = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|side| self.browser(side).tabs.iter().filter(|tab| tab.pending.as_ref().map(|pending| &pending.location).or(tab.folder_items.as_ref().map(|items| &items.location)).is_some_and(affected)).map(move |tab| (side, tab.id))).collect::<Vec<_>>();
        targets.into_iter().fold(Transition::default(), |mut total, (side, tab)| {
            total.append(self.refresh_tab(side, tab));
            total
        })
    }
    /// Removes a finished job and lets the runtime release what it kept.
    fn dismiss_operation(&mut self, id: OperationId) -> Transition {
        if !self.operations.dismiss(id) {
            return Transition::default();
        }
        self.operation_origins.remove(&id);
        Transition { outputs: vec![Output::OperationDismissed { id }], work: vec![WorkRequest::Operation(OperationEffect::Release { id })] }
    }
    /// Asks for confirmation while operations are active; otherwise, or once
    /// confirmed, cancels every unfinished job and accepts the quit.
    fn quit(&mut self, confirmed: bool) -> Transition {
        let running = self.operations.jobs().iter().filter(|job| job.is_active()).count();
        if running > 0 && !confirmed {
            return Transition::output(Output::QuitConfirmationRequired { running });
        }
        self.quitting = true;
        let unfinished = self.operations.jobs().iter().filter(|job| !matches!(job.status(), OperationStatus::Finished(_))).map(OperationJob::id).collect::<Vec<_>>();
        let mut transition = unfinished.into_iter().fold(Transition::default(), |mut total, id| {
            total.append(self.operation_transition(id, |operations| operations.cancel(id)));
            total
        });
        transition.outputs.push(Output::QuitAccepted);
        transition
    }
    pub fn favorites(&self) -> &FavoritesRecords {
        self.settings.favorites()
    }
    /// Whether Favorites are known, so an edit can be accepted.
    pub fn favorites_ready(&self) -> bool {
        self.settings_status != SettingsStatus::Loading && self.screenshots_probe.is_none()
    }
    pub fn settings_status(&self) -> SettingsStatus {
        self.settings_status
    }
    /// The effective shortcut of every catalogued action.
    pub fn bindings(&self) -> Vec<ActionBinding> {
        self.settings.bindings()
    }
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }
    /// The Browser's tab strip, toolbar availability, and effective sort.
    pub fn browser_chrome(&self, side: BrowserSide) -> BrowserChrome {
        let state = self.browser(side);
        let tabs = state.order.tabs().iter().filter_map(|id| self.tab(side, *id)).map(|tab| TabSummary { id: tab.id, location: tab.folder_items.as_ref().map(|items| &items.location).or_else(|| tab.pending.as_ref().map(|pending| &pending.location)).cloned(), loading: tab.pending.is_some(), failed: tab.error.is_some() }).collect();
        let location = self.location(side).cloned();
        BrowserChrome { tabs, active_tab: state.order.active(), can_open_tab: state.order.can_insert() && location.is_some(), can_go_back: self.can_go_back(side), can_go_forward: self.can_go_forward(side), can_go_up: location.as_ref().is_some_and(|location| !location.is_root()), can_refresh: location.is_some() || self.loading_location(side).is_some(), sort: location.as_ref().map(|location| self.settings.peek_sort(location)), location }
    }
    /// Workspace-wide presentation state.
    pub fn workspace_chrome(&self) -> WorkspaceChrome {
        WorkspaceChrome { active_browser: self.active_browser, favorites: self.settings.favorites().clone(), favorites_ready: self.favorites_ready(), bindings: self.bindings(), hide_notices_at_startup: self.settings.hide_notices_at_startup(), notices_startup_ready: self.settings_status != SettingsStatus::Loading, notices: Arc::clone(&self.notices) }
    }
    /// The save to flush before the application exits, if loaded settings
    /// have changed since the last confirmed save.
    pub fn final_settings_save(&self) -> Option<WorkRequest> {
        self.settings.has_unsaved_changes().then(|| self.save_settings()).flatten()
    }
    /// The newest independent session snapshot for the orderly-quit flush.
    pub fn final_session_save(&self) -> Option<WorkRequest> {
        self.session_snapshot().map(|session| WorkRequest::SaveSession { revision: self.session_revision, session })
    }
    fn browser(&self, side: BrowserSide) -> &BrowserState {
        match side {
            BrowserSide::Left => &self.left,
            BrowserSide::Right => &self.right,
        }
    }
    fn browser_mut(&mut self, side: BrowserSide) -> &mut BrowserState {
        match side {
            BrowserSide::Left => &mut self.left,
            BrowserSide::Right => &mut self.right,
        }
    }
    fn tab(&self, side: BrowserSide, id: TabId) -> Option<&TabState> {
        self.browser(side).tabs.iter().find(|t| t.id == id)
    }
    fn tab_mut(&mut self, side: BrowserSide, id: TabId) -> Option<&mut TabState> {
        self.browser_mut(side).tabs.iter_mut().find(|t| t.id == id)
    }
    /// Runs a gesture only while `tab` is still the Browser's active tab, so
    /// a gesture observed before a tab switch cannot act on another tab.
    fn on_active(&mut self, browser: BrowserSide, tab: TabId, gesture: impl FnOnce(&mut Self) -> Transition) -> Transition {
        if self.active_tab(browser) == tab { gesture(self) } else { Transition::default() }
    }
    fn activate_browser(&mut self, browser: BrowserSide) -> Transition {
        if self.active_browser == browser {
            Transition::default()
        } else {
            self.active_browser = browser;
            Transition::output(Output::ActiveBrowserChanged { browser })
        }
    }
    fn activate_tab(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        if !self.browser_mut(browser).order.activate(tab) {
            return Transition::default();
        }
        let mut outputs = vec![Output::ActiveTabChanged { browser, tab }];
        if self.active_browser != browser {
            self.active_browser = browser;
            outputs.insert(0, Output::ActiveBrowserChanged { browser });
        }
        outputs.push(self.tab_view(browser, tab));
        Transition { outputs, work: vec![] }
    }
    fn new_tab(&mut self, browser: BrowserSide) -> Transition {
        let Some(location) = self.location(browser).cloned() else {
            return Transition::default();
        };
        let id = TabId::new(self.next_tab);
        let state = self.browser_mut(browser);
        if !state.order.insert_active(id) {
            return Transition::default();
        }
        state.tabs.push(TabState::new(id));
        self.next_tab += 1;
        let mut transition = self.navigate(browser, location, None);
        transition.outputs.insert(0, Output::TabsChanged { browser, active_tab: id });
        transition.outputs.insert(1, self.tab_view(browser, id));
        transition
    }
    fn close_tab(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        let Some(pending) = self.tab(browser, tab).map(|state| state.pending.as_ref().map(|p| p.token)) else {
            return Transition::default();
        };
        let was_active = self.active_tab(browser) == tab;
        let final_tab = self.browser(browser).order.tabs().len() == 1;
        if self.pending_selection.as_ref().is_some_and(|pending| pending.browser == browser && pending.tab == tab) {
            self.pending_selection = None;
        }
        let replacement = if final_tab {
            let id = TabId::new(self.next_tab);
            self.next_tab += 1;
            let state = self.browser_mut(browser);
            state.order.replace_final(tab, id);
            state.tabs = vec![TabState::new(id)];
            id
        } else {
            let state = self.browser_mut(browser);
            state.tabs.retain(|item| item.id != tab);
            match state.order.close(tab) {
                Some(active) => active,
                None => return Transition::default(),
            }
        };
        let mut transition = Transition { outputs: vec![Output::TabsChanged { browser, active_tab: replacement }], work: pending.into_iter().map(|token| WorkRequest::Cancel { browser, tab, token }).collect() };
        if was_active {
            transition.outputs.push(self.tab_view(browser, replacement));
        }
        if final_tab {
            transition.append(self.navigate(browser, self.home.clone(), None));
        }
        transition
    }
    fn reorder_tab(&mut self, browser: BrowserSide, tab: TabId, position: usize) -> Transition {
        let state = self.browser_mut(browser);
        if !state.order.reorder(tab, position) {
            return Transition::default();
        }
        Transition::output(Output::TabsChanged { browser, active_tab: state.order.active() })
    }
    fn navigate(&mut self, browser: BrowserSide, location: Location, history_target: Option<usize>) -> Transition {
        let tab = self.active_tab(browser);
        if self.tab(browser, tab).and_then(|t| t.pending.as_ref()).is_some_and(|p| p.location == location && p.history_target == history_target) {
            return Transition::default();
        }
        if let Some(tab) = self.tab_mut(browser, tab) {
            tab.restoring = false;
        }
        self.start_read(browser, tab, location, history_target)
    }
    fn start_read(&mut self, browser: BrowserSide, tab: TabId, location: Location, history_target: Option<usize>) -> Transition {
        // Navigating elsewhere forgets an Item still waiting to be selected.
        if self.pending_selection.as_ref().is_some_and(|pending| pending.browser == browser && pending.tab == tab && pending.folder != location) {
            self.pending_selection = None;
        }
        let token = self.next_token();
        let sort = self.settings.sort_for(&location);
        let previous = self.tab(browser, tab).and_then(|state| state.folder_items.as_ref()).filter(|items| items.location == location).map(|items| Arc::clone(&items.entries));
        let old = self.tab_mut(browser, tab).and_then(|state| {
            state.error = None;
            state.requested = Some(location.clone());
            state.pending.replace(PendingRead { token, location: location.clone(), history_target, base: previous.clone(), restoring: state.restoring })
        });
        let mut work = old.into_iter().map(|old| WorkRequest::Cancel { browser, tab, token: old.token }).collect::<Vec<_>>();
        work.push(WorkRequest::ReadDirectory { browser, tab, token, location: location.clone(), sort, previous });
        Transition { outputs: vec![Output::LoadingStarted { browser, tab, location }], work }
    }
    fn next_token(&mut self) -> RequestToken {
        let token = self.last_token.map_or_else(RequestToken::first, RequestToken::next);
        self.last_token = Some(token);
        token
    }
    /// The history entry one step back or forward from the active tab. A Back
    /// or Forward that is still loading counts as taken, so repeated presses
    /// keep moving instead of re-requesting the same entry.
    fn history_step(&self, browser: BrowserSide, forward: bool) -> Option<(usize, Location)> {
        let tab = self.tab(browser, self.active_tab(browser))?;
        let base = tab.pending.as_ref().and_then(|pending| pending.history_target).or(tab.history.current_index())?;
        let target = tab.history.neighbor(base, forward)?;
        tab.history.get(target).map(|visit| (target, visit.location().clone()))
    }
    fn travel(&mut self, browser: BrowserSide, forward: bool) -> Transition {
        let tab = self.active_tab(browser);
        self.history_step(browser, forward).map_or_else(Transition::default, |(target, location)| self.start_read(browser, tab, location, Some(target)))
    }
    fn refresh_tab(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        let target = self.tab(browser, tab).and_then(|state| state.pending.as_ref().map(|pending| (pending.location.clone(), pending.history_target)).or_else(|| state.folder_items.as_ref().map(|items| (items.location.clone(), None)).or_else(|| state.requested.as_ref().map(|location| (location.clone(), None)))));
        target.map_or_else(Transition::default, |(location, history_target)| self.start_read(browser, tab, location, history_target))
    }
    fn refresh_location(&mut self, location: &Location) -> Transition {
        let targets = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|side| self.browser(side).tabs.iter().filter(move |tab| tab.pending.as_ref().map(|pending| &pending.location).or_else(|| tab.folder_items.as_ref().map(|items| &items.location)) == Some(location)).map(move |tab| (side, tab.id))).collect::<Vec<_>>();
        targets.into_iter().fold(Transition::default(), |mut total, (side, tab)| {
            total.append(self.refresh_tab(side, tab));
            total
        })
    }
    /// Applies a sort chosen for `tab`'s confirmed `location`. A sort click
    /// delivered after that tab moved elsewhere or closed changes nothing.
    fn set_sort(&mut self, browser: BrowserSide, tab: TabId, location: Location, sort: SortSpec) -> Transition {
        if self.tab(browser, tab).and_then(|state| state.folder_items.as_ref()).is_none_or(|items| items.location != location) {
            return Transition::default();
        }
        self.settings.set_sort(location.clone(), sort);
        if self.settings_status == SettingsStatus::Loading {
            self.pending_sorts.push((location.clone(), sort));
        }
        let mut transition = self.refresh_location(&location);
        transition.work.extend(self.save_settings());
        transition
    }
    fn settings_loaded(&mut self, snapshot: crate::SettingsSnapshot, session: SessionSnapshot) -> Transition {
        if self.settings_status != SettingsStatus::Loading {
            return Transition::default();
        }
        self.settings.apply(snapshot);
        // The loaded snapshot is what storage already holds.
        self.settings.mark_saved(self.settings.revision());
        self.settings_status = SettingsStatus::Loaded;
        let replayed = !self.pending_sorts.is_empty();
        for (location, sort) in std::mem::take(&mut self.pending_sorts) {
            self.settings.set_sort(location, sort);
        }
        // A fresh profile is seeded once its optional Screenshots folder has
        // been probed off the GUI thread; stored Favorites are probed now.
        self.screenshots_probe = (!self.settings.favorites().initialized).then(|| fresh_profile_screenshots(&self.home));
        self.reserve_favorite_ids();
        let mut transition = Transition { outputs: vec![Output::FavoritesChanged { favorites: self.settings.favorites().clone() }], work: if replayed { self.save_settings().into_iter().collect() } else { Vec::new() } };
        transition.work.extend(self.screenshots_probe.clone().map(|location| WorkRequest::ProbeScreenshotsFolder { location }));
        if self.screenshots_probe.is_none() {
            transition.work.extend(self.launch_probes());
        }
        if !self.launch_completed {
            self.launch_completed = true;
            match session {
                SessionSnapshot::Saved(saved) => transition.append(self.restore_session(saved)),
                SessionSnapshot::Damaged => {
                    transition.append(self.fallback_home());
                    transition.append(self.add_notice(NoticeKind::SessionNotRestored));
                }
                SessionSnapshot::Absent => transition.append(self.fallback_home()),
            }
            transition.outputs.push(Output::SessionRestored { active_browser: self.active_browser, layout: self.layout });
            // Presentation can report its initial layout while settings are
            // still loading. The completed launch makes that already-known
            // layout eligible for the first independent session save.
            if self.layout.is_some() {
                self.session_revision = self.session_revision.wrapping_add(1);
                if let Some(session) = self.session_snapshot() {
                    transition.work.push(WorkRequest::SaveSession { revision: self.session_revision, session });
                }
            }
        } else {
            let tabs = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|side| self.browser(side).tabs.iter().map(move |tab| (side, tab.id))).collect::<Vec<_>>();
            for (side, tab) in tabs {
                transition.append(self.refresh_tab(side, tab));
            }
        }
        transition
    }
    /// Seeds the fresh-profile Favorites once the matching Screenshots probe
    /// completes. Only a probe that found the folder adds Screenshots.
    fn screenshots_folder_probed(&mut self, location: &Location, outcome: FavoriteProbeOutcome) -> Transition {
        if self.screenshots_probe.as_ref() != Some(location) {
            return Transition::default();
        }
        self.screenshots_probe = None;
        if self.settings_status != SettingsStatus::Loaded || self.settings.favorites().initialized {
            return Transition::default();
        }
        self.settings.replace_favorites(fresh_profile_favorites(&self.home, outcome == FavoriteProbeOutcome::Available));
        self.reserve_favorite_ids();
        let mut work = self.save_settings().into_iter().collect::<Vec<_>>();
        work.extend(self.launch_probes());
        Transition { outputs: vec![Output::FavoritesChanged { favorites: self.settings.favorites().clone() }], work }
    }
    /// Probes every Favorite Item's target once Favorites are known.
    fn launch_probes(&mut self) -> Vec<WorkRequest> {
        let items = self.settings.favorites().items.iter().map(|item| (item.id, item.target.clone())).collect::<Vec<_>>();
        self.probing = items.iter().map(|(id, _)| *id).collect();
        items.into_iter().map(|(item_id, target)| WorkRequest::ProbeFavoriteTarget { item_id, target }).collect()
    }
    /// Keeps new Favorite IDs above every ID in the current collection.
    fn reserve_favorite_ids(&mut self) {
        self.next_favorite_group_id = self.next_favorite_group_id.max(self.settings.favorites().groups.iter().map(|group| group.id.saturating_add(1)).max().unwrap_or(1));
        self.next_favorite_item_id = self.next_favorite_item_id.max(self.settings.favorites().items.iter().map(|item| item.id.saturating_add(1)).max().unwrap_or(1));
    }
    fn settings_load_failed(&mut self, failure: SettingsFailure) -> Transition {
        if self.settings_status != SettingsStatus::Loading {
            return Transition::default();
        }
        self.settings_status = SettingsStatus::LoadFailed(failure);
        if failure == SettingsFailure::WorkerUnavailable {
            self.settings_worker = false;
        }
        self.screenshots_probe = None;
        self.pending_sorts.clear();
        let mut transition = Transition::output(Output::SettingsLoadFailed { failure });
        transition.append(self.add_notice(NoticeKind::SettingsLoadFailed { failure }));
        if !self.launch_completed {
            self.launch_completed = true;
            transition.append(self.fallback_home());
            transition.outputs.push(Output::SessionRestored { active_browser: self.active_browser, layout: None });
        }
        transition
    }
    fn settings_load_failed_legacy(&mut self, failure: SettingsFailure) -> Transition {
        if self.settings_status != SettingsStatus::Loading {
            return Transition::default();
        }
        self.settings_status = SettingsStatus::LoadFailed(failure);
        if failure == SettingsFailure::WorkerUnavailable {
            self.settings_worker = false
        }
        self.screenshots_probe = None;
        self.pending_sorts.clear();
        let mut transition = Transition::output(Output::SettingsLoadFailed { failure });
        transition.append(self.add_notice(NoticeKind::SettingsLoadFailed { failure }));
        transition
    }
    fn settings_load_timed_out(&mut self) -> Transition {
        if self.launch_completed {
            return Transition::default();
        }
        self.launch_completed = true;
        self.session_write_protected = true;
        let mut transition = self.fallback_home();
        transition.append(self.add_notice(NoticeKind::SettingsLoadTimedOut));
        transition.outputs.push(Output::SessionRestored { active_browser: self.active_browser, layout: None });
        transition
    }
    fn fallback_home(&mut self) -> Transition {
        let targets = [BrowserSide::Left, BrowserSide::Right]
            .into_iter()
            .filter_map(|side| {
                let tab = self.active_tab(side);
                self.tab(side, tab).is_some_and(|tab| tab.folder_items.is_none() && tab.pending.is_none()).then_some((side, tab))
            })
            .collect::<Vec<_>>();
        targets.into_iter().fold(Transition::default(), |mut total, (side, tab)| {
            total.append(self.start_read(side, tab, self.home.clone(), None));
            total
        })
    }
    fn restore_session(&mut self, snapshot: WorkspaceSnapshot) -> Transition {
        if !snapshot.layout.valid() || !self.placeholder(BrowserSide::Left) || !self.placeholder(BrowserSide::Right) {
            return self.fallback_home();
        }
        let Some(left) = self.restored_browser(&snapshot.left) else { return self.fallback_home() };
        let Some(right) = self.restored_browser(&snapshot.right) else { return self.fallback_home() };
        self.left = left;
        self.right = right;
        self.active_browser = snapshot.active_browser;
        self.layout = Some(snapshot.layout);
        let ordered = self.restore_read_order();
        let mut transition = Transition { outputs: vec![Output::TabsChanged { browser: BrowserSide::Left, active_tab: self.left.order.active() }, Output::TabsChanged { browser: BrowserSide::Right, active_tab: self.right.order.active() }, Output::ActiveBrowserChanged { browser: self.active_browser }], work: vec![] };
        for (side, tab) in ordered {
            let location = self.tab(side, tab).and_then(TabState::requested_location).expect("restored tabs have locations");
            transition.append(self.start_read(side, tab, location, None));
        }
        transition
    }
    fn placeholder(&self, side: BrowserSide) -> bool {
        let state = self.browser(side);
        state.tabs.len() == 1 && state.tabs[0].folder_items.is_none() && state.tabs[0].pending.is_none()
    }
    fn restored_browser(&mut self, saved: &BrowserSnapshot) -> Option<BrowserState> {
        let active = saved.active_tab?;
        let ids = (0..saved.tabs.len())
            .map(|_| {
                let id = TabId::new(self.next_tab);
                self.next_tab += 1;
                id
            })
            .collect::<Vec<_>>();
        let order = BrowserTabs::from_ordered(ids.clone(), active)?;
        let tabs = ids.into_iter().zip(saved.tabs.iter()).map(|(id, tab)| TabState::requested(id, tab.location.clone())).collect();
        Some(BrowserState { order, tabs })
    }
    fn restore_read_order(&self) -> Vec<(BrowserSide, TabId)> {
        let other = match self.active_browser {
            BrowserSide::Left => BrowserSide::Right,
            BrowserSide::Right => BrowserSide::Left,
        };
        let mut result = vec![(self.active_browser, self.active_tab(self.active_browser)), (other, self.active_tab(other))];
        for side in [BrowserSide::Left, BrowserSide::Right] {
            let already = result.clone();
            result.extend(self.browser(side).order.tabs().iter().copied().filter(|tab| !already.contains(&(side, *tab))).map(|tab| (side, tab)));
        }
        result
    }
    fn set_hide_notices_at_startup(&mut self, hide: bool) -> Transition {
        if self.settings_status != SettingsStatus::Loaded {
            return Transition::default();
        }
        self.settings.set_hide_notices_at_startup(hide);
        Transition { outputs: vec![], work: self.save_settings().into_iter().collect() }
    }
    fn update_window_layout(&mut self, layout: WindowLayout) -> Transition {
        if !layout.valid() {
            return Transition::default();
        }
        self.layout = Some(layout);
        self.session_revision += 1;
        Transition { outputs: vec![], work: self.session_snapshot().map(|session| WorkRequest::SaveSession { revision: self.session_revision, session }).into_iter().collect() }
    }
    pub fn session_snapshot(&self) -> Option<WorkspaceSnapshot> {
        if self.settings_status != SettingsStatus::Loaded || !self.launch_completed || self.session_write_protected {
            return None;
        }
        let layout = self.layout?;
        Some(WorkspaceSnapshot { left: self.browser_snapshot(BrowserSide::Left)?, right: self.browser_snapshot(BrowserSide::Right)?, active_browser: self.active_browser, layout })
    }
    fn browser_snapshot(&self, side: BrowserSide) -> Option<BrowserSnapshot> {
        let state = self.browser(side);
        let tabs = state.order.tabs().iter().map(|id| self.tab(side, *id)?.requested_location().map(|location| TabSnapshot { location })).collect::<Option<Vec<_>>>()?;
        let active_tab = state.order.tabs().iter().position(|id| *id == state.order.active());
        Some(BrowserSnapshot { tabs, active_tab })
    }
    fn is_stale_save(&self, revision: u64) -> bool {
        self.stale_save_floor.is_some_and(|floor| revision <= floor)
    }
    fn settings_save_failed(&mut self, revision: u64, failure: SettingsFailure) -> Transition {
        if self.is_stale_save(revision) {
            return Transition::default();
        }
        let mut transition = Transition::output(Output::SettingsSaveFailed { revision, failure });
        transition.append(self.add_notice(NoticeKind::SettingsSaveFailed { failure }));
        transition
    }
    /// Starts a confirmed reset. Favorites and settings are unknown until the
    /// replacement loads, so nothing is saved or probed meanwhile.
    fn reset_settings(&mut self) -> Transition {
        // A load in progress, including the reload after a reset, must land
        // before another reset could be ordered after it.
        if !self.settings_worker || self.reset_from.is_some() || self.settings_status == SettingsStatus::Loading {
            return Transition::default();
        }
        self.reset_from = Some(self.settings_status);
        self.settings_status = SettingsStatus::Loading;
        self.stale_save_floor = Some(self.settings.revision());
        self.screenshots_probe = None;
        self.probing.clear();
        self.pending_sorts.clear();
        Transition { outputs: vec![], work: vec![WorkRequest::ResetSettings] }
    }
    fn settings_reset(&mut self, backup: Option<Location>) -> Transition {
        if self.reset_from.take().is_none() {
            return Transition::default();
        }
        let mut transition = self.add_notice(NoticeKind::SettingsReset { backup });
        transition.append(self.reset_workspace());
        transition.work.push(WorkRequest::LoadSettings);
        transition
    }
    fn reset_workspace(&mut self) -> Transition {
        let cancelled = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|browser| self.browser(browser).tabs.iter().filter_map(move |tab| tab.pending.as_ref().map(|pending| WorkRequest::Cancel { browser, tab: tab.id, token: pending.token }))).collect::<Vec<_>>();
        let left = TabId::new(self.next_tab);
        self.next_tab += 1;
        let right = TabId::new(self.next_tab);
        self.next_tab += 1;
        self.left = BrowserState::new(left);
        self.right = BrowserState::new(right);
        self.active_browser = BrowserSide::Left;
        self.pending_selection = None;
        self.pending_open = None;
        self.layout = None;
        self.session_write_protected = false;
        let mut transition = Transition { outputs: vec![Output::TabsChanged { browser: BrowserSide::Left, active_tab: left }, Output::TabsChanged { browser: BrowserSide::Right, active_tab: right }, Output::ActiveBrowserChanged { browser: BrowserSide::Left }, Output::LayoutReset], work: cancelled };
        transition.append(self.start_read(BrowserSide::Left, left, self.home.clone(), None));
        transition.append(self.start_read(BrowserSide::Right, right, self.home.clone(), None));
        transition
    }
    fn settings_reset_failed(&mut self, failure: SettingsFailure) -> Transition {
        let Some(previous) = self.reset_from.take() else { return Transition::default() };
        self.settings_status = previous;
        self.add_notice(NoticeKind::SettingsResetFailed { failure })
    }
    /// Records a Notice. A storage failure that repeats the latest Notice is
    /// not added again, so repeated failing saves do not flood Notices.
    fn add_notice(&mut self, kind: NoticeKind) -> Transition {
        let storage = matches!(kind, NoticeKind::SettingsLoadFailed { .. } | NoticeKind::SettingsSaveFailed { .. } | NoticeKind::SettingsResetFailed { .. } | NoticeKind::SettingsLoadTimedOut);
        if storage && self.notices.last().is_some_and(|last| last.kind == kind) {
            return Transition::default();
        }
        let offers_reset = storage && self.settings_worker && !matches!(kind, NoticeKind::SettingsLoadTimedOut);
        let notice = Notice { id: self.next_notice_id, kind, offers_reset };
        self.next_notice_id += 1;
        let skip = (self.notices.len() + 1).saturating_sub(NOTICE_LIMIT);
        self.notices = self.notices.iter().skip(skip).cloned().chain([notice.clone()]).collect();
        // Failures the person can act on, or should review, open Notices; a
        // success is only recorded.
        let mut open = offers_reset
            || match &notice.kind {
                NoticeKind::SettingsReset { .. } | NoticeKind::JournalUnavailable | NoticeKind::OpenFailed { .. } => true,
                NoticeKind::OperationFinished { outcome, .. } => matches!(outcome, OperationOutcome::Partial | OperationOutcome::Failed | OperationOutcome::CleanupUncertain),
                NoticeKind::TemporariesSwept { failed, .. } => !failed.is_empty(),
                _ => false,
            };
        if self.startup_period && is_startup_notice(&notice.kind) {
            let actionable = offers_reset || matches!(notice.kind, NoticeKind::JournalUnavailable);
            if self.settings_status == SettingsStatus::Loading && !matches!(notice.kind, NoticeKind::SettingsLoadTimedOut) {
                self.startup_notice_pending |= !self.startup_notices_opened;
                open = false;
            } else {
                open = if self.settings.hide_notices_at_startup() { actionable } else { !self.startup_notices_opened };
            }
            self.startup_notices_opened |= open;
            if open {
                self.startup_notice_pending = false;
            }
        }
        Transition::output(Output::NoticeAdded { notice, open })
    }
    fn flush_startup_notice_open(&mut self) -> Transition {
        if self.settings_status != SettingsStatus::Loading && self.startup_notice_pending && !self.settings.hide_notices_at_startup() && !self.startup_notices_opened {
            self.startup_notice_pending = false;
            self.startup_notices_opened = true;
            Transition::output(Output::OpenNotices)
        } else {
            Transition::default()
        }
    }
    /// Applies one Favorites edit, reporting why it changed nothing.
    fn favorites_edit(&mut self, edit: FavoriteEdit, apply: impl FnOnce(&mut Self) -> Result<FavoritesRecords, FavoriteRejection>) -> Transition {
        let result = if self.favorites_ready() { apply(self).and_then(|favorites| self.commit_favorites(favorites)) } else { Err(FavoriteRejection::NotReady) };
        result.unwrap_or_else(|reason| Transition::output(Output::FavoriteEditRejected { edit, reason }))
    }
    fn create_favorite_group(&mut self, name: String) -> Result<FavoritesRecords, FavoriteRejection> {
        let mut favorites = self.checked_name(&name, |favorites| favorites.groups.iter().any(|group| group.name == name))?;
        let id = self.next_favorite_group_id;
        self.next_favorite_group_id = id.checked_add(1).ok_or(FavoriteRejection::Stale)?;
        let position = favorites.groups.len() as i64;
        favorites.groups.push(FavoriteGroupRecord { id, name, position });
        Ok(favorites)
    }
    fn rename_favorite_group(&mut self, id: i64, name: String) -> Result<FavoritesRecords, FavoriteRejection> {
        let mut favorites = self.checked_name(&name, |favorites| favorites.groups.iter().any(|group| group.id != id && group.name == name))?;
        let group = favorites.groups.iter_mut().find(|group| group.id == id).ok_or(FavoriteRejection::Stale)?;
        group.name = name;
        Ok(favorites)
    }
    fn reorder_favorite_group(&mut self, id: i64, position: usize) -> Result<FavoritesRecords, FavoriteRejection> {
        let mut favorites = self.settings.favorites().clone();
        let index = favorites.groups.iter().position(|group| group.id == id).ok_or(FavoriteRejection::Stale)?;
        let group = favorites.groups.remove(index);
        favorites.groups.insert(position.min(favorites.groups.len()), group);
        normalize_group_positions(&mut favorites);
        Ok(favorites)
    }
    fn delete_favorite_group(&mut self, id: i64) -> Result<FavoritesRecords, FavoriteRejection> {
        let mut favorites = self.settings.favorites().clone();
        let old = favorites.groups.len();
        favorites.groups.retain(|group| group.id != id);
        if old == favorites.groups.len() {
            return Err(FavoriteRejection::Stale);
        }
        favorites.items.retain(|item| item.group_id != id);
        normalize_group_positions(&mut favorites);
        Ok(favorites)
    }
    fn create_favorite_item(&mut self, group_id: i64, name: String, target: Location) -> Result<FavoritesRecords, FavoriteRejection> {
        if !self.settings.favorites().groups.iter().any(|group| group.id == group_id) {
            return Err(FavoriteRejection::Stale);
        }
        let mut favorites = self.checked_name(&name, |favorites| favorites.items.iter().any(|item| item.group_id == group_id && item.name == name))?;
        let id = self.next_favorite_item_id;
        self.next_favorite_item_id = id.checked_add(1).ok_or(FavoriteRejection::Stale)?;
        let position = favorites.items.iter().filter(|item| item.group_id == group_id).count() as i64;
        favorites.items.push(FavoriteItemRecord { id, group_id, name, target, position });
        Ok(favorites)
    }
    fn rename_favorite_item(&mut self, id: i64, name: String) -> Result<FavoritesRecords, FavoriteRejection> {
        let group_id = self.settings.favorites().items.iter().find(|item| item.id == id).ok_or(FavoriteRejection::Stale)?.group_id;
        let mut favorites = self.checked_name(&name, |favorites| favorites.items.iter().any(|item| item.id != id && item.group_id == group_id && item.name == name))?;
        if let Some(item) = favorites.items.iter_mut().find(|item| item.id == id) {
            item.name = name;
        }
        Ok(favorites)
    }
    /// The current Favorites to edit, once `name` is valid and `taken` does
    /// not report it as a duplicate.
    fn checked_name(&self, name: &str, taken: impl FnOnce(&FavoritesRecords) -> bool) -> Result<FavoritesRecords, FavoriteRejection> {
        if !valid_favorite_name(name) {
            return Err(FavoriteRejection::InvalidName);
        }
        let favorites = self.settings.favorites();
        if taken(favorites) {
            return Err(FavoriteRejection::DuplicateName);
        }
        Ok(favorites.clone())
    }
    fn move_favorite_item(&mut self, id: i64, group_id: i64, position: usize) -> Result<FavoritesRecords, FavoriteRejection> {
        let mut favorites = self.settings.favorites().clone();
        if !favorites.groups.iter().any(|group| group.id == group_id) {
            return Err(FavoriteRejection::Stale);
        }
        let index = favorites.items.iter().position(|item| item.id == id).ok_or(FavoriteRejection::Stale)?;
        let mut item = favorites.items.remove(index);
        if favorites.items.iter().any(|other| other.group_id == group_id && other.name == item.name) {
            return Err(FavoriteRejection::DuplicateName);
        }
        let same_group = item.group_id == group_id;
        item.group_id = group_id;
        // Past the group's end, the Item goes right after the group's last
        // Item, or back to its own index, so a move to the current position
        // leaves the records unchanged.
        let group_rows = favorites.items.iter().enumerate().filter(|(_, other)| other.group_id == group_id).map(|(row, _)| row).collect::<Vec<_>>();
        let target_index = group_rows.get(position).copied().or_else(|| group_rows.last().map(|last| last + 1)).unwrap_or(if same_group { index } else { favorites.items.len() });
        favorites.items.insert(target_index, item);
        normalize_item_positions(&mut favorites);
        Ok(favorites)
    }
    fn delete_favorite_item(&mut self, id: i64) -> Result<FavoritesRecords, FavoriteRejection> {
        let mut favorites = self.settings.favorites().clone();
        let old = favorites.items.len();
        favorites.items.retain(|item| item.id != id);
        if old == favorites.items.len() {
            return Err(FavoriteRejection::Stale);
        }
        normalize_item_positions(&mut favorites);
        Ok(favorites)
    }
    fn open_favorite_item(&mut self, browser: BrowserSide, id: i64) -> Transition {
        self.settings.favorites().items.iter().find(|item| item.id == id).map(|item| item.target.clone()).map_or_else(Transition::default, |target| self.navigate(browser, target, None))
    }
    /// Applies a launch probe that is still awaited. Only a completed check
    /// that found the target unavailable removes the still-matching Item.
    fn favorite_target_probed(&mut self, item_id: i64, target: Location, outcome: FavoriteProbeOutcome) -> Transition {
        if !self.probing.remove(&item_id) {
            return Transition::default();
        }
        let Some(name) = self.settings.favorites().items.iter().find(|item| item.id == item_id && item.target == target).map(|item| item.name.clone()) else { return Transition::default() };
        match outcome {
            FavoriteProbeOutcome::Available | FavoriteProbeOutcome::Cancelled => Transition::default(),
            FavoriteProbeOutcome::Failed => self.add_notice(NoticeKind::FavoriteProbeFailed { name, target }),
            FavoriteProbeOutcome::Unavailable => {
                let Ok(mut transition) = self.delete_favorite_item(item_id).and_then(|favorites| self.commit_favorites(favorites)) else { return Transition::default() };
                transition.append(self.add_notice(NoticeKind::FavoriteRemoved { name, target }));
                transition
            }
        }
    }
    fn commit_favorites(&mut self, mut favorites: FavoritesRecords) -> Result<Transition, FavoriteRejection> {
        favorites.initialized = true;
        // An edit that changes nothing, such as moving an Item to its own
        // position, neither reports a change nor queues a save.
        if favorites == *self.settings.favorites() {
            return Err(FavoriteRejection::Unchanged);
        }
        if favorites.hierarchy().is_err() {
            return Err(FavoriteRejection::Stale);
        }
        self.settings.replace_favorites(favorites.clone());
        Ok(Transition { outputs: vec![Output::FavoritesChanged { favorites }], work: self.save_settings().into_iter().collect() })
    }
    /// A save of the current settings, or `None` unless the stored settings
    /// loaded successfully. Saving after a pending or failed load would replace
    /// records the application never read.
    fn save_settings(&self) -> Option<WorkRequest> {
        (self.settings_status == SettingsStatus::Loaded).then(|| WorkRequest::SaveSettings { revision: self.settings.revision(), snapshot: self.settings.snapshot() })
    }
    fn clear_selection(&mut self, browser: BrowserSide) -> Transition {
        let tab = self.active_tab(browser);
        self.forget_pending_selection(browser, tab);
        let Some(visit) = self.tab_mut(browser, tab).and_then(|state| state.history.current_mut()) else { return Transition::default() };
        if visit.state_mut().clear_selection() { Transition::output(Output::SelectionChanged { browser, tab, selection: visit.state().selection().clone(), row: None }) } else { Transition::default() }
    }
    /// Applies one selection gesture to the active tab's current visit against
    /// its shown Folder Items, and reports the selection when it changed.
    /// `cursor_row` is the validated row a gesture moves the cursor to; `None`
    /// looks the unchanged cursor up instead.
    fn change_visit(&mut self, browser: BrowserSide, cursor_row: Option<usize>, gesture: impl FnOnce(&mut VisitState, &[Entry]) -> bool) -> Transition {
        let tab = self.active_tab(browser);
        self.forget_pending_selection(browser, tab);
        let Some(TabState { history, folder_items: Some(items), .. }) = self.tab_mut(browser, tab) else { return Transition::default() };
        let Some(visit) = history.current_mut() else { return Transition::default() };
        if !gesture(visit.state_mut(), &items.entries) {
            return Transition::default();
        }
        Transition::output(Output::SelectionChanged { browser, tab, selection: visit.state().selection().clone(), row: cursor_row.or_else(|| visit.state().cursor_row(&items.entries)) })
    }
    /// A selection gesture replaces any selection still waiting for a
    /// refreshed listing.
    fn forget_pending_selection(&mut self, browser: BrowserSide, tab: TabId) {
        if self.pending_selection.as_ref().is_some_and(|pending| pending.browser == browser && pending.tab == tab) {
            self.pending_selection = None;
        }
    }
    /// The shown entry at `row` if it is still `name`, with its location.
    fn shown_entry(&self, browser: BrowserSide, row: usize, name: &EntryName) -> Option<(Location, &Entry)> {
        let items = self.tab(browser, self.active_tab(browser))?.folder_items.as_ref()?;
        items.entries.get(row).filter(|entry| entry.name() == name).map(|entry| (items.location.join(name), entry))
    }
    fn open_entry(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        let Some((location, entry)) = self.shown_entry(browser, row, name) else { return Transition::default() };
        match entry.activation() {
            Activation::Enter => self.navigate(browser, location, None),
            Activation::Open => Transition::output(Output::OpenItem { item: location }),
            Activation::Resolve => {
                let tab = self.active_tab(browser);
                let token = self.next_token();
                self.pending_open = Some((browser, tab, token));
                Transition { outputs: vec![], work: vec![WorkRequest::ResolveItem { browser, tab, token, item: location }] }
            }
            Activation::None => Transition::default(),
        }
    }
    fn show_package_contents(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        match self.shown_entry(browser, row, name) {
            Some((location, entry)) if entry.is_package() => self.navigate(browser, location, None),
            _ => Transition::default(),
        }
    }
    /// Finishes a link activation while its tab is still active and still
    /// shows the folder holding the link.
    fn item_resolved(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, item: Location, target: ResolvedTarget) -> Transition {
        if self.pending_open != Some((browser, tab, token)) {
            return Transition::default();
        }
        self.pending_open = None;
        if self.active_tab(browser) != tab || self.location(browser) != item.parent().as_ref() {
            return Transition::default();
        }
        match target {
            ResolvedTarget::Folder => self.navigate(browser, item, None),
            ResolvedTarget::File | ResolvedTarget::Package => Transition::output(Output::OpenItem { item }),
            ResolvedTarget::Unavailable => self.add_notice(NoticeKind::OpenFailed { item }),
        }
    }
    fn go_to_parent(&mut self, browser: BrowserSide) -> Transition {
        self.location(browser).and_then(Location::parent).map_or_else(Transition::default, |location| self.navigate(browser, location, None))
    }
    fn scroll(&mut self, browser: BrowserSide, scroll: Option<ScrollAnchor>) -> Transition {
        let tab = self.active_tab(browser);
        if let Some(TabState { history, folder_items: Some(items), .. }) = self.tab_mut(browser, tab)
            && let Some(visit) = history.current_mut()
        {
            visit.state_mut().set_scroll(&items.entries, scroll);
        }
        Transition::default()
    }
    fn loaded(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, entries: Arc<[Entry]>, changes: Option<Vec<RowChange>>) -> Transition {
        let Some(pending) = self.take_pending(browser, tab, token) else { return Transition::default() };
        // A Rename or New Folder result is selected once, when its folder's
        // refreshed listing arrives; a listing without it forgets it.
        let select = self.pending_selection.take_if(|selection| selection.browser == browser && selection.tab == tab && selection.folder == pending.location);
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        state.history.arrive(&pending.location, pending.history_target);
        let Some(visit) = state.history.current_mut() else { return Transition::default() };
        visit.state_mut().reconcile(&entries);
        if let Some(select) = select
            && let Some(row) = entries.iter().position(|entry| *entry.name() == select.name)
        {
            visit.state_mut().select(&entries, row, &select.name);
        }
        // A same-folder reload keeps the worker's row change so the view keeps
        // its rows and scroll position. A change computed against anything but
        // the shown Folder Items, or one that does not fit them, replaces every
        // row instead, as does a new folder.
        let changes = match (state.folder_items.as_ref(), pending.base.as_ref(), changes) {
            (Some(old), Some(base), Some(changes)) if Arc::ptr_eq(&old.entries, base) && changes_fit(&changes, old.entries.len(), entries.len()) => Some(changes),
            _ => None,
        };
        state.folder_items = Some(FolderItems { location: pending.location.clone(), entries: Arc::clone(&entries) });
        state.error = None;
        state.restoring = false;
        let visit = visit.state();
        Transition { outputs: vec![Output::FolderItemsLoaded { browser, tab, location: pending.location, scroll_hint: visit.scroll().cloned(), changes, entries: Arc::clone(&entries) }, Output::SelectionChanged { browser, tab, selection: visit.selection().clone(), row: visit.cursor_row(&entries) }], work: vec![] }
    }
    fn failed(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, kind: ListingErrorKind) -> Transition {
        let Some(pending) = self.take_pending(browser, tab, token) else { return Transition::default() };
        if pending.restoring && is_unrestorable(kind) {
            let location = pending.location;
            let mut transition = self.close_tab(browser, tab);
            transition.append(self.add_notice(NoticeKind::TabDiscarded { location, reason: kind }));
            return transition;
        }
        let error = ListingError::new(pending.location, kind);
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        state.error = Some(error.clone());
        Transition::output(Output::FolderItemsFailed { browser, tab, error })
    }
    fn cancelled(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken) -> Transition {
        self.take_pending(browser, tab, token).map_or_else(Transition::default, |_| Transition::output(Output::FolderItemsCancelled { browser, tab }))
    }
    fn take_pending(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken) -> Option<PendingRead> {
        self.tab_mut(browser, tab)?.pending.take_if(|pending| pending.token == token)
    }
    fn tab_view(&self, browser: BrowserSide, id: TabId) -> Output {
        let tab = self.tab(browser, id).expect("tab view needs a live tab");
        let entries = tab.folder_items.as_ref().map_or_else(|| Arc::from([]), |items| Arc::clone(&items.entries));
        let location = tab.folder_items.as_ref().map(|items| items.location.clone());
        let visit = tab.history.current().map(Visit::state);
        let selection = visit.map_or_else(Selection::default, |visit| visit.selection().clone());
        let row = visit.and_then(|visit| visit.cursor_row(&entries));
        Output::TabViewChanged { browser, tab: id, location, selection, row, scroll_hint: visit.and_then(|visit| visit.scroll().cloned()), entries, loading: tab.pending.is_some(), error: tab.error.clone() }
    }
}
impl BrowserState {
    fn new(id: TabId) -> Self {
        Self { order: BrowserTabs::new(id), tabs: vec![TabState::new(id)] }
    }
}
impl TabState {
    fn new(id: TabId) -> Self {
        Self { id, history: TabHistory::default(), folder_items: None, pending: None, error: None, requested: None, restoring: false }
    }
    fn requested(id: TabId, location: Location) -> Self {
        Self { id, history: TabHistory::default(), folder_items: None, pending: None, error: None, requested: Some(location), restoring: true }
    }
    fn requested_location(&self) -> Option<Location> {
        self.folder_items.as_ref().map(|items| items.location.clone()).or_else(|| self.requested.clone())
    }
}

fn normalize_group_positions(favorites: &mut FavoritesRecords) {
    for (position, group) in favorites.groups.iter_mut().enumerate() {
        group.position = position as i64;
    }
}

fn normalize_item_positions(favorites: &mut FavoritesRecords) {
    let groups = favorites.groups.iter().map(|group| group.id).collect::<Vec<_>>();
    for group_id in groups {
        for (position, item) in favorites.items.iter_mut().filter(|item| item.group_id == group_id).enumerate() {
            item.position = position as i64;
        }
    }
}

/// Whether `changes`, applied in order, turn `old_len` rows into `new_len`
/// rows without overlapping or leaving the list.
fn changes_fit(changes: &[RowChange], old_len: usize, new_len: usize) -> bool {
    let mut len = old_len;
    let mut next = 0;
    for change in changes {
        if change.row < next || change.row.checked_add(change.removed).is_none_or(|end| end > len) {
            return false;
        }
        len = (len - change.removed).saturating_add(change.inserted);
        next = change.row.saturating_add(change.inserted);
    }
    len == new_len
}

/// Only a conclusive first restoration failure discards a saved tab. Queue
/// pressure and worker trouble remain refreshable rather than destroying the
/// person's workspace state.
fn is_unrestorable(kind: ListingErrorKind) -> bool {
    matches!(kind, ListingErrorKind::ItemMissing | ListingErrorKind::NotADirectory | ListingErrorKind::PermissionDenied | ListingErrorKind::PrivacyRestricted | ListingErrorKind::Unknown)
}

fn is_startup_notice(kind: &NoticeKind) -> bool {
    matches!(kind, NoticeKind::SettingsLoadFailed { .. } | NoticeKind::SettingsLoadTimedOut | NoticeKind::SessionNotRestored | NoticeKind::TabDiscarded { .. } | NoticeKind::JournalUnavailable | NoticeKind::TemporariesSwept { .. } | NoticeKind::FavoriteRemoved { .. } | NoticeKind::FavoriteProbeFailed { .. })
}
