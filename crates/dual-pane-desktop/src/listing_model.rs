use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QModelIndex, QString, QVariant};
use dual_pane_adapters::{PaneViewModel, UiEvent, reader_start_failure_status};
use dual_pane_application::Command;

use crate::pane_session::{DRAIN_SLICE, DRAIN_TIME_BUDGET, PaneSession, PaneStartup, ViewChange};
use crate::runtime::Runtime;

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
pub mod ffi {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
    }

    extern "Rust" {
        type PaneStartup;
    }

    unsafe extern "C++" {
        include!("dual_pane_desktop/desktop_window.hpp");

        /// Runs the Qt application with one pane until its window closes, and
        /// returns the event loop's exit status.
        fn run_desktop(startup: Box<PaneStartup>) -> i32;
        /// Posts a C++-owned queued drain event when the model is still alive.
        fn schedule_gui_drain();
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qproperty(QString, status_text, cxx_name = "statusText", READ, NOTIFY)]
        #[qproperty(i32, selected_row, cxx_name = "selectedRow", READ, NOTIFY)]
        type ListingModel = super::ListingModelRust;

        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ListingModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &ListingModel, index: &QModelIndex, role: i32) -> QVariant;

        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut ListingModel>);

        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut ListingModel>);

        /// Starts the pane's worker and its first navigation.
        fn start(self: Pin<&mut ListingModel>, startup: Box<PaneStartup>);

        /// Drains one slice. The C++ scheduler calls this and schedules the
        /// next slice only after this RustQt call has returned.
        fn drain(self: Pin<&mut ListingModel>) -> bool;

        /// Submits one application-owned row selection.
        fn select_row(self: Pin<&mut ListingModel>, row: i32);

        /// Submits one application-owned row activation.
        fn activate_row(self: Pin<&mut ListingModel>, row: i32);

        /// Navigates to the logical parent of the shown location.
        fn go_to_parent(self: Pin<&mut ListingModel>);
    }

    impl cxx_qt::Threading for ListingModel {}
}

/// `Qt::DisplayRole`.
const DISPLAY_ROLE: i32 = 0;

/// The Rust state of the Qt list model for one pane.
pub struct ListingModelRust {
    status_text: QString,
    selected_row: i32,
    session: Option<PaneSession>,
    /// What the view shows. It changes only inside a model reset, as Qt
    /// requires, even though the session updates first.
    shown: PaneViewModel,
}

impl Default for ListingModelRust {
    fn default() -> Self {
        Self { status_text: QString::default(), selected_row: -1, session: None, shown: PaneViewModel::default() }
    }
}

impl ffi::ListingModel {
    fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() {
            return 0;
        }
        i32::try_from(self.rust().shown.row_count()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if role != DISPLAY_ROLE {
            return QVariant::default();
        }
        let row = usize::try_from(index.row()).ok().and_then(|row| self.rust().shown.row(row));
        row.map_or_else(QVariant::default, |row| QVariant::from(&QString::from(row.name.as_str())))
    }

    #[expect(clippy::boxed_local, reason = "CXX passes an opaque Rust value from C++ only in a Box")]
    fn start(mut self: Pin<&mut Self>, startup: Box<PaneStartup>) {
        let PaneStartup { location, source_factory } = *startup;
        let wake = Box::new(ffi::schedule_gui_drain);
        match Runtime::start(source_factory, wake) {
            Ok(runtime) => {
                let mut session = PaneSession::new(runtime, DRAIN_SLICE, DRAIN_TIME_BUDGET);
                let change = session.submit(Command::Navigate(location));
                self.as_mut().rust_mut().session = Some(session);
                self.apply(change);
            }
            Err(_error) => self.set_status_text(QString::from(reader_start_failure_status())),
        }
    }

    /// Handles one slice on the GUI thread. C++ owns rescheduling so no Qt
    /// work is posted while this RustQt call holds its shared lock.
    fn drain(mut self: Pin<&mut Self>) -> bool {
        let Some(session) = self.as_mut().rust_mut().get_mut().session.as_mut() else {
            return false;
        };
        let drained = session.drain();
        self.as_mut().apply(drained.change);
        drained.more_pending
    }

    fn select_row(mut self: Pin<&mut Self>, row: i32) {
        let Some(row) = usize::try_from(row).ok() else {
            return;
        };
        let Some(session) = self.as_mut().rust_mut().get_mut().session.as_mut() else {
            return;
        };
        let change = session.submit_ui(UiEvent::SelectRow { row });
        self.apply(change);
    }

    fn activate_row(mut self: Pin<&mut Self>, row: i32) {
        let Some(row) = usize::try_from(row).ok() else {
            return;
        };
        let Some(session) = self.as_mut().rust_mut().get_mut().session.as_mut() else {
            return;
        };
        let change = session.submit_ui(UiEvent::ActivateRow { row });
        self.apply(change);
    }

    fn go_to_parent(mut self: Pin<&mut Self>) {
        let Some(session) = self.as_mut().rust_mut().get_mut().session.as_mut() else {
            return;
        };
        let change = session.submit_ui(UiEvent::GoToParent);
        self.apply(change);
    }

    /// Notifies Qt of `change`, copying the session's view into `shown`.
    fn apply(mut self: Pin<&mut Self>, change: ViewChange) {
        let reset = match change {
            ViewChange::None => return,
            ViewChange::Status | ViewChange::Selection => false,
            ViewChange::Reset => true,
        };
        let Some(view) = self.rust().session.as_ref().map(|session| session.view().clone()) else {
            return;
        };
        if reset {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().get_mut().shown = view;
        if reset {
            self.as_mut().end_reset_model();
        }
        let selected_row = self.rust().shown.selected_row().and_then(|row| i32::try_from(row).ok()).unwrap_or(-1);
        self.as_mut().set_selected_row(selected_row);
        let text = self.rust().shown.status_text().to_owned();
        self.set_status_text(QString::from(text.as_str()));
    }

    fn set_selected_row(mut self: Pin<&mut Self>, row: i32) {
        if self.rust().selected_row != row {
            self.as_mut().rust_mut().get_mut().selected_row = row;
            self.selected_row_changed();
        }
    }

    fn set_status_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().status_text != text {
            self.as_mut().rust_mut().get_mut().status_text = text;
            self.status_text_changed();
        }
    }
}
