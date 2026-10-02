//! Immutable projections of workspace state that presentation renders and
//! uses for command enablement. A new window and every later change render
//! from the same values.

use std::sync::Arc;

use crate::{ActionBinding, FavoritesRecords, OperationFailure, OperationOutcome, OperationProgress, SettingsFailure};
use dual_pane_domain::{BrowserSide, Location, OperationIntent, SortSpec, TabId};

/// One Browser Tab as its strip shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSummary {
    pub id: TabId,
    /// The confirmed location, or the pending target while the tab has never
    /// confirmed one.
    pub location: Option<Location>,
    pub loading: bool,
    pub failed: bool,
}

/// Everything about one Browser besides its rows and selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserChrome {
    pub tabs: Vec<TabSummary>,
    pub active_tab: TabId,
    /// Whether another tab fits under the per-Browser tab limit.
    pub can_open_tab: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub can_go_up: bool,
    pub can_refresh: bool,
    /// The active tab's confirmed location and its effective sort.
    pub location: Option<Location>,
    pub sort: Option<SortSpec>,
}

/// Workspace-wide state outside the Browsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceChrome {
    pub active_browser: BrowserSide,
    pub favorites: FavoritesRecords,
    /// Whether Favorites are known, so edits can be accepted.
    pub favorites_ready: bool,
    /// The effective shortcut of every catalogued action.
    pub bindings: Vec<ActionBinding>,
    /// Current-session Notices, oldest first.
    pub notices: Arc<[Notice]>,
}

/// The target of a Favorites edit, for reporting its rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteEdit {
    CreateGroup,
    Group(i64),
    CreateItem { group_id: i64 },
    Item(i64),
}

/// Why a Favorites edit changed nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteRejection {
    /// The name is empty or only whitespace.
    InvalidName,
    /// Another Group, or another Item in the same Group, has this exact name.
    DuplicateName,
    /// The Group or Item no longer exists.
    Stale,
    /// The edit would leave the hierarchy as it is.
    Unchanged,
    /// Favorites are still loading or waiting for fresh-profile seeding.
    NotReady,
}

/// A nonblocking current-session message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: u64,
    pub kind: NoticeKind,
    /// Whether the message offers Reset Settings.
    pub offers_reset: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeKind {
    SettingsLoadFailed {
        failure: SettingsFailure,
    },
    SettingsSaveFailed {
        failure: SettingsFailure,
    },
    SettingsReset {
        backup: Option<Location>,
    },
    SettingsResetFailed {
        failure: SettingsFailure,
    },
    FavoriteRemoved {
        name: String,
        target: Location,
    },
    FavoriteProbeFailed {
        name: String,
        target: Location,
    },
    /// A file operation's summary at its terminal outcome.
    OperationFinished {
        intent: OperationIntent,
        outcome: OperationOutcome,
        progress: OperationProgress,
        failure: Option<OperationFailure>,
    },
    /// Copy is disabled until the safety journal reopens; offers Try Again.
    JournalUnavailable,
    /// Temporaries left by earlier launches: how many were removed, and the
    /// folders where some could not be.
    TemporariesSwept {
        removed: usize,
        failed: Vec<Location>,
    },
    OpenFailed {
        item: Location,
    },
}
