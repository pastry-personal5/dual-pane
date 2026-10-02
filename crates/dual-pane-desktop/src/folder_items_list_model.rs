//! The Qt binder for one Browser: its multi-column Folder Items rows, tab
//! strip, toolbar state, and summary.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QModelIndex, QString, QVariant};
use dual_pane_adapters::{BrowserViewModel, EditorOutcome, FolderItemsColumn, FolderItemsUpdate, SelectionMovement, UiEvent};
use dual_pane_domain::{BrowserSide, OperationCommand, OperationId, SortDirection, SortField, SortSpec, TabId};

use crate::workspace_bridge::{revision, with_session};

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
        #[qproperty(QString, folder_name, cxx_name = "folderName", READ, NOTIFY)]
        #[qproperty(QString, summary_count_text, cxx_name = "summaryCountText", READ, NOTIFY)]
        #[qproperty(QString, summary_selected_text, cxx_name = "summarySelectedText", READ, NOTIFY)]
        #[qproperty(QString, summary_size_text, cxx_name = "summarySizeText", READ, NOTIFY)]
        #[qproperty(i32, cursor_row, cxx_name = "cursorRow", READ, NOTIFY)]
        #[qproperty(i32, selection_revision, cxx_name = "selectionRevision", READ, NOTIFY)]
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

        #[cxx_name = "setRightBrowser"]
        fn set_right_browser(self: Pin<&mut FolderItemsListModel>);
        fn refresh(self: Pin<&mut FolderItemsListModel>);
        #[cxx_name = "activateBrowser"]
        fn activate_browser(self: Pin<&mut FolderItemsListModel>);

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

/// The Qt code for `command`.
fn command_code(command: OperationCommand) -> i32 {
    OperationCommand::ALL.iter().position(|candidate| *candidate == command).map_or(-1, qt_int)
}

/// The job a Qt `i64` names, or `None` for a negative value.
pub fn operation_id(job: i64) -> Option<OperationId> {
    u64::try_from(job).ok().map(OperationId::new)
}

fn job_code(id: OperationId) -> i64 {
    i64::try_from(id.get()).unwrap_or(-1)
}

const DISPLAY_ROLE: i32 = 0;
const TOOL_TIP_ROLE: i32 = 3;
const ACCESSIBLE_TEXT_ROLE: i32 = 11;
const TEXT_ALIGNMENT_ROLE: i32 = 7;
/// `Qt::UserRole`: the Item's full path, which keys its native icon.
const PATH_ROLE: i32 = 0x0100;
/// `Qt::UserRole + 1`: Relative Date age as millionths of strip position.
const RELATIVE_AGE_ROLE: i32 = 0x0101;
/// `Qt::AlignRight | Qt::AlignVCenter`.
const ALIGN_RIGHT: i32 = 0x0002 | 0x0080;

/// `toolbarState` flags.
pub const TOOLBAR_NEW_TAB: i32 = 1;
pub const TOOLBAR_BACK: i32 = 2;
pub const TOOLBAR_FORWARD: i32 = 4;
pub const TOOLBAR_UP: i32 = 8;
pub const TOOLBAR_REFRESH: i32 = 16;

const SORT_FIELDS: [SortField; 4] = [SortField::Name, SortField::Type, SortField::Modified, SortField::Size];

/// The `sortChoice` code for `sort`.
fn sort_choice(sort: Option<SortSpec>) -> i32 {
    sort.and_then(|sort| SORT_FIELDS.iter().position(|field| *field == sort.field()).map(|field| field as i32 * 2 + i32::from(sort.direction() == SortDirection::Descending))).unwrap_or(-1)
}

