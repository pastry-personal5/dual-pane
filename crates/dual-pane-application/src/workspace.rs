use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::clipboard::CapturedTransfer;
use crate::operations::OperationCoordinator;
use crate::{ActionBinding, BrowserChrome, BrowserSnapshot, ClipboardAction, ClipboardGeneration, ClipboardOffer, ClipboardRejection, ClipboardRequestId, ClipboardSnapshot, Command, Event, FavoriteEdit, FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoriteRejection, FavoritesRecords, FilterFailure, FilterRequest, FilterResult, FilterStatus, Input, MonitoringStatus, Notice, NoticeKind, OperationEffect, OperationJob, OperationOutcome, OperationStatus, Output, PreviewRequest, PreviewResult, PreviewView, ResolvedTarget, RowChange, SessionSnapshot, SettingsFailure, SettingsState, SettingsStatus, TabSnapshot, TabSummary, TransferId, VisibilityPolicy, WindowLayout, WorkRequest, WorkspaceChrome, WorkspaceSnapshot, fresh_profile_favorites, fresh_profile_screenshots};
use dual_pane_domain::{Activation, BrowserSide, BrowserTabs, Entry, EntryKind, EntryName, ListingError, ListingErrorKind, Location, OperationCommand, OperationId, OperationIntent, OperationKind, OperationRejection, OperationRoot, OperationTarget, RequestToken, ScrollAnchor, Selection, SortSpec, TabHistory, TabId, Visit, VisitState, starts_with, valid_favorite_name};

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
    /// Presentation may open Settings only after a known-good load. A storage
    /// failure leaves in-memory state intact but disables that interaction
    /// until Reset Settings has reloaded a fresh store.
    settings_interaction_available: bool,
    /// False once the session learned that no settings service exists, so
    /// nothing can be reset.
    settings_worker: bool,
    /// The status before a confirmed reset, restored if the reset fails.
    reset_from: Option<SettingsStatus>,
    /// Save results for this revision or older predate the latest reset.
    stale_save_floor: Option<u64>,
    stale_session_save_floor: Option<u64>,
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
    transfers: HashMap<TransferId, CapturedTransfer>,
    current_transfer: Option<(TransferId, ClipboardGeneration)>,
    pending_publications: HashMap<ClipboardRequestId, TransferId>,
    latest_publication: Option<ClipboardRequestId>,
    pending_paste: Option<PendingPaste>,
    cut_operations: HashMap<OperationId, TransferId>,
    clipboard_generation: Option<ClipboardGeneration>,
    clipboard_offer: ClipboardOffer,
    next_clipboard_request: u64,
    next_transfer: u64,
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
    /// One application-owned generation per distinct open byte-exact
    /// location, shared by every tab that needs it.
    watches: HashMap<Location, u64>,
    watch_status: HashMap<Location, MonitoringStatus>,
    next_watch_generation: u64,
    application_active: bool,
    deferred_watch_invalidations: HashSet<Location>,
    watch_visible: HashSet<Location>,
    visibility_generation: u64,
    visibility_retries: HashMap<(BrowserSide, TabId), (Location, Option<usize>)>,
    left_preview: PreviewSlot,
    right_preview: PreviewSlot,
}
#[derive(Debug)]
struct PreviewSlot {
    target_width: u32,
    target_height: u32,
    current: Option<PreviewRequest>,
    view: PreviewView,
    next_token: u64,
}

