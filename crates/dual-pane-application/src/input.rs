use std::sync::Arc;

use crate::{ActionId, ApplicationIdentity, ByteProgress, CleanupResult, ClipboardAction, ClipboardGeneration, ClipboardOffer, ClipboardRejection, ClipboardRequestId, ClipboardSnapshot, ContextMenuKind, DefaultApplicationRequest, DragId, DropAction, DropRejection, DropRequestId, DropTarget, FavoriteProbeOutcome, FileAssociationLookup, FileAssociationRequest, FilterFailure, FilterRequest, FilterResult, FinalizeResult, FolderItemsMenuTarget, FolderItemsMenuToken, MonitoringStatus, OpenWithRequest, OpenWithToken, PathSubmission, PlannedItem, PreviewRequest, PreviewResult, ResolvedDropDestination, ResolvedPath, RowChange, ScanFailure, SessionSnapshot, SettingsFailure, SettingsSnapshot, Shortcut, StepResult, TransferId, VisibilityPolicy, WindowLayout};
use dual_pane_domain::{BrowserSide, DecisionToken, Entry, EntryName, ListingErrorKind, Location, OperationChoice, OperationCommand, OperationId, OperationKind, OperationRoot, RequestToken, ScrollAnchor, SortSpec, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    /// A gesture on visible rows or selection, stamped by the showing view.
    ObservedCommand {
        command: Command,
        browser: BrowserSide,
        tab: TabId,
        visible_revision: u64,
    },
    Event(Box<Event>),
}

