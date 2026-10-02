//! Qt-free input controllers and presenters.
//!
//! Adapters translate between the user interface and the application
//! boundary. They reshape data for a screen but make no file-manager
//! decisions of their own.

mod browser_presenter;
mod format;
mod input_controller;
mod workspace_presenter;

pub use browser_presenter::{BrowserPresenter, BrowserViewModel, Clock, FolderItemsColumn, FolderItemsUpdate, RowKind, RowViewModel, Summary, TabViewModel, Toolbar, reader_start_failure_status};
pub use format::{UNAVAILABLE, format_exact, format_relative, format_size, location_text};
pub use input_controller::{FavoritesEvent, GroupMotion, InputController, SelectionMovement, UiEvent};
pub use workspace_presenter::{FavoriteGroupViewModel, FavoriteItemViewModel, NoticeViewModel, WorkspacePresenter, WorkspaceViewModel, favorite_rejection_text};
