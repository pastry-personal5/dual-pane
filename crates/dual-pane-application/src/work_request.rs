use std::sync::Arc;

use crate::{OperationEffect, SettingsSnapshot};
use dual_pane_domain::{BrowserSide, Entry, Location, RequestToken, SortSpec, TabId};

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
    Operation(OperationEffect),
    ReadDirectory { browser: BrowserSide, tab: TabId, token: RequestToken, location: Location, sort: SortSpec, previous: Option<Arc<[Entry]>> },
    Cancel { browser: BrowserSide, tab: TabId, token: RequestToken },
    SaveSettings { revision: u64, snapshot: SettingsSnapshot },
    ProbeScreenshotsFolder { location: Location },
    ProbeFavoriteTarget { item_id: i64, target: Location },
    LoadSettings,
    ResetSettings,
    ResolveItem { browser: BrowserSide, tab: TabId, token: RequestToken, item: Location },
    ReopenJournal,
}