/// UI-neutral workspace requests. A gesture on a Browser's visible content
/// names the tab it observed, so a delayed gesture is rejected after that tab
/// stopped being active. `Navigate` and `OpenFavoriteItem` address the
/// Browser's active tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    BeginDrag {
        browser: BrowserSide,
        tab: TabId,
        visible_revision: u64,
    },
    PreviewDrop {
        drag: DragId,
        source_browser: BrowserSide,
        target: DropTarget,
        action: DropAction,
    },
    AcceptDrop {
        drag: DragId,
        source_browser: BrowserSide,
        target: DropTarget,
        action: DropAction,
    },
    CancelDrag {
        drag: DragId,
    },
    RejectDrop {
        browser: BrowserSide,
        tab: TabId,
        reason: DropRejection,
    },
    RequestFolderItemsMenu {
        browser: BrowserSide,
        tab: TabId,
        visible_revision: u64,
        kind: ContextMenuKind,
        target: FolderItemsMenuTarget,
    },
    InvokeFolderItemsMenu {
        token: FolderItemsMenuToken,
        action: ActionId,
    },
    InvokeOpenWith {
        menu: FolderItemsMenuToken,
        token: OpenWithToken,
    },
    DismissFolderItemsMenu {
        token: FolderItemsMenuToken,
    },
    CaptureClipboard {
        browser: BrowserSide,
        tab: TabId,
        action: ClipboardAction,
    },
    PasteClipboard {
        browser: BrowserSide,
        tab: TabId,
    },
    SetFilterText {
        browser: BrowserSide,
        tab: TabId,
        text: String,
    },
    StartOperation {
        browser: BrowserSide,
        tab: TabId,
        kind: OperationKind,
    },
    /// Tries another name for a Rename or New Folder whose name was refused.
    /// The new job keeps the refused job's frozen folder and target, whatever
    /// the tab shows now; the refused job closes only if the name is valid.
    RetryName {
        id: OperationId,
        name: EntryName,
    },
    /// Opens the inline editor for Rename or New Folder when the command is
    /// available; otherwise reports why it is not.
    RequestNameEditor {
        browser: BrowserSide,
        tab: TabId,
        command: OperationCommand,
    },
    ConfirmPermanentDelete {
        id: OperationId,
        targets: usize,
    },
    DecideOperation {
        id: OperationId,
        token: DecisionToken,
        item: Location,
        choice: OperationChoice,
        apply_to_all: bool,
    },
    CancelOperation {
        id: OperationId,
    },
    /// Removes a finished job once its panel hides or closes, or once its
    /// Notice is recorded when no panel was shown.
    DismissOperation {
        id: OperationId,
    },
    /// Asks the runtime to reopen the operation safety journal.
    RetryJournal,
    /// Quits the application. While operations run, an unconfirmed request
    /// only asks for confirmation; a confirmed one cancels every job.
    Quit {
        confirmed: bool,
    },
    ActivateBrowser {
        browser: BrowserSide,
    },
    /// Shows `folder` in an existing tab in either Browser, preferring the
    /// active Browser, or opens it in the active Browser,
    /// with `select` selected when it names a file inside it. An open tab on
    /// the folder is activated instead of duplicated. It waits for the
    /// session restore at launch and applies once.
    OpenLaunchTarget {
        folder: Location,
        select: Option<EntryName>,
    },
    /// Brings the existing workspace window forward without changing tabs.
    RaiseWindow,
    ActivateTab {
        browser: BrowserSide,
        tab: TabId,
    },
    NewTab {
        browser: BrowserSide,
    },
    CloseTab {
        browser: BrowserSide,
        tab: TabId,
    },
    ReorderTab {
        browser: BrowserSide,
        tab: TabId,
        position: usize,
    },
    Navigate {
        browser: BrowserSide,
        location: Location,
    },
    /// Navigates from a visual path control that showed `source` in `tab`.
    /// The target is an exact Location, including stored non-UTF-8 names.
    NavigateObservedPath {
        browser: BrowserSide,
        tab: TabId,
        source: Location,
        visible_revision: u64,
        target: Location,
    },
    /// Submits entered path text from the Path Edit Control. It names the tab
    /// the editor observed, which must still be the Browser's active tab.
    SubmitPath {
        browser: BrowserSide,
        tab: TabId,
        submission: PathSubmission,
    },
    /// Escape in the Path Edit Control: cancels the tab's pending path check.
    CancelPathEntry {
        browser: BrowserSide,
        tab: TabId,
    },
    GoBack {
        browser: BrowserSide,
        tab: TabId,
    },
    GoForward {
        browser: BrowserSide,
        tab: TabId,
    },
    Refresh {
        browser: BrowserSide,
        tab: TabId,
    },
    /// Remembers `sort` for `location`, which must still be the confirmed
    /// location of `tab`, and rereads every open tab showing it.
    SetSort {
        browser: BrowserSide,
        tab: TabId,
        location: Location,
        sort: SortSpec,
    },
    CreateFavoriteGroup {
        name: String,
    },
    RenameFavoriteGroup {
        id: i64,
        name: String,
    },
    ReorderFavoriteGroup {
        id: i64,
        position: usize,
    },
    DeleteFavoriteGroup {
        id: i64,
    },
    CreateFavoriteItem {
        group_id: i64,
        name: String,
        target: Location,
    },
    RenameFavoriteItem {
        id: i64,
        name: String,
    },
    MoveFavoriteItem {
        id: i64,
        group_id: i64,
        position: usize,
    },
    DeleteFavoriteItem {
        id: i64,
    },
    OpenFavoriteItem {
        browser: BrowserSide,
        id: i64,
    },
    SelectEntry {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    ToggleEntry {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    SelectAll {
        browser: BrowserSide,
        tab: TabId,
    },
    ClearSelection {
        browser: BrowserSide,
        tab: TabId,
    },
    /// Activates the entry: enters a folder, opens a regular file or
    /// package with its default application, or resolves a link first.
    OpenEntry {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    /// Navigates into a package instead of opening it.
    ShowPackageContents {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    /// Reveals the entry in the Finder without opening or navigating to it.
    RevealEntryInFinder {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    GoToParent {
        browser: BrowserSide,
        tab: TabId,
    },
    UpdateScrollHint {
        browser: BrowserSide,
        tab: TabId,
        row: Option<usize>,
        scroll: Option<ScrollAnchor>,
    },
    SelectRange {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    MoveSelection {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    SecondarySelect {
        browser: BrowserSide,
        tab: TabId,
        row: usize,
        name: EntryName,
    },
    /// Replaces stored settings after the person explicitly confirmed it.
    ResetSettings,
    SetHideNoticesAtStartup {
        hide: bool,
    },
    /// Records whether the Application Close Button asks before quitting.
    SetSkipQuitConfirmation {
        skip: bool,
    },
    SetVisibilityPolicy {
        visibility: VisibilityPolicy,
    },
    SetActionShortcuts {
        action: ActionId,
        shortcuts: Vec<Shortcut>,
    },
    RestoreActionShortcuts {
        action: ActionId,
    },
    SetContextMenuVisibility {
        kind: ContextMenuKind,
        action: ActionId,
        visible: bool,
    },
    ResetContextMenu {
        kind: ContextMenuKind,
    },
    ResetContextMenus,
    AddFileAssociationSample {
        file: dual_pane_domain::Location,
    },
    RefreshFileAssociations,
    SetFileAssociationSelected {
        content_type: String,
        kind: ContextMenuKind,
        application: ApplicationIdentity,
        selected: bool,
    },
    ClearFileAssociation {
        content_type: String,
    },
    RemoveFileAssociation {
        content_type: String,
    },
    SetDefaultApplication {
        request: DefaultApplicationRequest,
    },
    UpdateWindowLayout {
        layout: WindowLayout,
    },
    /// Reports usable Preview Pane content dimensions for the active tab;
    /// zero collapses it. A stale tab cannot start preview work.
    SetPreviewTarget {
        browser: BrowserSide,
        tab: TabId,
        width: u32,
        height: u32,
    },
}

impl Command {
    /// Commands whose meaning depends on the rows or selection currently
    /// shown by a Browser must carry an observed visible revision.
    pub fn visible_target(&self) -> Option<(BrowserSide, TabId)> {
        match self {
            Self::CaptureClipboard { browser, tab, .. } | Self::StartOperation { browser, tab, .. } | Self::RequestNameEditor { browser, tab, .. } | Self::SelectEntry { browser, tab, .. } | Self::ToggleEntry { browser, tab, .. } | Self::SelectRange { browser, tab, .. } | Self::MoveSelection { browser, tab, .. } | Self::SecondarySelect { browser, tab, .. } | Self::SelectAll { browser, tab } | Self::ClearSelection { browser, tab } | Self::OpenEntry { browser, tab, .. } | Self::ShowPackageContents { browser, tab, .. } | Self::RevealEntryInFinder { browser, tab, .. } | Self::UpdateScrollHint { browser, tab, .. } => Some((*browser, *tab)),
            _ => None,
        }
    }
}

/// Externally observed facts, usually the results of work requests.
///
/// In `FolderItemsLoaded`, `changes` turns the request's `previous` Folder
/// Items into `entries`, or is `None` when the request carried none.
/// `ScreenshotsFolderProbed` answers `WorkRequest::ProbeScreenshotsFolder`;
/// only `Available` adds Screenshots to the seeded Favorites.
/// `FavoriteTargetProbed` answers `WorkRequest::ProbeFavoriteTarget`.
/// `LocationInvalidated` reports that a location's contents may have changed;
/// every tab showing or loading it rereads it. `SettingsReset` reports where
/// the replaced database was preserved, when there was one to preserve.
/// `OperationProgress` is a coalesced display update for the step with that
/// generation. `ItemResolved` answers `WorkRequest::ResolveItem`, and
/// `OpenFailed` reports that macOS refused to open an item. `JournalStatus`
/// arrives at launch and after each reopen attempt; until the first one,
/// the journal counts as available. `TemporariesSwept` reports the launch
/// sweep of temporaries left by earlier launches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    FileAssociationsResolved {
        request: FileAssociationRequest,
        outcome: FileAssociationLookup,
    },
    FileAssociationRetryDue {
        folder: Location,
        generation: u64,
    },
    OpenWithFinished {
        request: OpenWithRequest,
        result: Result<(), String>,
    },
    DefaultApplicationFinished {
        request: DefaultApplicationRequest,
        result: Result<(), String>,
    },
    DropPreflighted {
        request: DropRequestId,
        result: Result<ResolvedDropDestination, DropRejection>,
    },
    /// Answers `WorkRequest::ResolvePath`. A result is accepted only while its
    /// token is still the tab's pending path check.
    PathResolved {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        result: Result<ResolvedPath, ListingErrorKind>,
    },
    ClipboardPublished {
        request: ClipboardRequestId,
        transfer: TransferId,
        generation: ClipboardGeneration,
        succeeded: bool,
    },
    ClipboardChanged {
        generation: ClipboardGeneration,
        offer: ClipboardOffer,
    },
    ClipboardRead {
        request: ClipboardRequestId,
        generation: ClipboardGeneration,
        snapshot: ClipboardSnapshot,
    },
    ClipboardPreflighted {
        request: ClipboardRequestId,
        generation: ClipboardGeneration,
        result: Result<Arc<[OperationRoot]>, ClipboardRejection>,
    },
    FilterProjected {
        request: FilterRequest,
        result: Result<FilterResult, FilterFailure>,
    },
    PreviewFinished {
        request: PreviewRequest,
        result: PreviewResult,
    },
    OperationScanned {
        id: OperationId,
        generation: u64,
        plan: Result<Arc<[PlannedItem]>, ScanFailure>,
    },
    OperationStepped {
        id: OperationId,
        generation: u64,
        result: StepResult,
    },
    OperationFinalized {
        id: OperationId,
        generation: u64,
        result: FinalizeResult,
    },
    OperationCleaned {
        id: OperationId,
        generation: u64,
        result: CleanupResult,
    },
    OperationExecutorUnavailable {
        id: OperationId,
        generation: u64,
    },
    OperationProgress {
        id: OperationId,
        generation: u64,
        bytes: ByteProgress,
    },
    ItemResolved {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        item: Location,
        target: ResolvedTarget,
    },
    OpenFailed {
        item: Location,
    },
    JournalStatus {
        available: bool,
    },
    TemporariesSwept {
        removed: usize,
        failed: Vec<Location>,
    },
    LocationInvalidated {
        location: Location,
    },
    /// A generation-tagged native or periodic subscription became usable.
    WatcherReady {
        location: Location,
        generation: u64,
        status: MonitoringStatus,
    },
    WatcherStatusChanged {
        location: Location,
        generation: u64,
        status: MonitoringStatus,
    },
    WatchedLocationInvalidated {
        location: Location,
        generation: u64,
    },
    /// Whether the application is foreground-active. Native subscriptions
    /// stay registered while inactive, but watcher-driven reads pause.
    ApplicationActivityChanged {
        active: bool,
    },
    FolderItemsLoaded {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        entries: Arc<[Entry]>,
        changes: Option<Vec<RowChange>>,
    },
    FolderItemsFailed {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        kind: ListingErrorKind,
    },
    FolderItemsCancelled {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
    },
    /// The runtime has delivered a result and may have room for a visibility
    /// read whose earlier admission returned Busy.
    ReadCapacityAvailable,
    FavoriteTargetProbed {
        item_id: i64,
        target: Location,
        outcome: FavoriteProbeOutcome,
    },
    ScreenshotsFolderProbed {
        location: Location,
        outcome: FavoriteProbeOutcome,
    },
    SettingsSaved {
        revision: u64,
    },
    SettingsSaveFailed {
        revision: u64,
        failure: SettingsFailure,
    },
    SessionSaveFailed {
        revision: u64,
        failure: SettingsFailure,
    },
    SettingsLoaded {
        snapshot: SettingsSnapshot,
    },
    /// Settings storage uses this atomic result so a launch cannot combine a
    /// settings moment with a session from another moment.
    SettingsLoadedWithSession {
        snapshot: SettingsSnapshot,
        session: SessionSnapshot,
    },
    SettingsLoadFailed {
        failure: SettingsFailure,
    },
    SettingsLoadFailedAtLaunch {
        failure: SettingsFailure,
    },
    SettingsLoadTimedOut,
    StartupPeriodElapsed,
    SettingsReset {
        backup: Option<Location>,
    },
    SettingsResetFailed {
        failure: SettingsFailure,
    },
}
/// What a link resolved to on a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTarget {
    Folder,
    File,
    Package,
    /// Missing, unreadable, or neither a file nor a folder.
    Unavailable,
}

impl From<Command> for Input {
    fn from(value: Command) -> Self {
        Self::Command(value)
    }
}
impl From<Event> for Input {
    fn from(value: Event) -> Self {
        Self::Event(Box::new(value))
    }
}
