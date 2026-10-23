//! The Qt binder for one Browser: its multi-column Folder Items rows, tab
//! strip, toolbar state, and summary.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QList, QModelIndex, QString, QVariant};
use dual_pane_adapters::{BrowserViewModel, EditorOutcome, FolderItemsColumn, FolderItemsUpdate, SelectionMovement, UiEvent, action_label};
use dual_pane_application::{ClipboardAction, ContextMenuKind, FolderItemsMenuAction, FolderItemsMenuSnapshot, PreviewView};
use dual_pane_domain::{BrowserSide, OperationCommand, OperationId, SortDirection, SortField, SortSpec, TabId};

use crate::workspace_bridge::{key_sequence, revision, with_session};

mod columns;
mod context_menus;
mod drag_drop;
mod filter;
mod navigation;
mod operations;
mod path_entry;
mod preview;
mod selection;
mod status;
mod tabs;
#[cfg(test)]
mod tests;

use navigation::*;

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
pub mod ffi {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        include!(<QtCore/QAbstractTableModel>);
        type QAbstractTableModel;
    }
    #[namespace = "Qt"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qt.h");
        type Orientation = cxx_qt_lib::Orientation;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractTableModel]
        #[qproperty(QString, status_text, cxx_name = "statusText", READ, NOTIFY)]
        #[qproperty(QString, path_text, cxx_name = "pathText", READ, NOTIFY)]
        #[qproperty(i32, path_view_revision, cxx_name = "pathViewRevision", READ, NOTIFY)]
        #[qproperty(i32, column_revision, cxx_name = "columnRevision", READ, NOTIFY)]
        #[qproperty(QString, folder_name, cxx_name = "folderName", READ, NOTIFY)]
        #[qproperty(QString, summary_count_text, cxx_name = "summaryCountText", READ, NOTIFY)]
        #[qproperty(QString, summary_selected_text, cxx_name = "summarySelectedText", READ, NOTIFY)]
        #[qproperty(QString, summary_size_text, cxx_name = "summarySizeText", READ, NOTIFY)]
        #[qproperty(QString, filter_text, cxx_name = "filterText", READ, NOTIFY)]
        #[qproperty(i32, filter_status, cxx_name = "filterStatus", READ, NOTIFY)]
        #[qproperty(i32, filter_result_count, cxx_name = "filterResultCount", READ, NOTIFY)]
        #[qproperty(bool, no_filter_matches, cxx_name = "noFilterMatches", READ, NOTIFY)]
        #[qproperty(i32, cursor_row, cxx_name = "cursorRow", READ, NOTIFY)]
        #[qproperty(i32, selection_revision, cxx_name = "selectionRevision", READ, NOTIFY)]
        #[qproperty(i32, preview_revision, cxx_name = "previewRevision", READ, NOTIFY)]
        #[qproperty(i32, tabs_revision, cxx_name = "tabsRevision", READ, NOTIFY)]
        /// Available commands as bit flags; see the `TOOLBAR_*` constants.
        #[qproperty(i32, toolbar_state, cxx_name = "toolbarState", READ, NOTIFY)]
        /// The effective sort as `field * 2 + descending`, with fields Name,
        /// Type, Date, Size, or -1 before a folder is shown.
        #[qproperty(i32, sort_choice, cxx_name = "sortChoice", READ, NOTIFY)]
        /// Changes with each refused file command's Status Bar reason.
        #[qproperty(i32, status_token, cxx_name = "statusToken", READ, NOTIFY)]
        #[qproperty(bool, missing_folder, cxx_name = "missingFolder", READ, NOTIFY)]
        /// Changes whenever an inline name editor may open.
        #[qproperty(i32, editor_revision, cxx_name = "editorRevision", READ, NOTIFY)]
        /// The Path Edit Control's submission state: 0 idle, 1 checking, 2 failed.
        #[qproperty(i32, path_entry_state, cxx_name = "pathEntryState", READ, NOTIFY)]
        /// Why the last submission failed, or empty.
        #[qproperty(QString, path_error, cxx_name = "pathError", READ, NOTIFY)]
        /// Changes whenever the submission state does.
        #[qproperty(i32, path_entry_revision, cxx_name = "pathEntryRevision", READ, NOTIFY)]
        /// Counts accepted submissions, so the editor can tell success from cancellation.
        #[qproperty(i32, path_accepted_count, cxx_name = "pathAcceptedCount", READ, NOTIFY)]
        type FolderItemsListModel = super::FolderItemsListModelRust;

        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FolderItemsListModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        #[cxx_name = "columnCount"]
        fn column_count(self: &FolderItemsListModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &FolderItemsListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "headerData"]
        fn header_data(self: &FolderItemsListModel, section: i32, orientation: Orientation, role: i32) -> QVariant;
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

        /// Emitted after any call that may have changed what a binder shows.
        #[qsignal]
        #[cxx_name = "sessionChanged"]
        fn session_changed(self: Pin<&mut FolderItemsListModel>);
        /// Emitted after the row, filter text, count, and status are coherent.
        #[qsignal]
        #[cxx_name = "filterViewChanged"]
        fn filter_view_changed(self: Pin<&mut FolderItemsListModel>);

        #[cxx_name = "setRightBrowser"]
        fn set_right_browser(self: Pin<&mut FolderItemsListModel>);
        fn refresh(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "setPreviewTarget"]
        fn set_preview_target(self: Pin<&mut FolderItemsListModel>, width: i32, height: i32);
        #[cxx_name = "previewKind"]
        fn preview_kind(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "previewDetails"]
        fn preview_details(self: &FolderItemsListModel) -> QString;
        #[cxx_name = "previewImageWidth"]
        fn preview_image_width(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "previewImageHeight"]
        fn preview_image_height(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "previewImagePixels"]
        fn preview_image_pixels(self: &FolderItemsListModel) -> QByteArray;
        #[cxx_name = "activateBrowser"]
        fn activate_browser(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "activeTabId"]
        fn active_tab_id(self: &FolderItemsListModel) -> u64;
        #[cxx_name = "visibleRevision"]
        fn visible_revision(self: &FolderItemsListModel) -> u64;
        #[cxx_name = "browserSide"]
        fn browser_side(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "rowIsSelected"]
        fn row_is_selected(self: &FolderItemsListModel, row: i32) -> bool;
        #[cxx_name = "beginFileDrag"]
        fn begin_file_drag(self: Pin<&mut FolderItemsListModel>, tab: u64, visible_revision: u64) -> u64;
        #[cxx_name = "previewFileDrop"]
        fn preview_file_drop(self: Pin<&mut FolderItemsListModel>, drag: u64, source_browser: i32, target_kind: i32, target_tab: u64, row: i32, copy: bool) -> i32;
        #[cxx_name = "acceptFileDrop"]
        fn accept_file_drop(self: Pin<&mut FolderItemsListModel>, drag: u64, source_browser: i32, target_kind: i32, target_tab: u64, row: i32, copy: bool) -> bool;
        #[cxx_name = "cancelFileDrag"]
        fn cancel_file_drag(self: Pin<&mut FolderItemsListModel>, drag: u64);
        #[cxx_name = "editFilterText"]
        fn edit_filter_text(self: Pin<&mut FolderItemsListModel>, observed_tab: u64, text: &QString);
        /// Captures the exact location the Path Edit Control is filled with.
        #[cxx_name = "beginPathEntry"]
        fn begin_path_entry(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "breadcrumbCount"]
        fn breadcrumb_count(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "breadcrumbLabel"]
        fn breadcrumb_label(self: &FolderItemsListModel, index: i32) -> QString;
        #[cxx_name = "breadcrumbPath"]
        fn breadcrumb_path(self: &FolderItemsListModel, index: i32) -> QString;
        #[cxx_name = "navigateBreadcrumb"]
        fn navigate_breadcrumb(self: Pin<&mut FolderItemsListModel>, index: i32, observed_tab: u64, visible_revision: u64);
        #[cxx_name = "setVisibleColumns"]
        fn set_visible_columns(self: Pin<&mut FolderItemsListModel>, first: i32, count: i32, generation: u64);
        #[cxx_name = "closeColumns"]
        fn close_columns(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "columnState"]
        fn column_state(self: &FolderItemsListModel, column: i32) -> i32;
        #[cxx_name = "columnChoiceCount"]
        fn column_choice_count(self: &FolderItemsListModel, column: i32) -> i32;
        #[cxx_name = "columnChoiceLabel"]
        fn column_choice_label(self: &FolderItemsListModel, column: i32, row: i32) -> QString;
        #[cxx_name = "columnChoicePath"]
        fn column_choice_path(self: &FolderItemsListModel, column: i32, row: i32) -> QString;
        #[cxx_name = "columnChoiceSelected"]
        fn column_choice_selected(self: &FolderItemsListModel, column: i32, row: i32) -> bool;
        #[cxx_name = "columnHasMore"]
        fn column_has_more(self: &FolderItemsListModel, column: i32) -> bool;
        #[cxx_name = "columnLoadMore"]
        fn column_load_more(self: Pin<&mut FolderItemsListModel>, column: i32);
        #[cxx_name = "navigateColumnChoice"]
        fn navigate_column_choice(self: Pin<&mut FolderItemsListModel>, column: i32, row: i32, observed_tab: u64, visible_revision: u64, generation: u64);
        #[cxx_name = "submitPath"]
        fn submit_path(self: Pin<&mut FolderItemsListModel>, observed_tab: u64, text: &QString, edited: bool);
        #[cxx_name = "cancelPathEntry"]
        fn cancel_path_entry(self: Pin<&mut FolderItemsListModel>, observed_tab: u64);
        #[cxx_name = "discardPathEntry"]
        fn discard_path_entry(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "startFileCommandObserved"]
        fn start_file_command_observed(self: Pin<&mut FolderItemsListModel>, command: i32, tab: u64, visible_revision: u64);
        /// Clipboard command: 0 Copy, 1 Cut, 2 Paste.
        #[cxx_name = "clipboardCommandAvailable"]
        fn clipboard_command_available(self: &FolderItemsListModel, command: i32) -> bool;
        #[cxx_name = "startClipboardCommandObserved"]
        fn start_clipboard_command_observed(self: Pin<&mut FolderItemsListModel>, command: i32, tab: u64, visible_revision: u64);
        #[cxx_name = "observeClipboard"]
        fn observe_clipboard(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "showPackageContentsObserved"]
        fn show_package_contents_observed(self: Pin<&mut FolderItemsListModel>, row: i32, tab: u64, visible_revision: u64);
        #[cxx_name = "requestContextMenu"]
        fn request_context_menu(self: Pin<&mut FolderItemsListModel>, kind: i32, row: i32) -> bool;
        #[cxx_name = "contextMenuActionCount"]
        fn context_menu_action_count(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "contextMenuActionId"]
        fn context_menu_action_id(self: &FolderItemsListModel, index: i32) -> i32;
        #[cxx_name = "contextMenuActionLabel"]
        fn context_menu_action_label(self: &FolderItemsListModel, index: i32) -> QString;
        #[cxx_name = "contextMenuActionShortcut"]
        fn context_menu_action_shortcut(self: &FolderItemsListModel, index: i32) -> QString;
        #[cxx_name = "contextMenuActionStartsSection"]
        fn context_menu_action_starts_section(self: &FolderItemsListModel, index: i32) -> bool;
        #[cxx_name = "contextMenuKind"]
        fn context_menu_kind(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "invokeContextMenuAction"]
        fn invoke_context_menu_action(self: Pin<&mut FolderItemsListModel>, index: i32);
        #[cxx_name = "dismissContextMenu"]
        fn dismiss_context_menu(self: Pin<&mut FolderItemsListModel>);

        #[cxx_name = "selectRow"]
        fn select_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        #[cxx_name = "toggleRow"]
        fn toggle_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        #[cxx_name = "extendToRow"]
        fn extend_to_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        #[cxx_name = "secondaryRow"]
        fn secondary_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        #[cxx_name = "activateRow"]
        fn activate_row(self: Pin<&mut FolderItemsListModel>, row: i32);
        /// Moves the cursor: `movement` is 0 previous, 1 next, 2 first,
        /// 3 last, 4 page up, 5 page down; `page` is the fully visible row
        /// count. `extend` extends the range from the anchor instead.
        #[cxx_name = "moveCursor"]
        fn move_cursor(self: Pin<&mut FolderItemsListModel>, movement: i32, page: i32, extend: bool);
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "activateSelected"]
        fn activate_selected(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "clearSelection"]
        fn clear_selection(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "goToParent"]
        fn go_to_parent(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "goBack"]
        fn go_back(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "goForward"]
        fn go_forward(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "refreshFolder"]
        fn refresh_folder(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "newTab"]
        fn new_tab(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "closeActiveTab"]
        fn close_active_tab(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "activateTab"]
        fn activate_tab(self: Pin<&mut FolderItemsListModel>, tab: u64);
        #[cxx_name = "closeTab"]
        fn close_tab(self: Pin<&mut FolderItemsListModel>, tab: u64);
        #[cxx_name = "reorderTab"]
        fn reorder_tab(self: Pin<&mut FolderItemsListModel>, tab: u64, position: i32);
        #[cxx_name = "setSort"]
        fn set_sort(self: Pin<&mut FolderItemsListModel>, choice: i32);
        /// Records that `row` is the first visible row, `offset` pixels above
        /// the viewport's top, or that no row is visible when `row` is -1.
        #[cxx_name = "updateScroll"]
        fn update_scroll(self: Pin<&mut FolderItemsListModel>, row: i32, offset: i32);

        #[cxx_name = "tabCount"]
        fn tab_count(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "tabId"]
        fn tab_id(self: &FolderItemsListModel, index: i32) -> u64;
        #[cxx_name = "tabLabel"]
        fn tab_label(self: &FolderItemsListModel, index: i32) -> QString;
        #[cxx_name = "tabPath"]
        fn tab_path(self: &FolderItemsListModel, index: i32) -> QString;
        #[cxx_name = "tabIsActive"]
        fn tab_is_active(self: &FolderItemsListModel, index: i32) -> bool;
        /// The selected rows as inclusive `first, last` pairs in ascending order.
        #[cxx_name = "selectedRanges"]
        fn selected_ranges(self: &FolderItemsListModel) -> QList_i32;
        /// The row the shown scroll hint names, or -1.
        #[cxx_name = "scrollHintRow"]
        fn scroll_hint_row(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "scrollHintOffset"]
        fn scroll_hint_offset(self: &FolderItemsListModel) -> i32;

        /// Whether a file command could start now. `command` is 0 Copy,
        /// 1 Move, 2 Rename, 3 New Folder, 4 Move to Trash, and 5 Delete
        /// Permanently.
        #[cxx_name = "fileCommandAvailable"]
        fn file_command_available(self: &FolderItemsListModel, command: i32) -> bool;
        /// Starts a file command; Rename and New Folder ask to open their
        /// editor. A refusal shows in the Status Bar.
        #[cxx_name = "startFileCommand"]
        fn start_file_command(self: Pin<&mut FolderItemsListModel>, command: i32);
        /// Clears the Status Bar reason `token` named, if it still shows.
        #[cxx_name = "expireStatus"]
        fn expire_status(self: Pin<&mut FolderItemsListModel>, token: i32);
        /// The editor that may open: its command code, the renamed row or -1
        /// for New Folder, and its starting text.
        #[cxx_name = "editorCommand"]
        fn editor_command(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "editorRow"]
        fn editor_row(self: &FolderItemsListModel) -> i32;
        #[cxx_name = "editorText"]
        fn editor_text(self: &FolderItemsListModel) -> QString;
        /// Commits an editor's name, closing the job `previous` refused. Returns
        /// the new job, -1 when the editor closes, or -2 with `nameError`.
        #[cxx_name = "commitName"]
        fn commit_name(self: Pin<&mut FolderItemsListModel>, command: i32, text: &QString, previous: i64) -> i64;
        #[cxx_name = "nameError"]
        fn name_error(self: &FolderItemsListModel) -> QString;
        /// What an editor waiting on `job` shows: 0 pending, 1 refused with
        /// `editorOutcomeText`, 2 done, 3 closed.
        #[cxx_name = "editorOutcome"]
        fn editor_outcome(self: &FolderItemsListModel, job: i64) -> i32;
        #[cxx_name = "editorOutcomeText"]
        fn editor_outcome_text(self: &FolderItemsListModel, job: i64) -> QString;
        /// Closes the job whose name the person abandoned.
        #[cxx_name = "cancelOperation"]
        fn cancel_operation(self: Pin<&mut FolderItemsListModel>, job: i64);
        #[cxx_name = "rowIsPackage"]
        fn row_is_package(self: &FolderItemsListModel, row: i32) -> bool;
        #[cxx_name = "showPackageContents"]
        fn show_package_contents(self: Pin<&mut FolderItemsListModel>, row: i32);
    }
}

