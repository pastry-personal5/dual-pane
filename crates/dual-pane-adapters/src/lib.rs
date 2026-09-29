//! Qt-free input controllers and presenters.
//!
//! Adapters translate between the user interface and the application
//! boundary. They reshape data for a screen but make no file-manager
//! decisions of their own.

mod input_controller;
mod pane_presenter;

pub use input_controller::{InputController, UiEvent};
pub use pane_presenter::{PanePresenter, PaneViewModel, RowKind, RowViewModel};
