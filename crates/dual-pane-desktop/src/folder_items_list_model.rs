use std::pin::Pin;
use std::sync::{Mutex, OnceLock};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QModelIndex, QString, QVariant};
use dual_pane_adapters::{BrowserViewModel, FolderItemsUpdate, SelectionMovement, UiEvent, reader_start_failure_status};
use dual_pane_domain::BrowserSide;

use crate::browser_session::{BrowserStartup, DRAIN_SLICE, DRAIN_TIME_BUDGET, SHUTDOWN_TIMEOUT, WorkspaceSession};
use crate::runtime::Runtime;
use crate::settings_storage::SettingsWorker;

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
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
    }
    extern "Rust" {
        type BrowserStartup;
        /// Flushes settings after the Qt event loop exits; see [`super::shutdown_desktop`].
        fn shutdown_desktop();
    }
    unsafe extern "C++" {
        include!("dual_pane_desktop/desktop_window.hpp");
        fn run_desktop(startup: Box<BrowserStartup>) -> i32;
        fn schedule_gui_drain();
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qproperty(QString, status_text, cxx_name = "statusText", READ, NOTIFY)]
        #[qproperty(QString, path_text, cxx_name = "pathText", READ, NOTIFY)]
        #[qproperty(QString, folder_name, cxx_name = "folderName", READ, NOTIFY)]
        #[qproperty(i32, selected_row, cxx_name = "selectedRow", READ, NOTIFY)]
        type FolderItemsListModel = super::FolderItemsListModelRust;
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FolderItemsListModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &FolderItemsListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut FolderItemsListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut FolderItemsListModel>);
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        fn begin_insert_rows(self: Pin<&mut FolderItemsListModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        fn end_insert_rows(self: Pin<&mut FolderItemsListModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        fn begin_remove_rows(self: Pin<&mut FolderItemsListModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        fn end_remove_rows(self: Pin<&mut FolderItemsListModel>);
        #[inherit]
        fn index(self: &FolderItemsListModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut FolderItemsListModel>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);
        fn start(self: Pin<&mut FolderItemsListModel>, startup: Box<BrowserStartup>);
        fn set_right_browser(self: Pin<&mut FolderItemsListModel>);
        fn refresh(self: Pin<&mut FolderItemsListModel>);
        fn drain(self: Pin<&mut FolderItemsListModel>) -> bool;
        fn activate_browser(self: Pin<&mut FolderItemsListModel>);
        fn select_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        fn select_previous(self: Pin<&mut FolderItemsListModel>);
        fn select_next(self: Pin<&mut FolderItemsListModel>);
        fn select_first(self: Pin<&mut FolderItemsListModel>);
        fn select_last(self: Pin<&mut FolderItemsListModel>);
        /// Moves the selection up by `rows`, the fully visible row count.
        fn select_page_up(self: Pin<&mut FolderItemsListModel>, rows: i32);
        /// Moves the selection down by `rows`, the fully visible row count.
        fn select_page_down(self: Pin<&mut FolderItemsListModel>, rows: i32);
        fn clear_selection(self: Pin<&mut FolderItemsListModel>);
        fn activate_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        fn activate_selected(self: Pin<&mut FolderItemsListModel>);
        fn go_to_parent(self: Pin<&mut FolderItemsListModel>);
    }
    impl cxx_qt::Threading for FolderItemsListModel {}
}

const DISPLAY_ROLE: i32 = 0;
/// The one session both Browser models share. Only the GUI thread uses it, so
/// the mutex is a holder rather than synchronization; its order comes from the
/// GUI thread's event loop. Each caller releases it before notifying Qt, which
/// may call back into a model.
static SESSION: OnceLock<Mutex<Option<WorkspaceSession>>> = OnceLock::new();
fn session() -> &'static Mutex<Option<WorkspaceSession>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

/// Ends the session after the Qt event loop has exited: unsaved settings get
/// a bounded flush, and outstanding reads are cancelled.
fn shutdown_desktop() {
    let session = session().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    if let Some(session) = session {
        session.shutdown(SHUTDOWN_TIMEOUT);
    }
}

/// Converts an inclusive row range to Qt's `int` rows.
fn qt_rows((first, last): (usize, usize)) -> Result<(i32, i32), std::num::TryFromIntError> {
    Ok((i32::try_from(first)?, i32::try_from(last)?))
}

pub struct FolderItemsListModelRust {
    status_text: QString,
    path_text: QString,
    folder_name: QString,
    selected_row: i32,
    browser: BrowserSide,
    shown: BrowserViewModel,
}
impl Default for FolderItemsListModelRust {
    fn default() -> Self {
        Self { status_text: QString::default(), path_text: QString::default(), folder_name: QString::default(), selected_row: -1, browser: BrowserSide::Left, shown: BrowserViewModel::default() }
    }
}

impl ffi::FolderItemsListModel {
    fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { i32::try_from(self.rust().shown.row_count()).unwrap_or(i32::MAX) }
    }
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if role != DISPLAY_ROLE {
            return QVariant::default();
        }
        usize::try_from(index.row()).ok().and_then(|row| self.rust().shown.row(row)).map_or_else(QVariant::default, |row| QVariant::from(&QString::from(row.name.as_str())))
    }
    #[expect(clippy::boxed_local, reason = "CXX passes an opaque Rust value from C++ only in a Box")]
    fn start(mut self: Pin<&mut Self>, startup: Box<BrowserStartup>) {
        let BrowserStartup { location, home, settings_path, source_factory, location_probe } = *startup;
        match Runtime::start(source_factory, location_probe, Box::new(ffi::schedule_gui_drain)) {
            Ok(runtime) => {
                let mut guard = session().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let settings = settings_path.and_then(|path| SettingsWorker::start_with_wake(path, Box::new(ffi::schedule_gui_drain)).ok());
                let mut coordinator = WorkspaceSession::with_settings(runtime, home, settings, DRAIN_SLICE, DRAIN_TIME_BUDGET);
                coordinator.start(location);
                *guard = Some(coordinator);
                drop(guard);
                self.as_mut().refresh();
            }
            Err(_) => self.set_status_text(QString::from(reader_start_failure_status())),
        }
    }
    fn set_right_browser(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().browser = BrowserSide::Right;
        self.as_mut().refresh();
    }
    fn refresh(mut self: Pin<&mut Self>) {
        let browser = self.rust().browser;
        let view = session().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|coordinator| coordinator.view(browser).clone());
        let Some(view) = view else {
            return;
        };
        // The session lock is released, so Qt may call back into the model
        // while these notifications are delivered.
        self.as_mut().show(view);
        let selected = self.rust().shown.selected_row().and_then(|row| i32::try_from(row).ok()).unwrap_or(-1);
        self.as_mut().set_selected_row(selected);
        let status = self.rust().shown.status_text().to_owned();
        self.as_mut().set_status_text(QString::from(status.as_str()));
        let path = QString::from(self.rust().shown.location_text());
        self.as_mut().set_path_text(path);
        let folder_name = QString::from(self.rust().shown.folder_name());
        self.as_mut().set_folder_name(folder_name);
    }
    /// Replaces the shown view, notifying Qt of only the rows that changed
    /// when the new listing is a same-folder reload of the shown one.
    fn show(mut self: Pin<&mut Self>, view: BrowserViewModel) {
        let update = view.update_from(&self.rust().shown);
        let FolderItemsUpdate::Rows { changed, inserted, removed } = update else {
            let reset = update == FolderItemsUpdate::Reset;
            if reset {
                self.as_mut().begin_reset_model();
            }
            self.as_mut().rust_mut().get_mut().shown = view;
            if reset {
                self.as_mut().end_reset_model();
            }
            return;
        };
        let (Ok(changed), Ok(inserted), Ok(removed)) = (changed.map(qt_rows).transpose(), inserted.map(qt_rows).transpose(), removed.map(qt_rows).transpose()) else {
            // Rows beyond Qt's `int` range cannot be addressed individually.
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().get_mut().shown = view;
            self.as_mut().end_reset_model();
            return;
        };
        let root = QModelIndex::default();
        if let Some((first, last)) = inserted {
            self.as_mut().begin_insert_rows(&root, first, last);
            self.as_mut().rust_mut().get_mut().shown = view;
            self.as_mut().end_insert_rows();
        } else if let Some((first, last)) = removed {
            self.as_mut().begin_remove_rows(&root, first, last);
            self.as_mut().rust_mut().get_mut().shown = view;
            self.as_mut().end_remove_rows();
        } else {
            self.as_mut().rust_mut().get_mut().shown = view;
        }
        if let Some((first, last)) = changed {
            let top_left = self.index(first, 0, &root);
            let bottom_right = self.index(last, 0, &root);
            self.as_mut().data_changed(&top_left, &bottom_right, &QList::default());
        }
    }
    fn drain(mut self: Pin<&mut Self>) -> bool {
        let more = session().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_mut().is_some_and(WorkspaceSession::drain);
        self.as_mut().refresh();
        more
    }
    /// Activation changes no Browser view model, so nothing is refreshed.
    fn activate_browser(self: Pin<&mut Self>) {
        let browser = self.rust().browser;
        if let Some(coordinator) = session().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_mut() {
            coordinator.submit_ui(browser, UiEvent::FocusBrowser);
        }
    }
    fn select_row(mut self: Pin<&mut Self>, row: i32) {
        if let Ok(row) = usize::try_from(row) {
            self.as_mut().submit_ui(UiEvent::SelectRow { row });
        }
    }
    fn select_previous(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::MoveSelection(SelectionMovement::Previous));
    }
    fn select_next(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::MoveSelection(SelectionMovement::Next));
    }
    fn select_first(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::MoveSelection(SelectionMovement::First));
    }
    fn select_last(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::MoveSelection(SelectionMovement::Last));
    }
    fn select_page_up(mut self: Pin<&mut Self>, rows: i32) {
        let rows = usize::try_from(rows).unwrap_or(0);
        self.as_mut().submit_ui(UiEvent::MoveSelection(SelectionMovement::PageUp { rows }));
    }
    fn select_page_down(mut self: Pin<&mut Self>, rows: i32) {
        let rows = usize::try_from(rows).unwrap_or(0);
        self.as_mut().submit_ui(UiEvent::MoveSelection(SelectionMovement::PageDown { rows }));
    }
    fn clear_selection(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::ClearSelection);
    }
    fn activate_row(mut self: Pin<&mut Self>, row: i32) {
        if let Ok(row) = usize::try_from(row) {
            self.as_mut().submit_ui(UiEvent::ActivateRow { row });
        }
    }
    fn activate_selected(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::ActivateSelection);
    }
    fn go_to_parent(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::GoToParent);
    }
    fn submit_ui(mut self: Pin<&mut Self>, event: UiEvent) {
        let browser = self.rust().browser;
        if let Some(coordinator) = session().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_mut() {
            coordinator.submit_ui(browser, event);
        }
        self.as_mut().refresh();
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
    fn set_path_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().path_text != text {
            self.as_mut().rust_mut().get_mut().path_text = text;
            self.path_text_changed();
        }
    }
    fn set_folder_name(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().folder_name != text {
            self.as_mut().rust_mut().get_mut().folder_name = text;
            self.folder_name_changed();
        }
    }
}
