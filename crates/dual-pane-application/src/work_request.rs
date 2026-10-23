use std::sync::Arc;

use crate::{ClipboardAction, ClipboardGeneration, ClipboardRequestId, DefaultApplicationRequest, DropPreflight, FileAssociationRequest, FilterRequest, OpenWithRequest, OperationEffect, PreviewRequest, SettingsSnapshot, TransferId, VisibilityPolicy, WorkspaceSnapshot};
use dual_pane_domain::{BrowserSide, Entry, Location, OperationRoot, RequestToken, SortSpec, TabId};

/// Outside work the reducer needs, carried out by a runtime off the GUI thread.
///
/// `ReadDirectory` reads, sorts, and diffs one directory. Its `previous` is the
/// tab's shown Folder Items when the read reloads that same location; the
/// gateway reports its row change against them in `FolderItemsLoaded`.
/// `ProbeScreenshotsFolder` checks whether the optional fresh-profile
/// Screenshots folder exists before Favorites are seeded, and
/// `ProbeFavoriteTarget` checks a Favorite Item's target at launch.
/// `LoadSettings` and `ResetSettings` go to the settings service.
/// `ResolveItem` reports what a link points to, for activation, and
/// `ReopenJournal` retries opening the operation safety journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkRequest {
    ResolveFileAssociations {
        request: FileAssociationRequest,
    },
    OpenWithApplication {
        request: OpenWithRequest,
    },
    SetDefaultApplication {
        request: DefaultApplicationRequest,
    },
    PreflightDrop {
        preflight: DropPreflight,
    },
    PublishClipboard {
        request: ClipboardRequestId,
        transfer: TransferId,
        action: ClipboardAction,
        /// Finder-compatible source identities for Copy. Cut publishes only
        /// the private transfer reference.
        finder_roots: Option<Arc<[OperationRoot]>>,
    },
    ReadClipboard {
        request: ClipboardRequestId,
    },
    PreflightClipboard {
        request: ClipboardRequestId,
        generation: ClipboardGeneration,
        sources: Arc<[Location]>,
    },
    ProjectFilter {
        request: FilterRequest,
    },
    CancelFilter {
        browser: BrowserSide,
        tab: TabId,
    },
    ReadPreview {
        request: PreviewRequest,
    },
    CancelPreview {
        browser: BrowserSide,
        token: u64,
    },
    Operation(OperationEffect),
    /// Starts one shared subscription for a byte-exact open location. The
    /// generation rejects callbacks from a subscription that was replaced.
    WatchLocation {
        location: Location,
        generation: u64,
    },
    UnwatchLocation {
        location: Location,
        generation: u64,
    },
    /// Pauses periodic checks while native subscriptions remain installed.
    SetWatchActivity {
        active: bool,
    },
    /// Names the locations shown by the two active tabs so degraded checks
    /// can use the visible interval while inactive tabs stay sparse.
    UpdateWatchVisibility {
        visible: Vec<Location>,
    },
    /// Feeds periodic-check backoff without making the driver authoritative
    /// for the listing or its error.
    WatchReadCompleted {
        location: Location,
        succeeded: bool,
    },
    ReadDirectory {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        location: Location,
        sort: SortSpec,
        visibility: VisibilityPolicy,
        previous: Option<Arc<[Entry]>>,
    },
    Cancel {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
    },
    SaveSettings {
        revision: u64,
        snapshot: SettingsSnapshot,
    },
    SaveSession {
        revision: u64,
        session: WorkspaceSnapshot,
    },
    ProbeScreenshotsFolder {
        location: Location,
    },
    ProbeFavoriteTarget {
        item_id: i64,
        target: Location,
    },
    LoadSettings,
    ResetSettings,
    ResolveItem {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        item: Location,
    },
    ReopenJournal,
    /// Rebuilds an entered path from stored names on the Browser's read lanes.
    ResolvePath {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
        candidate: Location,
    },
    CancelPathResolve {
        browser: BrowserSide,
        tab: TabId,
        token: RequestToken,
    },
}