/// The sort a `sortChoice` code names.
fn sort_from_choice(choice: i32) -> Option<SortSpec> {
    if choice < 0 {
        return None;
    }
    let field = SORT_FIELDS.get(usize::try_from(choice / 2).ok()?)?;
    Some(SortSpec::new(*field, if choice % 2 == 0 { SortDirection::Ascending } else { SortDirection::Descending }))
}

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
    folder_name: QString,
    summary_count_text: QString,
    summary_selected_text: QString,
    summary_size_text: QString,
    cursor_row: i32,
    selection_revision: i32,
    tabs_revision: i32,
    toolbar_state: i32,
    sort_choice: i32,
    status_token: i32,
    missing_folder: bool,
    editor_revision: i32,
    name_error: QString,
    browser: BrowserSide,
    shown: BrowserViewModel,
}
impl Default for FolderItemsListModelRust {
    fn default() -> Self {
        Self { status_text: QString::default(), path_text: QString::default(), folder_name: QString::default(), summary_count_text: QString::default(), summary_selected_text: QString::default(), summary_size_text: QString::default(), cursor_row: -1, selection_revision: 0, tabs_revision: 0, toolbar_state: 0, sort_choice: -1, status_token: 0, missing_folder: false, editor_revision: 0, name_error: QString::default(), browser: BrowserSide::Left, shown: BrowserViewModel::default() }
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
            DISPLAY_ROLE | ACCESSIBLE_TEXT_ROLE if column != FolderItemsColumn::Icon => shown.cell_text(row, column).map_or_else(QVariant::default, |text| QVariant::from(&QString::from(text.as_str()))),
            TOOL_TIP_ROLE if column == FolderItemsColumn::Name => shown.cell_text(row, column).map_or_else(QVariant::default, |text| QVariant::from(&QString::from(text.as_str()))),
            PATH_ROLE => shown.row_path(row).map_or_else(QVariant::default, |path| QVariant::from(&QString::from(path.as_str()))),
            RELATIVE_AGE_ROLE if column == FolderItemsColumn::RelativeDate => shown.relative_age_position(row).map_or_else(QVariant::default, |position| QVariant::from(&((position * 1_000_000.0).round() as i32))),
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
        let shown = &self.rust().shown;
        let cursor = shown.cursor_row().and_then(|row| i32::try_from(row).ok()).unwrap_or(-1);
        let selection = revision(shown.selection_revision());
        let tabs = revision(shown.tabs_revision());
        let toolbar = shown.toolbar();
        let flags = [(toolbar.can_open_tab, TOOLBAR_NEW_TAB), (toolbar.can_go_back, TOOLBAR_BACK), (toolbar.can_go_forward, TOOLBAR_FORWARD), (toolbar.can_go_up, TOOLBAR_UP), (toolbar.can_refresh, TOOLBAR_REFRESH)].into_iter().filter(|(on, _)| *on).fold(0, |flags, (_, flag)| flags | flag);
        let sort = sort_choice(toolbar.sort);
        let status = QString::from(shown.status_text());
        let path = QString::from(shown.location_text());
        let folder_name = QString::from(shown.folder_name());
        let summary = shown.summary();
        let (count, selected, size) = (QString::from(summary.count.as_str()), QString::from(summary.selected.as_str()), QString::from(summary.size.as_str()));
        self.as_mut().set_cursor_row(cursor);
        self.as_mut().set_selection_revision(selection);
        self.as_mut().set_status_text(status);
        self.as_mut().set_path_text(path);
        self.as_mut().set_folder_name(folder_name);
        self.as_mut().set_summary_count_text(count);
        self.as_mut().set_summary_selected_text(selected);
        self.as_mut().set_summary_size_text(size);
        self.as_mut().set_toolbar_state(flags);
        self.as_mut().set_sort_choice(sort);
        self.as_mut().set_tabs_revision(tabs);
        let (token, missing, editor) = (revision(self.rust().shown.status_token()), self.rust().shown.missing_folder(), revision(self.rust().shown.editor_revision()));
        self.as_mut().set_status_token(token);
        self.as_mut().set_missing_folder(missing);
        self.as_mut().set_editor_revision(editor);
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
    fn select_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::SelectRow { row });
    }
    fn toggle_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ToggleRow { row });
    }
    fn extend_to_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ExtendToRow { row });
    }
    fn secondary_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::SecondaryRow { row });
    }
    fn activate_row(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ActivateRow { row });
    }
    fn row_event(self: Pin<&mut Self>, row: i32, event: impl FnOnce(usize) -> UiEvent) {
        if let Ok(row) = usize::try_from(row) {
            self.submit_ui(event(row));
        }
    }
    fn move_cursor(self: Pin<&mut Self>, movement: i32, page: i32, extend: bool) {
        let rows = usize::try_from(page).unwrap_or(0);
        let movement = match movement {
            0 => SelectionMovement::Previous,
            1 => SelectionMovement::Next,
            2 => SelectionMovement::First,
            3 => SelectionMovement::Last,
            4 => SelectionMovement::PageUp { rows },
            5 => SelectionMovement::PageDown { rows },
            _ => return,
        };
        self.submit_ui(if extend { UiEvent::ExtendSelection(movement) } else { UiEvent::MoveSelection(movement) });
    }
    fn select_all(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::SelectAll);
    }
    fn activate_selected(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::ActivateSelection);
    }
    fn clear_selection(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::ClearSelection);
    }
    fn go_to_parent(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::GoToParent);
    }
    fn go_back(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::GoBack);
    }
    fn go_forward(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::GoForward);
    }
    fn refresh_folder(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::Refresh);
    }
    fn new_tab(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::NewTab);
    }
    fn close_active_tab(self: Pin<&mut Self>) {
        self.submit_ui(UiEvent::CloseActiveTab);
    }
    fn activate_tab(self: Pin<&mut Self>, tab: u64) {
        self.submit_ui(UiEvent::ActivateTab { tab: TabId::new(tab) });
    }
    fn close_tab(self: Pin<&mut Self>, tab: u64) {
        self.submit_ui(UiEvent::CloseTab { tab: TabId::new(tab) });
    }
    fn reorder_tab(self: Pin<&mut Self>, tab: u64, position: i32) {
        if let Ok(position) = usize::try_from(position) {
            self.submit_ui(UiEvent::ReorderTab { tab: TabId::new(tab), position });
        }
    }
    fn set_sort(self: Pin<&mut Self>, choice: i32) {
        if let Some(sort) = sort_from_choice(choice) {
            self.submit_ui(UiEvent::Sort(sort));
        }
    }
    /// Scrolling changes no view model and does not activate the Browser,
    /// so nothing is refreshed.
    fn update_scroll(self: Pin<&mut Self>, row: i32, offset: i32) {
        let browser = self.rust().browser;
        with_session(|session| session.submit_view_state(browser, UiEvent::Scrolled { row: usize::try_from(row).ok(), offset }));
    }
    fn submit_ui(mut self: Pin<&mut Self>, event: UiEvent) {
        let browser = self.rust().browser;
        with_session(|session| session.submit_ui(browser, event));
        self.as_mut().session_changed();
    }

    fn file_command_available(&self, command: i32) -> bool {
        let browser = self.rust().browser;
        operation_command(command).and_then(|command| with_session(|session| session.file_command_available(browser, command))).unwrap_or(false)
    }
    fn start_file_command(self: Pin<&mut Self>, command: i32) {
        if let Some(command) = operation_command(command) {
            self.submit_ui(UiEvent::FileCommand(command));
        }
    }
    fn expire_status(mut self: Pin<&mut Self>, token: i32) {
        let browser = self.rust().browser;
        // The token is the view model's counter truncated like every revision.
        let current = self.rust().shown.status_token();
        if revision(current) == token {
            with_session(|session| session.expire_status(browser, current));
            self.as_mut().session_changed();
        }
    }
    fn editor_command(&self) -> i32 {
        self.rust().shown.editor().map_or(-1, |editor| command_code(editor.command))
    }
    fn editor_row(&self) -> i32 {
        self.rust().shown.editor().and_then(|editor| editor.row).map_or(-1, qt_int)
    }
    fn editor_text(&self) -> QString {
        self.rust().shown.editor().map_or_else(QString::default, |editor| QString::from(editor.text.as_str()))
    }
    fn commit_name(mut self: Pin<&mut Self>, command: i32, text: &QString, previous: i64) -> i64 {
        let browser = self.rust().browser;
        let Some(command) = operation_command(command) else { return -1 };
        let text = text.to_string();
        let result = with_session(|session| session.commit_name(browser, command, &text, operation_id(previous))).unwrap_or(Ok(None));
        self.as_mut().session_changed();
        match result {
            Ok(Some(job)) => job_code(job),
            Ok(None) => -1,
            Err(error) => {
                self.as_mut().rust_mut().get_mut().name_error = QString::from(error);
                -2
            }
        }
    }
    fn name_error(&self) -> QString {
        self.rust().name_error.clone()
    }
    fn editor_outcome(&self, job: i64) -> i32 {
        match operation_id(job).and_then(|job| with_session(|session| session.editor_outcome(job))) {
            Some(EditorOutcome::Pending) => 0,
            Some(EditorOutcome::Rejected(_)) => 1,
            Some(EditorOutcome::Succeeded) => 2,
            Some(EditorOutcome::Closed) | None => 3,
        }
    }
    fn editor_outcome_text(&self, job: i64) -> QString {
        match operation_id(job).and_then(|job| with_session(|session| session.editor_outcome(job))) {
            Some(EditorOutcome::Rejected(text)) => QString::from(text),
            _ => QString::default(),
        }
    }
    fn cancel_operation(mut self: Pin<&mut Self>, job: i64) {
        if let Some(id) = operation_id(job) {
            with_session(|session| session.submit(dual_pane_application::Command::CancelOperation { id }));
            self.as_mut().session_changed();
        }
    }
    fn row_is_package(&self, row: i32) -> bool {
        usize::try_from(row).is_ok_and(|row| self.rust().shown.is_package(row))
    }
    fn show_package_contents(self: Pin<&mut Self>, row: i32) {
        self.row_event(row, |row| UiEvent::ShowPackageContents { row });
    }

    fn set_status_token(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().status_token != value {
            self.as_mut().rust_mut().get_mut().status_token = value;
            self.status_token_changed();
        }
    }
    fn set_missing_folder(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().missing_folder != value {
            self.as_mut().rust_mut().get_mut().missing_folder = value;
            self.missing_folder_changed();
        }
    }
    fn set_editor_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().editor_revision != value {
            self.as_mut().rust_mut().get_mut().editor_revision = value;
            self.editor_revision_changed();
        }
    }

    fn tab(&self, index: i32) -> Option<&dual_pane_adapters::TabViewModel> {
        usize::try_from(index).ok().and_then(|index| self.rust().shown.tabs().get(index))
    }
    fn tab_count(&self) -> i32 {
        qt_int(self.rust().shown.tabs().len())
    }
    fn tab_id(&self, index: i32) -> u64 {
        self.tab(index).map_or(u64::MAX, |tab| tab.id.value())
    }
    fn tab_label(&self, index: i32) -> QString {
        self.tab(index).map_or_else(QString::default, |tab| QString::from(tab.label.as_str()))
    }
    fn tab_path(&self, index: i32) -> QString {
        self.tab(index).map_or_else(QString::default, |tab| QString::from(tab.path.as_str()))
    }
    fn tab_is_active(&self, index: i32) -> bool {
        self.tab(index).is_some_and(|tab| tab.active)
    }
    fn selected_ranges(&self) -> QList<i32> {
        let mut ranges = QList::default();
        let rows = self.rust().shown.selected_rows();
        let mut rows = rows.into_iter().filter_map(|row| i32::try_from(row).ok()).peekable();
        while let Some(first) = rows.next() {
            let mut last = first;
            while rows.peek() == Some(&(last + 1)) {
                last += 1;
                rows.next();
            }
            ranges.append(first);
            ranges.append(last);
        }
        ranges
    }
    fn scroll_hint_row(&self) -> i32 {
        self.rust().shown.scroll_hint_row().and_then(|(row, _)| i32::try_from(row).ok()).unwrap_or(-1)
    }
    fn scroll_hint_offset(&self) -> i32 {
        self.rust().shown.scroll_hint_row().map_or(0, |(_, offset)| offset)
    }

    fn set_cursor_row(mut self: Pin<&mut Self>, row: i32) {
        if self.rust().cursor_row != row {
            self.as_mut().rust_mut().get_mut().cursor_row = row;
            self.cursor_row_changed();
        }
    }
    fn set_selection_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().selection_revision != value {
            self.as_mut().rust_mut().get_mut().selection_revision = value;
            self.selection_revision_changed();
        }
    }
    fn set_tabs_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().tabs_revision != value {
            self.as_mut().rust_mut().get_mut().tabs_revision = value;
            self.tabs_revision_changed();
        }
    }
    fn set_toolbar_state(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().toolbar_state != value {
            self.as_mut().rust_mut().get_mut().toolbar_state = value;
            self.toolbar_state_changed();
        }
    }
    fn set_sort_choice(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().sort_choice != value {
            self.as_mut().rust_mut().get_mut().sort_choice = value;
            self.sort_choice_changed();
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
    fn set_summary_count_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().summary_count_text != text {
            self.as_mut().rust_mut().get_mut().summary_count_text = text;
            self.summary_count_text_changed();
        }
    }
    fn set_summary_selected_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().summary_selected_text != text {
            self.as_mut().rust_mut().get_mut().summary_selected_text = text;
            self.summary_selected_text_changed();
        }
    }
    fn set_summary_size_text(mut self: Pin<&mut Self>, text: QString) {
        if self.rust().summary_size_text != text {
            self.as_mut().rust_mut().get_mut().summary_size_text = text;
            self.summary_size_text_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_choices_round_trip_every_direct_sort_button() {
        for choice in 0..8 {
            assert_eq!(sort_choice(sort_from_choice(choice)), choice);
        }
        assert_eq!(sort_choice(None), -1);
        assert_eq!(sort_from_choice(-1), None);
        assert_eq!(sort_from_choice(8), None);
        assert_eq!(sort_from_choice(5), Some(SortSpec::new(SortField::Modified, SortDirection::Descending)));
    }
}