impl Default for PreviewSlot {
    fn default() -> Self {
        Self { target_width: 0, target_height: 0, current: None, view: PreviewView::Collapsed, next_token: 0 }
    }
}
fn same_preview_target(left: &PreviewRequest, right: &PreviewRequest) -> bool {
    left.browser == right.browser && left.tab == right.tab && left.listing == right.listing && left.name == right.name && left.location == right.location && left.target_width == right.target_width && left.target_height == right.target_height
}
#[derive(Debug, Clone)]
struct PendingSelection {
    browser: BrowserSide,
    tab: TabId,
    folder: Location,
    name: EntryName,
}
#[derive(Debug)]
struct PendingPaste {
    request: ClipboardRequestId,
    browser: BrowserSide,
    tab: TabId,
    destination: Location,
    generation: Option<ClipboardGeneration>,
    external_sources: Option<Arc<[Location]>>,
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
    /// A stricter policy has removed the shown rows; retain visit state only
    /// as a reconciliation hint until a fresh listing succeeds.
    visibility_cleared: bool,
    visibility_recheck: bool,
    filter_text: String,
    filter_generation: u64,
    visible_revision: u64,
    filter_status: FilterStatus,
    filter_result: Option<FilterResult>,
    scroll_reference: Option<Arc<[Entry]>>,
}
#[derive(Debug)]
struct FolderItems {
    location: Location,
    entries: Arc<[Entry]>,
    listing: RequestToken,
}
#[derive(Debug)]
struct PendingRead {
    token: RequestToken,
    location: Location,
    history_target: Option<usize>,
    /// The shown Folder Items the read was asked to diff against.
    base: Option<Arc<[Entry]>>,
    restoring: bool,
    /// One coalesced watcher or operation invalidation observed while this
    /// exact location was already being read.
    dirty: bool,
    visibility_generation: u64,
    retry_on_busy: bool,
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
        Self { left: BrowserState::new(left_id), right: BrowserState::new(right_id), active_browser: BrowserSide::Left, home, settings: SettingsState::new(), settings_status: SettingsStatus::Loading, settings_interaction_available: false, settings_worker: true, reset_from: None, stale_save_floor: None, stale_session_save_floor: None, pending_sorts: Vec::new(), screenshots_probe: None, probing: HashSet::new(), notices: Arc::from([]), next_notice_id: 1, last_token: None, next_tab: 2, next_favorite_group_id: 1, next_favorite_item_id: 1, operations: OperationCoordinator::new(), operation_origins: HashMap::new(), transfers: HashMap::new(), current_transfer: None, pending_publications: HashMap::new(), latest_publication: None, pending_paste: None, cut_operations: HashMap::new(), clipboard_generation: None, clipboard_offer: ClipboardOffer::Unsupported, next_clipboard_request: 0, next_transfer: 0, journal_available: true, pending_selection: None, pending_open: None, quitting: false, layout: None, launch_completed: false, session_write_protected: false, session_revision: 0, startup_period: true, startup_notices_opened: false, startup_notice_pending: false, watches: HashMap::new(), watch_status: HashMap::new(), next_watch_generation: 0, application_active: true, deferred_watch_invalidations: HashSet::new(), watch_visible: HashSet::new(), visibility_generation: 0, visibility_retries: HashMap::new(), left_preview: PreviewSlot::default(), right_preview: PreviewSlot::default() }
    }
    pub fn handle(&mut self, input: Input) -> Transition {
        let save_session = matches!(&input, Input::Command(Command::ActivateBrowser { .. } | Command::ActivateTab { .. } | Command::NewTab { .. } | Command::CloseTab { .. } | Command::ReorderTab { .. } | Command::Navigate { .. }) | Input::Event(Event::FolderItemsLoaded { .. }));
        let mut transition = match input {
            Input::Command(command) => {
                if let Command::CaptureClipboard { browser, .. } = command {
                    Transition::output(Output::ClipboardRejected { browser: Some(browser), request: None, reason: ClipboardRejection::StaleObservation })
                } else if command.visible_target().is_some_and(|(browser, tab)| self.tab(browser, tab).is_some_and(|state| !state.filter_text.is_empty())) {
                    Transition::default()
                } else {
                    self.command(command)
                }
            }
            Input::ObservedCommand { command, browser, tab, visible_revision } => {
                if command.visible_target() == Some((browser, tab)) && self.valid_observation(browser, tab, visible_revision) {
                    self.command(command)
                } else if matches!(command, Command::CaptureClipboard { .. }) {
                    Transition::output(Output::ClipboardRejected { browser: Some(browser), request: None, reason: ClipboardRejection::StaleObservation })
                } else {
                    Transition::default()
                }
            }
            Input::Event(event) => self.event(event),
        };
        if save_session {
            self.session_revision = self.session_revision.wrapping_add(1);
            if let Some(session) = self.session_snapshot() {
                transition.work.push(WorkRequest::SaveSession { revision: self.session_revision, session });
            }
        }
        self.reconcile_watches(&mut transition);
        self.reconcile_previews(&mut transition);
        transition.append(self.flush_startup_notice_open());
        transition
    }
    fn command(&mut self, command: Command) -> Transition {
        match command {
            Command::CaptureClipboard { browser, tab, action } => self.capture_clipboard(browser, tab, action),
            Command::PasteClipboard { browser, tab } => self.paste_clipboard(browser, tab),
            Command::SetFilterText { browser, tab, text } => self.set_filter_text(browser, tab, text),
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
            Command::SetVisibilityPolicy { visibility } => self.set_visibility_policy(visibility),
            Command::SetActionShortcuts { action, shortcuts } => self.set_action_shortcuts(action, shortcuts),
            Command::RestoreActionShortcuts { action } => self.restore_action_shortcuts(action),
            Command::UpdateWindowLayout { layout } => self.update_window_layout(layout),
            Command::SetPreviewTarget { browser, tab, width, height } => self.set_preview_target(browser, tab, width, height),
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
            Command::UpdateScrollHint { browser, tab, row, scroll } => self.on_active(browser, tab, |workspace| workspace.scroll(browser, row, scroll)),
        }
    }
    fn event(&mut self, event: Event) -> Transition {
        match event {
            Event::ClipboardPublished { request, transfer, generation, succeeded } => self.clipboard_published(request, transfer, generation, succeeded),
            Event::ClipboardChanged { generation, offer } => self.clipboard_changed(generation, offer),
            Event::ClipboardRead { request, generation, snapshot } => self.clipboard_read(request, generation, snapshot),
            Event::ClipboardPreflighted { request, generation, result } => self.clipboard_preflighted(request, generation, result),
            Event::FilterProjected { request, result } => self.filter_projected(request, result),
            Event::PreviewFinished { request, result } => self.preview_finished(request, result),
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
            Event::WatcherReady { location, generation, status } => self.watcher_ready(location, generation, status),
            Event::WatcherStatusChanged { location, generation, status } => self.watcher_status_changed(location, generation, status),
            Event::WatchedLocationInvalidated { location, generation } => self.watched_location_invalidated(location, generation),
            Event::ApplicationActivityChanged { active } => self.application_activity_changed(active),
            Event::FolderItemsLoaded { browser, tab, token, entries, changes } => self.loaded(browser, tab, token, entries, changes),
            Event::FolderItemsFailed { browser, tab, token, kind } => self.failed(browser, tab, token, kind),
            Event::FolderItemsCancelled { browser, tab, token } => self.cancelled(browser, tab, token),
            Event::ReadCapacityAvailable => self.retry_visibility_reads(),
            Event::FavoriteTargetProbed { item_id, target, outcome } => self.favorite_target_probed(item_id, target, outcome),
            Event::ScreenshotsFolderProbed { location, outcome } => self.screenshots_folder_probed(&location, outcome),
            Event::SettingsSaved { revision } => {
                if !self.is_stale_save(revision) {
                    self.settings.mark_saved(revision);
                }
                Transition::default()
            }
            Event::SettingsSaveFailed { revision, failure } => self.settings_save_failed(revision, failure),
            Event::SessionSaveFailed { revision, failure } => self.session_save_failed(revision, failure),
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
        self.tab(side, self.active_tab(side)).filter(|tab| tab.folder_items.is_some()).and_then(|tab| tab.history.current()).map_or(&NONE, |visit| visit.state().selection())
    }
    pub fn operation_jobs(&self) -> &[OperationJob] {
        self.operations.jobs()
    }
    pub fn clipboard_capture_availability(&self, browser: BrowserSide, tab: TabId) -> Result<(), ClipboardRejection> {
        self.clipboard_roots(browser, tab).map(|_| ())
    }
    pub fn paste_availability(&self, browser: BrowserSide, tab: TabId) -> Result<(), ClipboardRejection> {
        self.paste_destination(browser, tab)?;
        if self.clipboard_offer.supports_paste() { Ok(()) } else { Err(ClipboardRejection::UnsupportedContent) }
    }
    fn clipboard_roots(&self, browser: BrowserSide, tab: TabId) -> Result<Arc<[OperationRoot]>, ClipboardRejection> {
        if self.active_tab(browser) != tab {
            return Err(ClipboardRejection::StaleTab);
        }
        let state = self.tab(browser, tab).ok_or(ClipboardRejection::StaleTab)?;
        if state.pending.is_some() || state.error.is_some() || state.filter_status != FilterStatus::Ready {
            return Err(ClipboardRejection::SourceUnavailable);
        }
        let items = state.folder_items.as_ref().ok_or(ClipboardRejection::SourceUnavailable)?;
        let selected = state.history.current().map(|visit| visit.state().selection().entries()).unwrap_or(&[]);
        if selected.is_empty() {
            return Err(ClipboardRejection::NoSelection);
        }
        let selected_names = selected.iter().collect::<HashSet<_>>();
        if selected_names.len() != selected.len() {
            return Err(ClipboardRejection::StaleObservation);
        }
        let roots = state.visible_entries().iter().filter(|entry| selected_names.contains(entry.name())).map(|entry| OperationRoot { source: items.location.join(entry.name()), kind: entry.kind() }).collect::<Vec<_>>();
        if roots.len() != selected.len() {
            return Err(ClipboardRejection::StaleObservation);
        }
        Ok(roots.into())
    }
    fn paste_destination(&self, browser: BrowserSide, tab: TabId) -> Result<Location, ClipboardRejection> {
        if self.active_browser != browser || self.active_tab(browser) != tab {
            return Err(ClipboardRejection::StaleTab);
        }
        let state = self.tab(browser, tab).ok_or(ClipboardRejection::StaleTab)?;
        if state.pending.is_some() || state.error.is_some() || state.filter_status != FilterStatus::Ready {
            return Err(ClipboardRejection::DestinationUnavailable);
        }
        state.folder_items.as_ref().map(|items| items.location.clone()).ok_or(ClipboardRejection::DestinationUnavailable)
    }
    fn next_clipboard_request(&mut self) -> ClipboardRequestId {
        self.next_clipboard_request = self.next_clipboard_request.wrapping_add(1);
        ClipboardRequestId::new(self.next_clipboard_request)
    }
    fn discard_transfer_if_unused(&mut self, transfer: TransferId) {
        let current = self.current_transfer.is_some_and(|current| current.0 == transfer);
        let publishing = self.pending_publications.values().any(|pending| *pending == transfer);
        let running_cut = self.cut_operations.values().any(|running| *running == transfer);
        if !current && !publishing && !running_cut {
            self.transfers.remove(&transfer);
        }
    }
    fn discard_pending_publications(&mut self) {
        let transfers = self.pending_publications.drain().map(|(_, transfer)| transfer).collect::<Vec<_>>();
        self.latest_publication = None;
        for transfer in transfers {
            self.discard_transfer_if_unused(transfer);
        }
    }
    fn clipboard_presentation(&self) -> Output {
        // A started Cut keeps its cue after a clipboard replacement, until its operation ends.
        let current_cut = self.current_transfer.map(|(transfer, _)| transfer).filter(|transfer| self.transfers.get(transfer).is_some_and(|captured| captured.action == ClipboardAction::Cut));
        let pending_cut = current_cut.or_else(|| self.cut_operations.values().copied().next()).and_then(|transfer| self.transfers.get(&transfer)).map_or_else(|| Arc::from([]), |captured| Arc::clone(&captured.roots));
        Output::ClipboardPresentationChanged { offer: self.clipboard_offer, pending_cut }
    }
    fn capture_clipboard(&mut self, browser: BrowserSide, tab: TabId, action: ClipboardAction) -> Transition {
        let roots = match self.clipboard_roots(browser, tab) {
            Ok(roots) => roots,
            Err(reason) => return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: None, reason }),
        };
        // Only the newest publication may become current. Its work request
        // owns the payload it needs, so superseded captured state can go now.
        self.discard_pending_publications();
        self.next_transfer = self.next_transfer.wrapping_add(1);
        let transfer = TransferId::new(self.next_transfer);
        let request = self.next_clipboard_request();
        self.transfers.insert(transfer, CapturedTransfer { browser, tab, action, roots: Arc::clone(&roots), reserved: false });
        self.pending_publications.insert(request, transfer);
        self.latest_publication = Some(request);
        let finder_roots = (action == ClipboardAction::Copy).then_some(roots);
        Transition { outputs: vec![], work: vec![WorkRequest::PublishClipboard { request, transfer, action, finder_roots }] }
    }
    fn clipboard_published(&mut self, request: ClipboardRequestId, transfer: TransferId, generation: ClipboardGeneration, succeeded: bool) -> Transition {
        let Some(expected) = self.pending_publications.remove(&request) else { return Transition::default() };
        let is_latest = self.latest_publication == Some(request);
        if is_latest {
            self.latest_publication = None;
        }
        if expected != transfer || !is_latest {
            self.discard_transfer_if_unused(expected);
            return Transition::default();
        }
        if !succeeded {
            let browser = self.transfers.get(&transfer).map(|captured| captured.browser);
            // Only a write newer than anything observed has replaced the previous content.
            if self.clipboard_generation.is_some_and(|observed| generation > observed) {
                let mut transition = self.clipboard_changed(generation, ClipboardOffer::Unsupported);
                self.discard_transfer_if_unused(transfer);
                transition.outputs.push(Output::ClipboardRejected { browser, request: Some(request), reason: ClipboardRejection::PublicationFailed });
                return transition;
            }
            self.discard_transfer_if_unused(transfer);
            return Transition::output(Output::ClipboardRejected { browser, request: Some(request), reason: ClipboardRejection::PublicationFailed });
        }
        // The GUI may have observed this write, or a later replacement, before
        // the lane reported it. Only a replacement newer than the write
        // outranks the publication.
        if self.clipboard_generation.is_some_and(|observed| observed > generation) {
            self.discard_transfer_if_unused(transfer);
            return Transition::default();
        }
        let previous = self.current_transfer.replace((transfer, generation));
        self.clipboard_generation = Some(generation);
        self.clipboard_offer = if self.transfers.get(&transfer).is_some_and(|captured| captured.action == ClipboardAction::Copy) { ClipboardOffer::PrivateAndFiles } else { ClipboardOffer::Private };
        if let Some((previous, _)) = previous {
            self.discard_transfer_if_unused(previous);
        }
        let captured = self.transfers.get(&transfer).expect("published transfer remains captured");
        Transition { outputs: vec![Output::ClipboardPrepared { browser: captured.browser, action: captured.action, count: captured.roots.len() }, self.clipboard_presentation()], work: vec![] }
    }
    fn clipboard_changed(&mut self, generation: ClipboardGeneration, offer: ClipboardOffer) -> Transition {
        if self.clipboard_generation == Some(generation) && self.clipboard_offer == offer {
            return Transition::default();
        }
        let replaced = self.clipboard_generation != Some(generation);
        self.clipboard_generation = Some(generation);
        self.clipboard_offer = offer;
        if !replaced {
            return Transition::output(self.clipboard_presentation());
        }
        let previous = self.current_transfer.take();
        self.pending_paste = None;
        if let Some((previous, _)) = previous {
            self.discard_transfer_if_unused(previous);
        }
        Transition::output(self.clipboard_presentation())
    }
    fn paste_clipboard(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        let destination = match self.paste_destination(browser, tab) {
            Ok(destination) => destination,
            Err(reason) => return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: None, reason }),
        };
        let request = self.next_clipboard_request();
        self.pending_paste = Some(PendingPaste { request, browser, tab, destination, generation: None, external_sources: None });
        Transition { outputs: vec![], work: vec![WorkRequest::ReadClipboard { request }] }
    }
    fn clipboard_read(&mut self, request: ClipboardRequestId, generation: ClipboardGeneration, snapshot: ClipboardSnapshot) -> Transition {
        let Some(pending) = self.pending_paste.as_mut().filter(|pending| pending.request == request) else { return Transition::default() };
        let browser = pending.browser;
        if self.clipboard_generation.is_some_and(|current| current != generation) {
            self.pending_paste = None;
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::ClipboardReplaced });
        }
        self.clipboard_generation = Some(generation);
        pending.generation = Some(generation);
        match snapshot {
            ClipboardSnapshot::Private { transfer } => self.start_private_paste(request, transfer, generation),
            ClipboardSnapshot::ExternalFiles { sources } => self.start_external_paste(request, generation, sources),
            ClipboardSnapshot::PrivateAndExternal { transfer, sources } => {
                if self.current_transfer == Some((transfer, generation)) && self.transfers.contains_key(&transfer) {
                    self.start_private_paste(request, transfer, generation)
                } else {
                    self.start_external_paste(request, generation, sources)
                }
            }
            ClipboardSnapshot::Unavailable => {
                self.pending_paste = None;
                Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::ClipboardBusy })
            }
            ClipboardSnapshot::Unsupported => {
                self.pending_paste = None;
                Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::UnsupportedContent })
            }
        }
    }
    fn start_external_paste(&mut self, request: ClipboardRequestId, generation: ClipboardGeneration, sources: Arc<[Location]>) -> Transition {
        let Some(pending) = self.pending_paste.as_mut().filter(|pending| pending.request == request && pending.generation == Some(generation)) else { return Transition::default() };
        let browser = pending.browser;
        let mut seen = HashSet::new();
        if sources.is_empty() || sources.iter().any(Location::is_root) {
            self.pending_paste = None;
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::MalformedExternalContent });
        }
        if sources.iter().any(|source| !seen.insert(source)) {
            self.pending_paste = None;
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::DuplicateSource });
        }
        pending.external_sources = Some(Arc::clone(&sources));
        Transition { outputs: vec![], work: vec![WorkRequest::PreflightClipboard { request, generation, sources }] }
    }
    fn start_private_paste(&mut self, request: ClipboardRequestId, transfer: TransferId, generation: ClipboardGeneration) -> Transition {
        let Some(pending) = self.pending_paste.take().filter(|pending| pending.request == request && pending.generation == Some(generation)) else { return Transition::default() };
        let browser = pending.browser;
        if self.current_transfer != Some((transfer, generation)) {
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::UnknownPrivateTransfer });
        }
        let Some(captured) = self.transfers.get(&transfer) else {
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::UnknownPrivateTransfer });
        };
        if captured.reserved {
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::TransferReserved });
        }
        let kind = match captured.action {
            ClipboardAction::Copy => OperationKind::Copy,
            ClipboardAction::Cut => OperationKind::Move,
        };
        self.start_clipboard_operation(pending, Some(transfer), kind, captured.roots.to_vec())
    }
    fn clipboard_preflighted(&mut self, request: ClipboardRequestId, generation: ClipboardGeneration, result: Result<Arc<[OperationRoot]>, ClipboardRejection>) -> Transition {
        let Some(pending) = self.pending_paste.take().filter(|pending| pending.request == request && pending.generation == Some(generation)) else { return Transition::default() };
        let browser = pending.browser;
        if self.clipboard_generation != Some(generation) {
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::ClipboardReplaced });
        }
        let roots = match result {
            Ok(roots) if pending.external_sources.as_deref().is_some_and(|sources| sources.len() == roots.len() && sources.iter().zip(roots.iter()).all(|(source, root)| source == &root.source) && roots.iter().all(|root| root.kind != EntryKind::Other)) => roots,
            Ok(_) => return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::SourceChanged }),
            Err(reason) => return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason }),
        };
        self.start_clipboard_operation(pending, None, OperationKind::Copy, roots.to_vec())
    }
    fn start_clipboard_operation(&mut self, pending: PendingPaste, transfer: Option<TransferId>, kind: OperationKind, roots: Vec<OperationRoot>) -> Transition {
        let request = pending.request;
        let browser = pending.browser;
        let intent = match OperationIntent::transfer(kind, roots, pending.destination) {
            Ok(intent) => intent,
            Err(reason) => return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: clipboard_operation_rejection(reason) }),
        };
        if matches!(intent.kind(), OperationKind::Copy) && !self.journal_available {
            return Transition::output(Output::ClipboardRejected { browser: Some(browser), request: Some(request), reason: ClipboardRejection::JournalUnavailable });
        }
        let (id, effects) = self.operations.start(intent);
        self.operation_origins.insert(id, (pending.browser, pending.tab));
        if let Some(transfer) = transfer
            && let Some(captured) = self.transfers.get_mut(&transfer)
            && captured.action == ClipboardAction::Cut
        {
            captured.reserved = true;
            self.cut_operations.insert(id, transfer);
        }
        let mut transition = self.operation_output(id, effects);
        transition.outputs.push(Output::ClipboardPasteStarted { browser: pending.browser, tab: pending.tab });
        transition
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
        if source_tab.pending.is_some() || source_tab.error.is_some() || source_tab.filter_status != FilterStatus::Ready {
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
            let targets = source_tab.visible_entries().iter().filter(|entry| selected_names.contains(entry.name())).map(|entry| OperationTarget { name: entry.name().clone(), kind: entry.kind() }).collect::<Vec<_>>();
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
        let mut cut_consumed = false;
        if let Some(transfer) = self.cut_operations.remove(&id) {
            cut_consumed = true;
            self.transfers.remove(&transfer);
            if self.current_transfer.is_some_and(|current| current.0 == transfer) {
                self.current_transfer = None;
            }
        }
        // A pending confirmation or a refused name closes without having
        // changed anything, so it leaves no summary.
        if matches!(before, OperationStatus::AwaitingConfirmation { .. } | OperationStatus::NameCollision { .. } | OperationStatus::NameRejected { .. }) {
            return Transition::default();
        }
        let Some(job) = self.operations.job(id).cloned() else { return Transition::default() };
        let mut transition = Transition::default();
        if cut_consumed {
            transition.outputs.push(self.clipboard_presentation());
        }
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
        let mut folders = intent.roots().iter().filter_map(|root| root.source.parent()).collect::<Vec<_>>();
        folders.extend(intent.destination().cloned());
        let mut roots = match intent.kind() {
            OperationKind::NewFolder { name } => vec![source.join(name)],
            OperationKind::Rename { to } => vec![source.join(to)],
            _ => vec![],
        };
        for root in intent.roots() {
            roots.push(root.source.clone());
            roots.extend(intent.destination_for(&root.source));
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
    pub fn settings_interaction_available(&self) -> bool {
        self.settings_interaction_available
    }
    /// The effective shortcut of every catalogued action.
    pub fn bindings(&self) -> Vec<ActionBinding> {
        self.settings.all_bindings()
    }
    pub fn shortcuts(&self, action: crate::ActionId) -> &[crate::Shortcut] {
        self.settings.shortcuts(action)
    }
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }
    /// The Browser's tab strip, toolbar availability, and effective sort.
    pub fn browser_chrome(&self, side: BrowserSide) -> BrowserChrome {
        let state = self.browser(side);
        let tabs = state.order.tabs().iter().filter_map(|id| self.tab(side, *id)).map(|tab| TabSummary { id: tab.id, location: tab.folder_items.as_ref().map(|items| &items.location).or_else(|| tab.pending.as_ref().map(|pending| &pending.location)).or_else(|| (tab.visibility_cleared || tab.error.is_some()).then_some(tab.requested.as_ref()).flatten()).cloned(), loading: tab.pending.is_some(), failed: tab.error.is_some() }).collect();
        let location = self.location(side).cloned();
        let monitored = self.tab(side, state.order.active()).and_then(TabState::requested_location);
        let monitoring = monitored.as_ref().and_then(|location| self.watch_status.get(location)).copied();
        BrowserChrome { tabs, active_tab: state.order.active(), can_open_tab: state.order.can_insert() && location.is_some(), can_go_back: self.can_go_back(side), can_go_forward: self.can_go_forward(side), can_go_up: location.as_ref().is_some_and(|location| !location.is_root()), can_refresh: location.is_some() || self.loading_location(side).is_some(), sort: location.as_ref().map(|location| self.settings.peek_sort(location)), location, monitoring }
    }
    /// Workspace-wide presentation state.
    pub fn workspace_chrome(&self) -> WorkspaceChrome {
        WorkspaceChrome { active_browser: self.active_browser, favorites: self.settings.favorites().clone(), favorites_ready: self.favorites_ready(), bindings: self.bindings(), hide_notices_at_startup: self.settings.hide_notices_at_startup(), visibility: self.settings.visibility(), notices_startup_ready: self.settings_status != SettingsStatus::Loading, settings_interaction_available: self.settings_interaction_available, notices: Arc::clone(&self.notices) }
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
    fn valid_observation(&self, browser: BrowserSide, tab: TabId, revision: u64) -> bool {
        self.active_tab(browser) == tab && self.tab(browser, tab).is_some_and(|state| state.visible_revision == revision && state.filter_status == FilterStatus::Ready)
    }
    fn set_filter_text(&mut self, browser: BrowserSide, tab: TabId, text: String) -> Transition {
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        if state.filter_text == text && state.filter_status != FilterStatus::Failed {
            return Transition::default();
        }
        state.filter_text = text;
        self.begin_filter_projection(browser, tab)
    }
    fn begin_filter_projection(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        let visibility_generation = self.visibility_generation;
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        state.filter_generation = state.filter_generation.wrapping_add(1);
        state.visible_revision = state.visible_revision.wrapping_add(1);
        if state.scroll_reference.is_none() {
            state.scroll_reference = state.folder_items.as_ref().map(|items| Arc::clone(&items.entries));
        }
        state.filter_result = None;
        let Some(items) = state.folder_items.as_ref() else {
            state.filter_status = FilterStatus::Ready;
            state.scroll_reference = None;
            return Transition::default();
        };
        let request = FilterRequest { browser, tab, listing: items.listing, visibility_generation, generation: state.filter_generation, text: state.filter_text.clone(), entries: Arc::clone(&items.entries) };
        if request.text.is_empty() {
            state.filter_status = FilterStatus::Ready;
            let entries = Arc::clone(&items.entries);
            if let Some(visit) = state.history.current_mut() {
                visit.state_mut().reconcile(&entries);
            }
            state.scroll_reference = None;
            return Transition { outputs: vec![self.visible_output(browser, tab)], work: vec![WorkRequest::CancelFilter { browser, tab }] };
        }
        state.filter_status = FilterStatus::Updating;
        Transition { outputs: vec![self.visible_output(browser, tab)], work: vec![WorkRequest::ProjectFilter { request }] }
    }
    fn filter_projected(&mut self, request: FilterRequest, result: Result<FilterResult, FilterFailure>) -> Transition {
        let FilterRequest { browser, tab, listing, visibility_generation, generation, text, .. } = &request;
        if *visibility_generation != self.visibility_generation {
            return Transition::default();
        }
        let Some(state) = self.tab(*browser, *tab) else { return Transition::default() };
        if state.filter_generation != *generation || state.filter_text != *text || state.filter_status != FilterStatus::Updating || state.folder_items.as_ref().is_none_or(|items| items.listing != *listing || !Arc::ptr_eq(&items.entries, &request.entries)) {
            return Transition::default();
        }
        let old_full = state.scroll_reference.as_ref().cloned().or_else(|| state.folder_items.as_ref().map(|items| Arc::clone(&items.entries))).unwrap_or_else(|| Arc::from([]));
        let old_scroll = state.history.current().and_then(|visit| visit.state().scroll().cloned());
        let location = state.folder_items.as_ref().map(|items| items.location.clone());
        let result = result.and_then(|result| authoritative_filter_result(&request.entries, result));
        let select = if result.is_ok() { self.pending_selection.take_if(|selection| selection.browser == *browser && selection.tab == *tab && location.as_ref() == Some(&selection.folder)) } else { None };
        let Some(state) = self.tab_mut(*browser, *tab) else { return Transition::default() };
        match result {
            Ok(result) => {
                let scroll = nearest_scroll(old_scroll.as_ref(), &old_full, &result.entries);
                if let Some(visit) = state.history.current_mut() {
                    visit.state_mut().reconcile(&result.entries);
                    if let Some(scroll) = scroll {
                        visit.state_mut().set_scroll(&result.entries, Some(scroll));
                    }
                    if let Some(select) = select
                        && let Some(row) = result.entries.iter().position(|entry| *entry.name() == select.name)
                    {
                        visit.state_mut().select(&result.entries, row, &select.name);
                    }
                }
                state.filter_result = Some(result);
                state.filter_status = FilterStatus::Ready;
            }
            Err(_) => {
                state.filter_status = FilterStatus::Failed;
            }
        }
        state.visible_revision = state.visible_revision.wrapping_add(1);
        state.scroll_reference = None;
        Transition::output(self.visible_output(*browser, *tab))
    }
    fn visible_output(&self, browser: BrowserSide, tab: TabId) -> Output {
        let state = self.tab(browser, tab).expect("visible output needs a live tab");
        let items = state.folder_items.as_ref().expect("visible output needs a listing");
        let entries = state.visible_entries_arc();
        let visit = state.history.current().map(Visit::state);
        Output::VisibleItemsChanged { browser, tab, location: items.location.clone(), entries: Arc::clone(&entries), selection: visit.map_or_else(Selection::default, |visit| visit.selection().clone()), row: visit.and_then(|visit| visit.cursor_row(&entries)), scroll_hint: visit.and_then(|visit| visit.scroll().cloned()), filter_text: state.filter_text.clone(), filter_status: state.filter_status, total_count: items.entries.len(), visible_revision: state.visible_revision, loading: state.pending.is_some(), error: state.error.clone() }
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
        if let Some(state) = self.tab_mut(browser, tab) {
            state.visible_revision = state.visible_revision.wrapping_add(1);
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
        self.visibility_retries.remove(&(browser, tab));
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
        let mut transition = Transition { outputs: vec![Output::TabsChanged { browser, active_tab: replacement }], work: pending.into_iter().map(|token| WorkRequest::Cancel { browser, tab, token }).chain(std::iter::once(WorkRequest::CancelFilter { browser, tab })).collect() };
        if was_active {
            if let Some(state) = self.tab_mut(browser, replacement) {
                state.visible_revision = state.visible_revision.wrapping_add(1);
            }
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
            tab.visibility_recheck = false;
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
        self.visibility_retries.remove(&(browser, tab));
        let visibility_generation = self.visibility_generation;
        let previous = self.tab(browser, tab).and_then(|state| state.folder_items.as_ref()).filter(|items| items.location == location).map(|items| Arc::clone(&items.entries));
        let old = self.tab_mut(browser, tab).and_then(|state| {
            state.error = None;
            state.requested = Some(location.clone());
            state.pending.replace(PendingRead { token, location: location.clone(), history_target, base: previous.clone(), restoring: state.restoring, dirty: false, visibility_generation, retry_on_busy: state.visibility_recheck })
        });
        let mut work = old.into_iter().map(|old| WorkRequest::Cancel { browser, tab, token: old.token }).collect::<Vec<_>>();
        work.push(WorkRequest::ReadDirectory { browser, tab, token, location: location.clone(), sort, visibility: self.settings.visibility(), previous });
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
        let targets = [BrowserSide::Left, BrowserSide::Right]
            .into_iter()
            .flat_map(|side| {
                self.browser(side).tabs.iter().filter_map(move |tab| {
                    if tab.pending.as_ref().is_some_and(|pending| pending.location == *location) {
                        Some((side, tab.id, true))
                    } else if tab.pending.is_none() && (tab.folder_items.as_ref().is_some_and(|items| items.location == *location) || tab.requested.as_ref() == Some(location)) {
                        Some((side, tab.id, false))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();
        targets.into_iter().fold(Transition::default(), |mut total, (side, tab, pending)| {
            if pending {
                if let Some(pending) = self.tab_mut(side, tab).and_then(|state| state.pending.as_mut()) {
                    pending.dirty = true;
                }
            } else {
                total.append(self.refresh_tab(side, tab));
            }
            total
        })
    }
    /// A person's sort choice supersedes matching in-flight reads instead of
    /// being folded into their watcher dirty bit.
    fn restart_location(&mut self, location: &Location) -> Transition {
        let targets = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|side| self.browser(side).tabs.iter().filter(move |tab| tab.pending.as_ref().map(|pending| &pending.location).or_else(|| tab.folder_items.as_ref().map(|items| &items.location)) == Some(location)).map(move |tab| (side, tab.id))).collect::<Vec<_>>();
        targets.into_iter().fold(Transition::default(), |mut total, (side, tab)| {
            total.append(self.refresh_tab(side, tab));
            total
        })
    }
    /// Diffs the application-owned desired set after every transition. New
    /// subscriptions are dispatched before reads so watch readiness closes
    /// the initial-listing race; removals follow the transition's work.
    fn reconcile_watches(&mut self, transition: &mut Transition) {
        let desired = [BrowserSide::Left, BrowserSide::Right]
            .into_iter()
            .flat_map(|side| self.browser(side).tabs.iter())
            .flat_map(|tab| {
                let shown = tab.folder_items.as_ref().map(|items| items.location.clone());
                let pending = tab.pending.as_ref().map(|pending| pending.location.clone());
                let requested = tab.requested.clone();
                shown.into_iter().chain(pending).chain(requested)
            })
            .collect::<HashSet<_>>();
        let removed = self.watches.keys().filter(|location| !desired.contains(*location)).cloned().collect::<Vec<_>>();
        let mut starts = Vec::new();
        for location in desired {
            if self.watches.contains_key(&location) {
                continue;
            }
            self.next_watch_generation = self.next_watch_generation.wrapping_add(1).max(1);
            let generation = self.next_watch_generation;
            self.watches.insert(location.clone(), generation);
            self.watch_status.remove(&location);
            starts.push(WorkRequest::WatchLocation { location, generation });
        }
        if !starts.is_empty() {
            starts.append(&mut transition.work);
            transition.work = starts;
        }
        for location in removed {
            if let Some(generation) = self.watches.remove(&location) {
                self.watch_status.remove(&location);
                self.deferred_watch_invalidations.remove(&location);
                transition.work.push(WorkRequest::UnwatchLocation { location, generation });
            }
        }
        let visible = [BrowserSide::Left, BrowserSide::Right]
            .into_iter()
            .filter_map(|side| self.tab(side, self.active_tab(side)))
            .flat_map(|tab| {
                let shown = tab.folder_items.as_ref().map(|items| items.location.clone());
                let pending = tab.pending.as_ref().map(|pending| pending.location.clone());
                let requested = tab.requested.clone();
                shown.into_iter().chain(pending).chain(requested)
            })
            .filter(|location| self.watches.contains_key(location))
            .collect::<HashSet<_>>();
        if visible != self.watch_visible {
            self.watch_visible = visible;
            transition.work.push(WorkRequest::UpdateWatchVisibility { visible: self.watch_visible.iter().cloned().collect() });
        }
    }
    fn watcher_ready(&mut self, location: Location, generation: u64, status: MonitoringStatus) -> Transition {
        if self.watches.get(&location) != Some(&generation) {
            return Transition::default();
        }
        self.watch_status.insert(location.clone(), status);
        self.invalidate_from_watch(location)
    }
    fn watcher_status_changed(&mut self, location: Location, generation: u64, status: MonitoringStatus) -> Transition {
        if self.watches.get(&location) == Some(&generation) {
            self.watch_status.insert(location, status);
        }
        Transition::default()
    }
    fn watched_location_invalidated(&mut self, location: Location, generation: u64) -> Transition {
        if self.watches.get(&location) != Some(&generation) {
            return Transition::default();
        }
        self.invalidate_from_watch(location)
    }
    fn invalidate_from_watch(&mut self, location: Location) -> Transition {
        if self.application_active {
            self.refresh_location(&location)
        } else {
            self.deferred_watch_invalidations.insert(location);
            Transition::default()
        }
    }
    fn application_activity_changed(&mut self, active: bool) -> Transition {
        if self.application_active == active {
            return Transition::default();
        }
        self.application_active = active;
        let mut transition = Transition { outputs: vec![], work: vec![WorkRequest::SetWatchActivity { active }] };
        if active {
            self.deferred_watch_invalidations.clear();
            let locations = self.watches.keys().cloned().collect::<Vec<_>>();
            for location in locations {
                transition.append(self.refresh_location(&location));
            }
        }
        transition
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
        let mut transition = self.restart_location(&location);
        transition.work.extend(self.save_settings());
        transition
    }
    fn settings_loaded(&mut self, snapshot: crate::SettingsSnapshot, session: SessionSnapshot) -> Transition {
        if self.settings_status != SettingsStatus::Loading {
            return Transition::default();
        }
        let previous_visibility = self.settings.visibility();
        self.settings.apply(snapshot);
        if self.settings.visibility() != previous_visibility {
            self.visibility_generation = self.visibility_generation.wrapping_add(1);
        }
        // The loaded snapshot is what storage already holds.
        self.settings.mark_saved(self.settings.revision());
        self.settings_status = SettingsStatus::Loaded;
        self.settings_interaction_available = true;
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
            let tabs = self.restore_read_order();
            if self.settings.visibility().may_hide_more_than(previous_visibility) {
                for (side, tab) in &tabs {
                    if let Some(state) = self.tab_mut(*side, *tab) {
                        state.folder_items = None;
                        state.error = None;
                        state.visibility_cleared = true;
                        state.visibility_recheck = true;
                    }
                    if self.active_tab(*side) == *tab {
                        transition.outputs.push(self.tab_view(*side, *tab));
                    }
                }
            }
            for (side, tab) in tabs {
                if self.settings.visibility() != previous_visibility
                    && let Some(state) = self.tab_mut(side, tab)
                {
                    state.visibility_recheck = true;
                }
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
        self.settings_interaction_available = false;
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
        self.settings_interaction_available = false;
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
        self.settings_interaction_available = false;
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
    fn set_visibility_policy(&mut self, visibility: VisibilityPolicy) -> Transition {
        if !self.settings_interaction_available {
            return Transition::default();
        }
        let previous = self.settings.visibility();
        if !self.settings.set_visibility(visibility) {
            return Transition::default();
        }
        self.visibility_generation = self.visibility_generation.wrapping_add(1);
        let stricter = visibility.may_hide_more_than(previous);
        let targets = self.restore_read_order();
        let mut transition = Transition { outputs: Vec::new(), work: self.save_settings().into_iter().collect() };
        for (side, tab) in targets {
            if stricter {
                if let Some(state) = self.tab_mut(side, tab) {
                    state.folder_items = None;
                    state.filter_result = None;
                    state.filter_status = FilterStatus::Updating;
                    state.visible_revision = state.visible_revision.wrapping_add(1);
                    state.error = None;
                    state.visibility_cleared = true;
                }
                if self.active_tab(side) == tab {
                    transition.outputs.push(self.tab_view(side, tab));
                }
                transition.work.push(WorkRequest::CancelFilter { browser: side, tab });
            }
            if let Some(state) = self.tab_mut(side, tab) {
                state.visibility_recheck = true;
            }
            transition.append(self.refresh_tab(side, tab));
        }
        transition
    }
    /// Retry only reads whose visibility-driven replacement was refused by
    /// the bounded runtime. A later runtime completion supplies capacity.
    fn retry_visibility_reads(&mut self) -> Transition {
        let targets = self.restore_read_order();
        let mut transition = Transition::default();
        for (side, tab) in targets {
            if let Some((location, history_target)) = self.visibility_retries.remove(&(side, tab))
                && self.tab(side, tab).is_some_and(|state| state.pending.is_none() && state.visibility_recheck)
            {
                transition.append(self.start_read(side, tab, location, history_target));
            }
        }
        transition
    }
    fn set_action_shortcuts(&mut self, action: crate::ActionId, shortcuts: Vec<crate::Shortcut>) -> Transition {
        if !self.settings_interaction_available {
            return Transition::default();
        }
        match self.settings.set_shortcuts(action, shortcuts) {
            Ok(true) => Transition { outputs: vec![], work: self.save_settings().into_iter().collect() },
            Ok(false) => Transition::default(),
            Err(reason) => Transition::output(Output::ShortcutEditRejected { action, reason }),
        }
    }
    fn restore_action_shortcuts(&mut self, action: crate::ActionId) -> Transition {
        if !self.settings_interaction_available {
            return Transition::default();
        }
        match self.settings.restore_shortcuts(action) {
            Ok(true) => Transition { outputs: vec![], work: self.save_settings().into_iter().collect() },
            Ok(false) => Transition::default(),
            Err(reason) => Transition::output(Output::ShortcutEditRejected { action, reason }),
        }
    }
    fn update_window_layout(&mut self, layout: WindowLayout) -> Transition {
        if !layout.valid() {
            return Transition::default();
        }
        self.layout = Some(layout);
        self.session_revision += 1;
        Transition { outputs: vec![], work: self.session_snapshot().map(|session| WorkRequest::SaveSession { revision: self.session_revision, session }).into_iter().collect() }
    }
    fn preview_slot(&self, browser: BrowserSide) -> &PreviewSlot {
        match browser {
            BrowserSide::Left => &self.left_preview,
            BrowserSide::Right => &self.right_preview,
        }
    }
    fn preview_slot_mut(&mut self, browser: BrowserSide) -> &mut PreviewSlot {
        match browser {
            BrowserSide::Left => &mut self.left_preview,
            BrowserSide::Right => &mut self.right_preview,
        }
    }
    fn set_preview_target(&mut self, browser: BrowserSide, tab: TabId, width: u32, height: u32) -> Transition {
        if self.active_tab(browser) == tab {
            let slot = self.preview_slot_mut(browser);
            slot.target_width = width.min(8192);
            slot.target_height = height.min(8192);
        }
        Transition::default()
    }
    fn preview_candidate(&self, browser: BrowserSide, width: u32, height: u32) -> (Option<PreviewRequest>, PreviewView) {
        if width == 0 || height == 0 {
            return (None, PreviewView::Collapsed);
        }
        let tab = self.active_tab(browser);
        let Some(state) = self.tab(browser, tab) else { return (None, PreviewView::NoSelection) };
        let Some(items) = state.folder_items.as_ref().filter(|_| state.pending.is_none() && state.error.is_none() && state.filter_status == FilterStatus::Ready) else { return (None, PreviewView::NoSelection) };
        let selected = state.history.current().map(|visit| visit.state().selection().entries()).unwrap_or(&[]);
        let [name] = selected else {
            return (None, if selected.is_empty() { PreviewView::NoSelection } else { PreviewView::MultipleSelection });
        };
        let Some(entry) = state.visible_entries().iter().find(|entry| entry.name() == name) else { return (None, PreviewView::Unavailable { name: name.clone() }) };
        if entry.kind() != EntryKind::File {
            return (None, PreviewView::UnsupportedSelection { name: name.clone() });
        }
        (Some(PreviewRequest { browser, tab, listing: items.listing, name: name.clone(), location: items.location.join(name), target_width: width, target_height: height, token: 0 }), PreviewView::Loading { name: name.clone() })
    }
    fn reconcile_previews(&mut self, transition: &mut Transition) {
        for browser in [BrowserSide::Left, BrowserSide::Right] {
            let slot = self.preview_slot(browser);
            let (candidate, empty_view) = self.preview_candidate(browser, slot.target_width, slot.target_height);
            let slot = self.preview_slot_mut(browser);
            if candidate.as_ref().is_some_and(|candidate| slot.current.as_ref().is_some_and(|current| same_preview_target(candidate, current))) {
                continue;
            }
            if let Some(current) = slot.current.take() {
                transition.work.push(WorkRequest::CancelPreview { browser, token: current.token });
            }
            let view = if let Some(mut request) = candidate {
                slot.next_token = slot.next_token.wrapping_add(1).max(1);
                request.token = slot.next_token;
                let view = PreviewView::Loading { name: request.name.clone() };
                slot.current = Some(request.clone());
                transition.work.push(WorkRequest::ReadPreview { request });
                view
            } else {
                empty_view
            };
            if slot.view != view {
                slot.view = view.clone();
                transition.outputs.push(Output::PreviewChanged { browser, view });
            }
        }
    }
    fn preview_finished(&mut self, request: PreviewRequest, result: PreviewResult) -> Transition {
        let slot = self.preview_slot(request.browser);
        if slot.current.as_ref() != Some(&request) {
            return Transition::default();
        }
        let (candidate, _) = self.preview_candidate(request.browser, slot.target_width, slot.target_height);
        if !candidate.as_ref().is_some_and(|candidate| same_preview_target(candidate, &request)) {
            return Transition::default();
        }
        let view = match result {
            PreviewResult::Image { pixels, width, height, source_width, source_height, byte_size, modified_unix_seconds } if width > 0 && height > 0 && usize::try_from(width).ok().and_then(|width| usize::try_from(height).ok().and_then(|height| width.checked_mul(height))).and_then(|pixels| pixels.checked_mul(4)) == Some(pixels.len()) => PreviewView::Image { name: request.name.clone(), pixels, width, height, source_width, source_height, byte_size, modified_unix_seconds },
            PreviewResult::Image { .. } | PreviewResult::DecodeFailed => PreviewView::DecodeFailed { name: request.name.clone() },
            PreviewResult::Unsupported => PreviewView::UnsupportedSelection { name: request.name.clone() },
            PreviewResult::Unavailable | PreviewResult::Cancelled => PreviewView::Unavailable { name: request.name.clone() },
        };
        let slot = self.preview_slot_mut(request.browser);
        if slot.view == view {
            return Transition::default();
        }
        slot.view = view.clone();
        Transition::output(Output::PreviewChanged { browser: request.browser, view })
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
        self.save_failed(revision, failure)
    }
    fn session_save_failed(&mut self, revision: u64, failure: SettingsFailure) -> Transition {
        if self.stale_session_save_floor.is_some_and(|floor| revision <= floor) {
            return Transition::default();
        }
        self.save_failed(revision, failure)
    }
    fn save_failed(&mut self, revision: u64, failure: SettingsFailure) -> Transition {
        self.settings_interaction_available = false;
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
        self.settings_interaction_available = false;
        self.stale_save_floor = Some(self.settings.revision());
        self.stale_session_save_floor = Some(self.session_revision);
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
        self.visibility_retries.clear();
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
        self.settings_interaction_available = false;
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
        if self.tab(browser, tab).is_none_or(|state| state.filter_status != FilterStatus::Ready) {
            return Transition::default();
        }
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
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        if state.filter_status != FilterStatus::Ready || state.folder_items.is_none() {
            return Transition::default();
        }
        let entries = state.visible_entries_arc();
        let history = &mut state.history;
        let Some(visit) = history.current_mut() else { return Transition::default() };
        if !gesture(visit.state_mut(), &entries) {
            return Transition::default();
        }
        Transition::output(Output::SelectionChanged { browser, tab, selection: visit.state().selection().clone(), row: cursor_row.or_else(|| visit.state().cursor_row(&entries)) })
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
        let state = self.tab(browser, self.active_tab(browser))?;
        let items = state.folder_items.as_ref()?;
        (state.filter_status == FilterStatus::Ready).then_some(())?;
        state.visible_entries().get(row).filter(|entry| entry.name() == name).map(|entry| (items.location.join(name), entry))
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
    fn scroll(&mut self, browser: BrowserSide, row: Option<usize>, scroll: Option<ScrollAnchor>) -> Transition {
        let tab = self.active_tab(browser);
        if let Some(state) = self.tab_mut(browser, tab)
            && state.filter_status == FilterStatus::Ready
        {
            let entries = state.visible_entries_arc();
            let valid = match (row, scroll.as_ref()) {
                (Some(row), Some(scroll)) => entries.get(row).is_some_and(|entry| entry.name() == scroll.entry()),
                (None, None) => true,
                _ => false,
            };
            if valid && let Some(visit) = state.history.current_mut() {
                visit.state_mut().set_scroll(&entries, scroll);
            }
        }
        Transition::default()
    }
    fn loaded(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, entries: Arc<[Entry]>, changes: Option<Vec<RowChange>>) -> Transition {
        let Some(pending) = self.take_pending(browser, tab, token) else { return Transition::default() };
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        let different_folder = state.history.current().is_none_or(|visit| visit.location() != &pending.location);
        if different_folder {
            state.filter_text.clear();
            state.filter_generation = state.filter_generation.wrapping_add(1);
            state.filter_result = None;
            state.filter_status = FilterStatus::Ready;
            state.scroll_reference = None;
        } else if !state.filter_text.is_empty() {
            state.scroll_reference = state.folder_items.as_ref().map(|items| Arc::clone(&items.entries));
        }
        state.history.arrive(&pending.location, pending.history_target);
        let Some(visit) = state.history.current_mut() else { return Transition::default() };
        if state.filter_text.is_empty() {
            visit.state_mut().reconcile(&entries);
        }
        // A same-folder reload keeps the worker's row change so the view keeps
        // its rows and scroll position. A change computed against anything but
        // the shown Folder Items, or one that does not fit them, replaces every
        // row instead, as does a new folder.
        let changes = match (state.folder_items.as_ref(), pending.base.as_ref(), changes) {
            (Some(old), Some(base), Some(changes)) if Arc::ptr_eq(&old.entries, base) && changes_fit(&changes, old.entries.len(), entries.len()) => Some(changes),
            _ => None,
        };
        state.folder_items = Some(FolderItems { location: pending.location.clone(), entries: Arc::clone(&entries), listing: token });
        state.error = None;
        state.restoring = false;
        state.visibility_cleared = false;
        state.visibility_recheck = false;
        state.visible_revision = state.visible_revision.wrapping_add(1);
        let filtered = !state.filter_text.is_empty();
        let location = pending.location.clone();
        let dirty = pending.dirty;
        let mut transition = if filtered {
            let mut projection = self.begin_filter_projection(browser, tab);
            projection.work.push(WorkRequest::WatchReadCompleted { location: location.clone(), succeeded: true });
            projection
        } else {
            let select = self.pending_selection.take_if(|selection| selection.browser == browser && selection.tab == tab && selection.folder == location);
            if let Some(select) = select
                && let Some(row) = entries.iter().position(|entry| *entry.name() == select.name)
                && let Some(visit) = self.tab_mut(browser, tab).and_then(|state| state.history.current_mut())
            {
                visit.state_mut().select(&entries, row, &select.name);
            }
            let state = self.tab(browser, tab).expect("loaded tab remains live");
            let visit = state.history.current().map(Visit::state);
            Transition { outputs: vec![Output::FolderItemsLoaded { browser, tab, location: pending.location, scroll_hint: visit.and_then(|visit| visit.scroll().cloned()), changes, entries: Arc::clone(&entries), visible_revision: state.visible_revision }, Output::SelectionChanged { browser, tab, selection: visit.map_or_else(Selection::default, |visit| visit.selection().clone()), row: visit.and_then(|visit| visit.cursor_row(&entries)) }], work: vec![WorkRequest::WatchReadCompleted { location: location.clone(), succeeded: true }] }
        };
        if dirty && self.tab(browser, tab).and_then(TabState::requested_location).as_ref() == Some(&location) {
            transition.append(self.start_read(browser, tab, location, None));
        }
        transition
    }
    fn failed(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, kind: ListingErrorKind) -> Transition {
        let Some(pending) = self.take_pending(browser, tab, token) else { return Transition::default() };
        if pending.restoring && is_unrestorable(kind) {
            let location = pending.location;
            let mut transition = self.close_tab(browser, tab);
            transition.append(self.add_notice(NoticeKind::TabDiscarded { location, reason: kind }));
            return transition;
        }
        let location = pending.location.clone();
        let dirty = pending.dirty;
        if kind == ListingErrorKind::Busy && pending.retry_on_busy {
            self.visibility_retries.insert((browser, tab), (location.clone(), pending.history_target));
        }
        let error = ListingError::new(pending.location, kind);
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        state.error = Some(error.clone());
        let mut transition = Transition { outputs: vec![Output::FolderItemsFailed { browser, tab, error }], work: vec![WorkRequest::WatchReadCompleted { location: location.clone(), succeeded: false }] };
        if dirty && kind != ListingErrorKind::Busy && self.tab(browser, tab).and_then(TabState::requested_location).as_ref() == Some(&location) {
            transition.append(self.start_read(browser, tab, location, None));
        }
        transition
    }
    fn cancelled(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken) -> Transition {
        self.take_pending(browser, tab, token).map_or_else(Transition::default, |pending| {
            let location = pending.location.clone();
            let mut transition = Transition::output(Output::FolderItemsCancelled { browser, tab });
            if pending.dirty && self.tab(browser, tab).and_then(TabState::requested_location).as_ref() == Some(&location) {
                transition.append(self.start_read(browser, tab, location, None));
            }
            transition
        })
    }
    fn take_pending(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken) -> Option<PendingRead> {
        let generation = self.visibility_generation;
        self.tab_mut(browser, tab)?.pending.take_if(|pending| pending.token == token && pending.visibility_generation == generation)
    }
    fn tab_view(&self, browser: BrowserSide, id: TabId) -> Output {
        let tab = self.tab(browser, id).expect("tab view needs a live tab");
        let entries = tab.visible_entries_arc();
        let location = tab.folder_items.as_ref().map(|items| items.location.clone()).or_else(|| (tab.visibility_cleared || tab.error.is_some()).then_some(tab.requested.clone()).flatten());
        let visit = tab.history.current().map(Visit::state);
        let selection = if tab.folder_items.is_some() { visit.map_or_else(Selection::default, |visit| visit.selection().clone()) } else { Selection::default() };
        let row = if tab.folder_items.is_some() { visit.and_then(|visit| visit.cursor_row(&entries)) } else { None };
        Output::TabViewChanged { browser, tab: id, location, selection, row, scroll_hint: if tab.folder_items.is_some() { visit.and_then(|visit| visit.scroll().cloned()) } else { None }, entries, loading: tab.pending.is_some(), error: tab.error.clone(), filter_text: tab.filter_text.clone(), filter_status: tab.filter_status, total_count: tab.folder_items.as_ref().map_or(0, |items| items.entries.len()), visible_revision: tab.visible_revision }
    }
}

fn clipboard_operation_rejection(reason: OperationRejection) -> ClipboardRejection {
    match reason {
        OperationRejection::SameSourceAndDestination => ClipboardRejection::SameSourceAndDestination,
        OperationRejection::DestinationWithinSource => ClipboardRejection::DestinationWithinSource,
        OperationRejection::DestinationUnavailable | OperationRejection::DestinationRequired => ClipboardRejection::DestinationUnavailable,
        OperationRejection::SourceUnavailable => ClipboardRejection::SourceUnavailable,
        OperationRejection::DuplicateTarget => ClipboardRejection::DuplicateSource,
        OperationRejection::JournalUnavailable => ClipboardRejection::JournalUnavailable,
        OperationRejection::StaleTab => ClipboardRejection::StaleTab,
        OperationRejection::NoTargets => ClipboardRejection::NoSelection,
        OperationRejection::StaleSelection => ClipboardRejection::StaleObservation,
        OperationRejection::InvalidTargetCount | OperationRejection::InvalidName | OperationRejection::UnchangedName | OperationRejection::DestinationNotAllowed => ClipboardRejection::MalformedExternalContent,
    }
}
impl BrowserState {
    fn new(id: TabId) -> Self {
        Self { order: BrowserTabs::new(id), tabs: vec![TabState::new(id)] }
    }
}
impl TabState {
    fn new(id: TabId) -> Self {
        Self { id, history: TabHistory::default(), folder_items: None, pending: None, error: None, requested: None, restoring: false, visibility_cleared: false, visibility_recheck: false, filter_text: String::new(), filter_generation: 0, visible_revision: 0, filter_status: FilterStatus::Ready, filter_result: None, scroll_reference: None }
    }
    fn requested(id: TabId, location: Location) -> Self {
        Self { requested: Some(location), restoring: true, ..Self::new(id) }
    }
    fn requested_location(&self) -> Option<Location> {
        self.folder_items.as_ref().map(|items| items.location.clone()).or_else(|| self.requested.clone())
    }
    fn visible_entries(&self) -> &[Entry] {
        match (self.filter_status, self.filter_text.is_empty(), self.filter_result.as_ref()) {
            (FilterStatus::Ready, true, _) => self.folder_items.as_ref().map_or(&[], |items| &items.entries),
            (FilterStatus::Ready, false, Some(result)) => &result.entries,
            _ => &[],
        }
    }
    fn visible_entries_arc(&self) -> Arc<[Entry]> {
        match (self.filter_status, self.filter_text.is_empty(), self.filter_result.as_ref()) {
            (FilterStatus::Ready, true, _) => self.folder_items.as_ref().map_or_else(|| Arc::from([]), |items| Arc::clone(&items.entries)),
            (FilterStatus::Ready, false, Some(result)) => Arc::clone(&result.entries),
            _ => Arc::from([]),
        }
    }
}

fn authoritative_filter_result(entries: &Arc<[Entry]>, result: FilterResult) -> Result<FilterResult, FilterFailure> {
    if result.indices.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(FilterFailure::InvalidResult);
    }
    let projected = result.indices.iter().map(|&index| entries.get(index).cloned()).collect::<Option<Vec<_>>>().ok_or(FilterFailure::InvalidResult)?;
    if projected.as_slice() != result.entries.as_ref() {
        return Err(FilterFailure::InvalidResult);
    }
    Ok(FilterResult { indices: result.indices, entries: projected.into() })
}

fn nearest_scroll(old: Option<&ScrollAnchor>, previous: &[Entry], visible: &[Entry]) -> Option<ScrollAnchor> {
    let old = old?;
    if visible.iter().any(|entry| entry.name() == old.entry()) {
        return Some(old.clone());
    }
    let old_index = previous.iter().position(|entry| entry.name() == old.entry())?;
    let positions = previous.iter().enumerate().map(|(index, entry)| (entry.name(), index)).collect::<HashMap<_, _>>();
    visible.iter().filter_map(|entry| positions.get(entry.name()).map(|index| (*index, entry.name()))).min_by_key(|(index, _)| (index.abs_diff(old_index), *index < old_index)).map(|(_, name)| ScrollAnchor::new(name.clone(), 0))
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