/// The file command a Qt `command` code names.
pub fn operation_command(code: i32) -> Option<OperationCommand> {
    usize::try_from(code).ok().and_then(|code| OperationCommand::ALL.get(code)).copied()
}

/// The job a Qt `i64` names, or `None` for a negative value.
pub fn operation_id(job: i64) -> Option<OperationId> {
    u64::try_from(job).ok().map(OperationId::new)
}

const DISPLAY_ROLE: i32 = 0;
const TOOL_TIP_ROLE: i32 = 3;
const ACCESSIBLE_TEXT_ROLE: i32 = 11;
const TEXT_ALIGNMENT_ROLE: i32 = 7;
/// `Qt::UserRole`: the Item's full path, which keys its native icon.
const PATH_ROLE: i32 = 0x0100;
/// `Qt::UserRole + 1`: Relative Date age as millionths of strip position.
const RELATIVE_AGE_ROLE: i32 = 0x0101;
/// `Qt::UserRole + 2`: the Item's byte-exact native path for Quick Look.
const NATIVE_PATH_ROLE: i32 = 0x0102;
/// `Qt::UserRole + 3`: whether that native path names a directory.
const NATIVE_DIRECTORY_ROLE: i32 = 0x0103;
/// `Qt::UserRole + 4`: whether the row is a captured pending Cut root.
const PENDING_CUT_ROLE: i32 = 0x0104;
/// `Qt::AlignRight | Qt::AlignVCenter`.
const ALIGN_RIGHT: i32 = 0x0002 | 0x0080;

