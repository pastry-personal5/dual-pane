//! Workspace state and use cases.
//!
//! [`Workspace::handle`] is a pure reducer: it validates one input, updates
//! the workspace, and returns application outputs and work requests. It never
//! performs I/O, starts threads, sorts or diffs listings, or waits.

mod actions;
mod clipboard;
mod context_menu;
mod filter;
mod input;
mod operations;
mod output;
mod preview;
mod projection;
mod settings;
mod work_request;
mod workspace;

pub use actions::{ActionSpec, FOLDER_ITEM_ACTION_COUNT, FolderItemSpec, MenuAvailability, MenuInvocation, MenuOperation, MenuTarget, ShortcutCategory, folder_item_spec, spec as action_spec};
pub use clipboard::{ClipboardAction, ClipboardGeneration, ClipboardOffer, ClipboardRejection, ClipboardRequestId, ClipboardSnapshot, TransferId};
pub use context_menu::{FolderItemsMenuAction, FolderItemsMenuSection, FolderItemsMenuSnapshot, FolderItemsMenuTarget, FolderItemsMenuToken};
pub use filter::{FilterFailure, FilterRequest, FilterResult, FilterStatus, project_filter};
pub use input::{Command, Event, Input, ResolvedTarget};
pub use operations::{ByteProgress, CleanupResult, FinalizeResult, NameProblem, OperationEffect, OperationErrorKind, OperationFailure, OperationJob, OperationOutcome, OperationProgress, OperationStatus, PendingDecision, PlannedItem, ScanFailure, ScanSkip, StepResult};
pub use output::{Output, RowChange, listing_changes};
pub use preview::{PreviewRequest, PreviewResult, PreviewView};
pub use projection::{BrowserChrome, FavoriteEdit, FavoriteRejection, MonitoringStatus, Notice, NoticeKind, TabSummary, WorkspaceChrome};
pub use settings::{ActionBinding, ActionId, ActionShortcuts, BrowserSnapshot, CONTEXT_MENU_ACTIONS, ContextMenuGroup, ContextMenuKind, ContextMenuPreference, ContextMenuPreferenceRejection, ContextMenuPreferences, FOLDER_SORT_LIMIT, FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoritesRecords, Key, LoadedSettings, SessionSnapshot, SettingsFailure, SettingsSnapshot, SettingsState, SettingsStatus, Shortcut, ShortcutRejection, ShortcutScope, TabSnapshot, VisibilityPolicy, WindowFrame, WindowLayout, WindowState, WorkspaceSnapshot, context_menu_group, default_bindings, default_shortcut, default_shortcut_lists, default_shortcuts, fresh_profile_favorites, fresh_profile_screenshots, validate_bindings};
pub use work_request::WorkRequest;
pub use workspace::{Transition, Workspace};
