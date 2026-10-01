//! Qt-free input controllers and presenters.
//!
//! Adapters translate between the user interface and the application
//! boundary. They reshape data for a screen but make no file-manager
//! decisions of their own.

mod browser_presenter;
mod input_controller;

pub use browser_presenter::{BrowserPresenter, BrowserViewModel, FolderItemsUpdate, RowKind, RowViewModel, reader_start_failure_status};
pub use input_controller::{InputController, UiEvent};
