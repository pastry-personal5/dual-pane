//! Qt-free input controllers and presenters.
//!
//! Adapters translate between the user interface and the application
//! boundary. They reshape data for a screen but make no file-manager
//! decisions of their own.

mod browser_presenter;
mod format;
mod input_controller;
mod operation_panels;
mod operation_presenter;
mod workspace_presenter;

pub use browser_presenter::{BrowserPresenter, BrowserViewModel, Clock, EditorRequest, FolderItemsColumn, FolderItemsUpdate, RowKind, RowViewModel, Summary, TabViewModel, Toolbar, reader_start_failure_status};
pub use format::{UNAVAILABLE, format_exact, format_relative, format_size, location_text};
pub use input_controller::{FavoritesEvent, GroupMotion, InputController, SelectionMovement, UiEvent};
pub use operation_panels::{ChoiceViewModel, DecisionCardViewModel, EditorOutcome, HIDE_DELAY, OperationPanelViewModel, OperationsPresenter, OperationsViewModel, REVEAL_DELAY, Ticks};
pub use operation_presenter::{UNREPRESENTABLE_NAME_TEXT, name_problem_text, name_rejection_text, operation_error_text, operation_summary, operation_title, rejection_text};
pub use workspace_presenter::{FavoriteGroupViewModel, FavoriteItemViewModel, NoticeViewModel, WorkspacePresenter, WorkspaceViewModel, favorite_rejection_text};
