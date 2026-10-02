//! Workspace state and use cases.
//!
//! [`Workspace::handle`] is a pure reducer: it validates one input, updates
//! the workspace, and returns application outputs and work requests. It never
//! performs I/O, starts threads, sorts or diffs listings, or waits.

mod input;
mod operations;
mod output;
mod projection;
mod settings;
mod work_request;
mod workspace;

pub use input::{Command, Event, Input, ResolvedTarget};
pub use operations::{ByteProgress, CleanupResult, FinalizeResult, NameProblem, OperationEffect, OperationErrorKind, OperationFailure, OperationJob, OperationOutcome, OperationProgress, OperationStatus, PendingDecision, PlannedItem, ScanFailure, ScanSkip, StepResult};
pub use output::{Output, RowChange, listing_changes};
pub use projection::{BrowserChrome, FavoriteEdit, FavoriteRejection, Notice, NoticeKind, TabSummary, WorkspaceChrome};
pub use settings::{ActionBinding, ActionId, BrowserSnapshot, FOLDER_SORT_LIMIT, FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoritesRecords, Key, SettingsFailure, SettingsSnapshot, SettingsState, SettingsStatus, Shortcut, ShortcutScope, TabSnapshot, WorkspaceSnapshot, default_bindings, default_shortcut, fresh_profile_favorites, fresh_profile_screenshots, validate_bindings};
pub use work_request::WorkRequest;
pub use workspace::{Transition, Workspace};
