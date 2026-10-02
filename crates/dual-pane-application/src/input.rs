use std::sync::Arc;

use crate::{CleanupResult, FavoriteProbeOutcome, OperationFailure, PlannedItem, RowChange, SettingsFailure, SettingsSnapshot, StepResult};
use dual_pane_domain::{BrowserSide, DecisionToken, Entry, EntryName, ListingErrorKind, Location, OperationChoice, OperationId, OperationKind, RequestToken, ScrollAnchor, SortSpec, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Command(Command),
    Event(Event),
}

/// UI-neutral workspace requests. A gesture on a Browser's visible content
/// names the tab it observed, so a delayed gesture is rejected after that tab
/// stopped being active. `Navigate` and `OpenFavoriteItem` address the
/// Browser's active tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    StartOperation {
        browser: BrowserSide,
        tab: TabId,
        kind: OperationKind,
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
    ActivateBrowser {
        browser: BrowserSide,
    },
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
    OpenEntry {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    OperationScanned { id: OperationId, generation: u64, plan: Result<Arc<[PlannedItem]>, OperationFailure> },
    OperationStepped { id: OperationId, generation: u64, result: StepResult },
    OperationCleaned { id: OperationId, generation: u64, result: CleanupResult },
    OperationExecutorUnavailable { id: OperationId, generation: u64 },
    LocationInvalidated { location: Location },
    FolderItemsLoaded { browser: BrowserSide, tab: TabId, token: RequestToken, entries: Arc<[Entry]>, changes: Option<Vec<RowChange>> },
    FolderItemsFailed { browser: BrowserSide, tab: TabId, token: RequestToken, kind: ListingErrorKind },
    FolderItemsCancelled { browser: BrowserSide, tab: TabId, token: RequestToken },
    FavoriteTargetProbed { item_id: i64, target: Location, outcome: FavoriteProbeOutcome },
    ScreenshotsFolderProbed { location: Location, outcome: FavoriteProbeOutcome },
    SettingsSaved { revision: u64 },
    SettingsSaveFailed { revision: u64, failure: SettingsFailure },
    SettingsLoaded { snapshot: SettingsSnapshot },
    SettingsLoadFailed { failure: SettingsFailure },
    SettingsReset { backup: Option<Location> },
    SettingsResetFailed { failure: SettingsFailure },
}
impl From<Command> for Input {
    fn from(value: Command) -> Self {
        Self::Command(value)
    }
}
impl From<Event> for Input {
    fn from(value: Event) -> Self {
        Self::Event(value)
    }
}