/// `toolbarState` flags.
pub const TOOLBAR_NEW_TAB: i32 = 1;
pub const TOOLBAR_BACK: i32 = 2;
pub const TOOLBAR_FORWARD: i32 = 4;
pub const TOOLBAR_UP: i32 = 8;
pub const TOOLBAR_REFRESH: i32 = 16;

/// Converts an inclusive row range to Qt's `int` rows.
fn qt_rows((first, last): (usize, usize)) -> Result<(i32, i32), std::num::TryFromIntError> {
    Ok((i32::try_from(first)?, i32::try_from(last)?))
}

fn qt_int(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub struct FolderItemsListModelRust {
    status_text: QString,
    path_text: QString,
    path_view_revision: i32,
    column_revision: i32,
    folder_name: QString,
    summary_count_text: QString,
    summary_selected_text: QString,
    summary_size_text: QString,
    filter_text: QString,
    filter_status: i32,
    filter_result_count: i32,
    no_filter_matches: bool,
    cursor_row: i32,
    selection_revision: i32,
    preview_revision: i32,
    tabs_revision: i32,
    toolbar_state: i32,
    sort_choice: i32,
    status_token: i32,
    missing_folder: bool,
    editor_revision: i32,
    name_error: QString,
    path_entry_state: i32,
    path_error: QString,
    path_entry_revision: i32,
    path_accepted_count: i32,
    /// The location the Path Edit Control was filled with when editing began.
    path_origin: Option<dual_pane_domain::Location>,
    column_generation: Option<u64>,
    column_source: Option<dual_pane_domain::Location>,
    column_tab: u64,
    column_visible_revision: u64,
    columns: Vec<Option<columns::ColumnData>>,
    browser: BrowserSide,
    shown: BrowserViewModel,
    context_menu: Option<FolderItemsMenuSnapshot>,
}
impl Default for FolderItemsListModelRust {
    fn default() -> Self {
        Self { status_text: QString::default(), path_text: QString::default(), path_view_revision: 0, column_revision: 0, folder_name: QString::default(), summary_count_text: QString::default(), summary_selected_text: QString::default(), summary_size_text: QString::default(), filter_text: QString::default(), filter_status: 0, filter_result_count: 0, no_filter_matches: false, cursor_row: -1, selection_revision: 0, preview_revision: 0, tabs_revision: 0, toolbar_state: 0, sort_choice: -1, status_token: 0, missing_folder: false, editor_revision: 0, name_error: QString::default(), path_entry_state: 0, path_error: QString::default(), path_entry_revision: 0, path_accepted_count: 0, path_origin: None, column_generation: None, column_source: None, column_tab: 0, column_visible_revision: 0, columns: Vec::new(), browser: BrowserSide::Left, shown: BrowserViewModel::default(), context_menu: None }
    }
}

impl ffi::FolderItemsListModel {
    fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { qt_int(self.rust().shown.row_count()) }
    }
    fn column_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() { 0 } else { qt_int(FolderItemsColumn::ALL.len()) }
    }
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let (Ok(row), Some(column)) = (usize::try_from(index.row()), usize::try_from(index.column()).ok().and_then(FolderItemsColumn::from_index)) else { return QVariant::default() };
        let shown = &self.rust().shown;
        match role {
            DISPLAY_ROLE if column != FolderItemsColumn::Icon => shown.cell_text(row, column).map_or_else(QVariant::default, |text| QVariant::from(&QString::from(text.as_str()))),
            ACCESSIBLE_TEXT_ROLE if column != FolderItemsColumn::Icon => shown.accessible_cell_text(row, column).map_or_else(QVariant::default, |text| QVariant::from(&QString::from(text.as_str()))),
            TOOL_TIP_ROLE if column == FolderItemsColumn::Name => shown.cell_text(row, column).map_or_else(QVariant::default, |text| QVariant::from(&QString::from(text.as_str()))),
            PATH_ROLE => shown.row_path(row).map_or_else(QVariant::default, |path| QVariant::from(&QString::from(path.as_str()))),
            RELATIVE_AGE_ROLE if column == FolderItemsColumn::RelativeDate => shown.relative_age_position(row).map_or_else(QVariant::default, |position| QVariant::from(&((position * 1_000_000.0).round() as i32))),
            NATIVE_PATH_ROLE => shown.row_native_path(row).map_or_else(QVariant::default, |(path, _)| QVariant::from(&QByteArray::from(path.as_slice()))),
            NATIVE_DIRECTORY_ROLE => shown.row_native_path(row).map_or_else(QVariant::default, |(_, directory)| QVariant::from(&directory)),
            PENDING_CUT_ROLE => QVariant::from(&shown.row_pending_cut(row)),
            TEXT_ALIGNMENT_ROLE if column == FolderItemsColumn::Size => QVariant::from(&ALIGN_RIGHT),
            _ => QVariant::default(),
        }
    }
    /// Column names stay available to accessibility although the view hides
    /// its header row.
    fn header_data(&self, section: i32, orientation: ffi::Orientation, role: i32) -> QVariant {
        if orientation != ffi::Orientation::Horizontal || !matches!(role, DISPLAY_ROLE | ACCESSIBLE_TEXT_ROLE) {
            return QVariant::default();
        }
        usize::try_from(section).ok().and_then(FolderItemsColumn::from_index).map_or_else(QVariant::default, |column| QVariant::from(&QString::from(column.title())))
    }
    fn set_right_browser(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().browser = BrowserSide::Right;
        self.as_mut().refresh();
    }
    fn refresh(mut self: Pin<&mut Self>) {
        let browser = self.rust().browser;
        let Some(view) = with_session(|session| session.view(browser).clone()) else { return };
        // The session lock is released, so Qt may call back into the model
        // while these notifications are delivered.
        self.as_mut().show(view);
        self.as_mut().refresh_column_results();
        let shown = &self.rust().shown;
        let cursor = shown.cursor_row().and_then(|row| i32::try_from(row).ok()).unwrap_or(-1);
        let selection = revision(shown.selection_revision());
        let preview = revision(shown.preview_revision());
        let tabs = revision(shown.tabs_revision());
        let toolbar = shown.toolbar();
        let flags = [(toolbar.can_open_tab, TOOLBAR_NEW_TAB), (toolbar.can_go_back, TOOLBAR_BACK), (toolbar.can_go_forward, TOOLBAR_FORWARD), (toolbar.can_go_up, TOOLBAR_UP), (toolbar.can_refresh, TOOLBAR_REFRESH)].into_iter().filter(|(on, _)| *on).fold(0, |flags, (_, flag)| flags | flag);
        let sort = sort_choice(toolbar.sort);
        let status = QString::from(shown.status_text());
        let path = QString::from(shown.location_text());
        let path_revision = revision(shown.visible_revision());
        let folder_name = QString::from(shown.folder_name());
        let summary = shown.summary();
        let (count, selected, size) = (QString::from(summary.count.as_str()), QString::from(summary.selected.as_str()), QString::from(summary.size.as_str()));
        let filter_text = QString::from(shown.filter_text());
        let filter_status = match shown.filter_status() {
            dual_pane_application::FilterStatus::Ready => 0,
            dual_pane_application::FilterStatus::Updating => 1,
            dual_pane_application::FilterStatus::Failed => 2,
        };
        let filter_result_count = qt_int(shown.row_count());
        let no_filter_matches = shown.no_matches();
        self.as_mut().set_cursor_row(cursor);
        self.as_mut().set_selection_revision(selection);
        self.as_mut().set_preview_revision(preview);
        self.as_mut().set_status_text(status);
        self.as_mut().set_path_text(path);
        self.as_mut().set_path_view_revision(path_revision);
        self.as_mut().set_folder_name(folder_name);
        self.as_mut().set_summary_count_text(count);
        self.as_mut().set_summary_selected_text(selected);
        self.as_mut().set_summary_size_text(size);
        self.as_mut().set_filter_text(filter_text);
        self.as_mut().set_filter_status(filter_status);
        self.as_mut().set_filter_result_count(filter_result_count);
        self.as_mut().set_no_filter_matches(no_filter_matches);
        self.as_mut().set_toolbar_state(flags);
        self.as_mut().set_sort_choice(sort);
        self.as_mut().set_tabs_revision(tabs);
        let (token, missing, editor) = (revision(self.rust().shown.status_token()), self.rust().shown.missing_folder(), revision(self.rust().shown.editor_revision()));
        self.as_mut().set_status_token(token);
        self.as_mut().set_missing_folder(missing);
        self.as_mut().set_editor_revision(editor);
        let entry = self.rust().shown.path_entry();
        let (state, error, entry_revision, accepted) = (
            if entry.is_pending() {
                1
            } else if entry.error().is_some() {
                2
            } else {
                0
            },
            QString::from(entry.error().unwrap_or_default()),
            revision(entry.revision()),
            revision(entry.accepted()),
        );
        self.as_mut().set_path_entry_state(state);
        self.as_mut().set_path_error(error);
        self.as_mut().set_path_accepted_count(accepted);
        self.as_mut().set_path_entry_revision(entry_revision);
        self.as_mut().filter_view_changed();
    }
    /// Replaces the shown view, notifying Qt of only the rows that changed
    /// when the new listing is a same-folder reload of the shown one.
    fn show(mut self: Pin<&mut Self>, view: BrowserViewModel) {
        let repaint_relative_dates = view.relative_dates_changed_from(&self.rust().shown);
        let update = view.update_from(&self.rust().shown);
        let FolderItemsUpdate::Rows { changed, inserted, removed } = update else {
            let reset = update == FolderItemsUpdate::Reset;
            if reset {
                self.as_mut().begin_reset_model();
            }
            self.as_mut().rust_mut().get_mut().shown = view;
            if reset {
                self.as_mut().end_reset_model();
            } else if repaint_relative_dates {
                self.as_mut().repaint_relative_dates();
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
            let bottom_right = self.index(last, qt_int(FolderItemsColumn::ALL.len() - 1), &root);
            self.as_mut().data_changed(&top_left, &bottom_right, &QList::default());
        }
        if repaint_relative_dates {
            self.as_mut().repaint_relative_dates();
        }
    }
    fn repaint_relative_dates(mut self: Pin<&mut Self>) {
        let Some(last) = self.rust().shown.row_count().checked_sub(1) else { return };
        let root = QModelIndex::default();
        let column = FolderItemsColumn::RelativeDate as i32;
        let top_left = self.index(0, column, &root);
        let bottom_right = self.index(qt_int(last), column, &root);
        self.as_mut().data_changed(&top_left, &bottom_right, &QList::default());
    }
    fn activate_browser(mut self: Pin<&mut Self>) {
        self.as_mut().submit_ui(UiEvent::FocusBrowser);
    }
    fn active_tab_id(&self) -> u64 {
        self.rust().shown.active_tab().map_or(0, dual_pane_domain::TabId::value)
    }
    fn visible_revision(&self) -> u64 {
        self.rust().shown.visible_revision()
    }
    fn browser_side(&self) -> i32 {
        if self.rust().browser == BrowserSide::Left { 0 } else { 1 }
    }
    fn submit_ui(mut self: Pin<&mut Self>, event: UiEvent) {
        let browser = self.rust().browser;
        let shown = self.rust().shown.clone();
        with_session(|session| session.submit_observed_ui(browser, event, shown));
        self.as_mut().session_changed();
    }
}
