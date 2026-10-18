//! Workspace state and use cases.
//!
//! [`Workspace::handle`] is a pure reducer: it validates one input, updates
//! the workspace, and returns application outputs and work requests. It never
//! performs I/O, starts threads, sorts or diffs listings, or waits.

mod clipboard;
mod filter;
mod input;
mod operations;
mod output;
mod preview;
mod projection;
mod settings;
mod work_request;
mod workspace;

pub use clipboard::{ClipboardAction, ClipboardGeneration, ClipboardRejection, ClipboardRequestId, ClipboardSnapshot, TransferId};
pub use filter::{FilterFailure, FilterRequest, FilterResult, FilterStatus, project_filter};
pub use input::{Command, Event, Input, ResolvedTarget};
pub use operations::{ByteProgress, CleanupResult, FinalizeResult, NameProblem, OperationEffect, OperationErrorKind, OperationFailure, OperationJob, OperationOutcome, OperationProgress, OperationStatus, PendingDecision, PlannedItem, ScanFailure, ScanSkip, StepResult};
pub use output::{Output, RowChange, listing_changes};
pub use preview::{PreviewRequest, PreviewResult, PreviewView};
pub use projection::{BrowserChrome, FavoriteEdit, FavoriteRejection, MonitoringStatus, Notice, NoticeKind, TabSummary, WorkspaceChrome};
pub use settings::{ActionBinding, ActionId, ActionShortcuts, BrowserSnapshot, FOLDER_SORT_LIMIT, FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoritesRecords, Key, LoadedSettings, SessionSnapshot, SettingsFailure, SettingsSnapshot, SettingsState, SettingsStatus, Shortcut, ShortcutRejection, ShortcutScope, TabSnapshot, VisibilityPolicy, WindowFrame, WindowLayout, WindowState, WorkspaceSnapshot, default_bindings, default_shortcut, default_shortcut_lists, default_shortcuts, fresh_profile_favorites, fresh_profile_screenshots, validate_bindings};
pub use work_request::WorkRequest;
pub use workspace::{Transition, Workspace};
