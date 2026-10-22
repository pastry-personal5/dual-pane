use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dual_pane_application::{BrowserChrome, ClipboardAction, ClipboardRejection, FilterStatus, MonitoringStatus, Output, PathEntryFailure, PathRejection, PreviewView, RowChange};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingError, ListingErrorKind, Location, OperationCommand, ScrollAnchor, Selection, SortSpec, TabId, entry_type_text};
use jiff::tz::TimeZone;

use crate::format::{UNAVAILABLE, format_exact, format_relative, format_size, item_count, location_text, relative_age_position, total_size};
use crate::operation_presenter::rejection_text;

/// The current time in Unix seconds, read when a listing arrives so relative
/// dates change on reload rather than by timer.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The Folder Pane summary: the item count and selection count at the left,
/// and the selected-over-total or folder size at the right.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub count: String,
    pub selected: String,
    pub size: String,
}

/// The Path Edit Control's submission state for the shown tab.
///
/// `error` stays until `discard_path_entry` runs, which the desktop calls when
/// the text is edited, Escape is pressed, or focus leaves. Every change bumps
/// `revision`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathEntryView {
    tab: Option<TabId>,
    entered: String,
    pending: bool,
    error: Option<String>,
    revision: u64,
    accepted: u64,
}

impl PathEntryView {
    /// The tab the state belongs to.
    pub fn tab(&self) -> Option<TabId> {
        self.tab
    }
    /// True while the submitted path is being checked.
    pub fn is_pending(&self) -> bool {
        self.pending
    }
    /// Why the last submission failed, as safe display text.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// How many submissions this Browser has accepted. The editor watches
    /// it to know that editing ended in success.
    pub fn accepted(&self) -> u64 {
        self.accepted
    }
}

/// What a Browser shows. Rows are formatted when requested, so updating the view
/// model costs the same for any directory size.
#[derive(Debug, Clone)]
pub struct BrowserViewModel {
    location: Option<Location>,
    location_text: String,
    /// The root volume's name, or empty when it could not be read.
    root_label: String,
    loading: bool,
    error: Option<String>,
    /// The loading, error, or path text the Status Bar normally shows.
    base_status: String,
    status_text: String,
    /// A refused file command's reason, shown until its token expires.
    status_message: Option<String>,
    status_token: u64,
    monitoring: Option<MonitoringStatus>,
    missing_folder: bool,
    editor: Option<EditorRequest>,
    editor_revision: u64,
    path_entry: PathEntryView,
    entries: Arc<[Entry]>,
    filter_text: String,
    filter_status: FilterStatus,
    total_count: usize,
    visible_revision: u64,
    selection: Selection,
    cursor_row: Option<usize>,
    selection_revision: u64,
    preview: PreviewView,
    preview_revision: u64,
    summary: Summary,
    scroll_hint: Option<ScrollAnchor>,
    folder_items_revision: u64,
    /// How this revision's rows differ from the previous revision's, or
    /// `None` when every row must be treated as replaced.
    folder_items_delta: Option<Vec<RowChange>>,
    /// When the shown listing arrived, in Unix seconds.
    listed_at: i64,
    time_zone: TimeZone,
    active_tab: Option<TabId>,
    tabs: Vec<TabViewModel>,
    tabs_revision: u64,
    toolbar: Toolbar,
    pending_cut: Arc<[dual_pane_domain::OperationRoot]>,
    /// The captured Cut roots by parent folder, then exact name, so matching
    /// a row allocates nothing and a folder that holds none is skipped at once.
    pending_cut_kinds: HashMap<Location, HashMap<EntryName, EntryKind>>,
}

impl Default for BrowserViewModel {
    fn default() -> Self {
        Self { location: None, location_text: String::new(), root_label: String::new(), loading: false, error: None, base_status: String::new(), status_text: String::new(), status_message: None, status_token: 0, monitoring: None, missing_folder: false, editor: None, editor_revision: 0, path_entry: PathEntryView::default(), entries: Arc::from([]), filter_text: String::new(), filter_status: FilterStatus::Ready, total_count: 0, visible_revision: 0, selection: Selection::default(), cursor_row: None, selection_revision: 0, preview: PreviewView::Collapsed, preview_revision: 0, summary: Summary::default(), scroll_hint: None, folder_items_revision: 0, folder_items_delta: None, listed_at: 0, time_zone: TimeZone::UTC, active_tab: None, tabs: Vec::new(), tabs_revision: 0, toolbar: Toolbar::default(), pending_cut: Arc::from([]), pending_cut_kinds: HashMap::new() }
    }
}

/// Command availability and the effective sort, from application state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Toolbar {
    pub can_open_tab: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub can_go_up: bool,
    pub can_refresh: bool,
    /// The active tab's effective sort, or `None` before it shows a folder.
    pub sort: Option<SortSpec>,
}

/// An inline name editor the application allowed to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorRequest {
    /// Rename or New Folder.
    pub command: OperationCommand,
    /// The renamed Item's row; `None` for New Folder's empty row at the top.
    pub row: Option<usize>,
    /// The text the editor starts with.
    pub text: String,
}

/// One Browser Tab in its strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabViewModel {
    pub id: TabId,
    /// The folder name, or the volume name for the root.
    pub label: String,
    /// The full path for the tooltip and accessibility description.
    pub path: String,
    pub active: bool,
    pub loading: bool,
    pub failed: bool,
}

/// How a list showing one revision of a Browser's Folder Items moves to a
/// newer one. Row ranges are inclusive `(first, last)` pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderItemsUpdate {
    Unchanged,
    /// Replace every row.
    Reset,
    /// Rows `changed` keep their position but show new content. Then either
    /// `inserted` rows are added or `removed` rows are taken away.
    Rows {
        changed: Option<(usize, usize)>,
        inserted: Option<(usize, usize)>,
        removed: Option<(usize, usize)>,
    },
}

/// The Folder Items columns, from left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderItemsColumn {
    Icon,
    Name,
    Type,
    RelativeDate,
    ExactDate,
    Size,
}

impl FolderItemsColumn {
    pub const ALL: [Self; 6] = [Self::Icon, Self::Name, Self::Type, Self::RelativeDate, Self::ExactDate, Self::Size];

    /// The column's canonical name, for accessibility.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Icon => "Item Icon Column",
            Self::Name => "Name",
            Self::Type => "Type",
            Self::RelativeDate => "Relative Date Column",
            Self::ExactDate => "Exact Date Column",
            Self::Size => "Size",
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// One displayed row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowViewModel {
    pub name: String,
    pub kind: RowKind,
}

/// The kind of row, for choosing an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Folder,
    File,
    FolderLink,
    Link,
    Other,
}

impl BrowserViewModel {
    /// The shown location as a path, or empty before the first listing.
    pub fn location_text(&self) -> &str {
        &self.location_text
    }

    /// The confirmed location the rows belong to.
    pub fn location(&self) -> Option<&Location> {
        self.location.as_ref()
    }

    /// The shown folder's own name for titles, tab labels, and new Favorite
    /// aliases: the volume name for the root, or `/` when that is unknown and
    /// before the first listing.
    pub fn folder_name(&self) -> &str {
        match &self.location {
            Some(location) if !location.is_root() => self.location_text.rsplit('/').find(|part| !part.is_empty()).unwrap_or("/"),
            _ if self.location.is_some() && !self.root_label.is_empty() => &self.root_label,
            _ => "/",
        }
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// A message for the most recent failure, until the next navigation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Fully formatted status text for the desktop shell.
    pub fn status_text(&self) -> &str {
        &self.status_text
    }

    /// Identifies the Status Bar's current refusal reason, so only its own
    /// timer can clear it.
    pub fn status_token(&self) -> u64 {
        self.status_token
    }

    /// Whether the shown folder was renamed, moved, or removed, so its last
    /// rows are dimmed under the Missing Folder Overlay.
    pub fn missing_folder(&self) -> bool {
        self.missing_folder
    }

    /// The latest inline editor the application allowed to open.
    pub fn editor(&self) -> Option<&EditorRequest> {
        self.editor.as_ref()
    }

    /// Changes whenever an inline editor is allowed to open.
    pub fn path_entry(&self) -> &PathEntryView {
        &self.path_entry
    }

    pub fn editor_revision(&self) -> u64 {
        self.editor_revision
    }

    /// Whether the Item in `row` is a macOS package.
    pub fn is_package(&self, row: usize) -> bool {
        self.entries.get(row).is_some_and(Entry::is_package)
    }

    /// Folder Pane Toolbar Row #2: the count, any selection count, and the
    /// non-recursive size total of the selection or else the whole folder.
    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    pub fn row_count(&self) -> usize {
        self.entries.len()
    }
    /// The byte-exact identity of a row displayed in this projection.
    pub fn row_entry_name(&self, row: usize) -> Option<EntryName> {
        self.entry(row).map(|entry| entry.name().clone())
    }
    pub fn filter_text(&self) -> &str {
        &self.filter_text
    }
    pub fn filter_status(&self) -> FilterStatus {
        self.filter_status
    }
    pub fn total_count(&self) -> usize {
        self.total_count
    }
    pub fn no_matches(&self) -> bool {
        !self.filter_text.is_empty() && self.filter_status == FilterStatus::Ready && self.entries.is_empty() && self.total_count > 0
    }
    pub fn visible_revision(&self) -> u64 {
        self.visible_revision
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// The cursor's row, which keyboard movement starts from even when it is
    /// not selected.
    pub fn cursor_row(&self) -> Option<usize> {
        self.cursor_row
    }

    /// Changes whenever the selection or cursor changes.
    pub fn selection_revision(&self) -> u64 {
        self.selection_revision
    }

    pub fn preview(&self) -> &PreviewView {
        &self.preview
    }

    pub fn preview_revision(&self) -> u64 {
        self.preview_revision
    }

    pub fn preview_details_text(&self) -> String {
        let name = match &self.preview {
            PreviewView::Collapsed => return String::new(),
            PreviewView::NoSelection => return "Select one item to see its details.".to_owned(),
            PreviewView::MultipleSelection => return "Multiple items selected. Select one item to see its details.".to_owned(),
            PreviewView::UnsupportedSelection { name } | PreviewView::Loading { name } | PreviewView::Unavailable { name } | PreviewView::DecodeFailed { name } | PreviewView::Image { name, .. } => name,
        };
        let entry = self.entries.iter().find(|entry| entry.name() == name);
        let kind = entry.map(entry_type_text).unwrap_or_else(|| "—".to_owned());
        let (size, modified) = match &self.preview {
            PreviewView::Image { byte_size, modified_unix_seconds, .. } => (Some(*byte_size), *modified_unix_seconds),
            _ => entry.map_or((None, None), |entry| (entry.metadata().size_bytes(), entry.metadata().modified_unix_seconds())),
        };
        let size = size.map(format_size).unwrap_or_else(|| "—".to_owned());
        let modified = modified.map(|seconds| format_exact(seconds, &self.time_zone)).unwrap_or_else(|| "—".to_owned());
        let mut details = format!("Name: {}\nType: {kind}\nModified: {modified}\nSize: {size}", name.to_text_lossy());
        match &self.preview {
            PreviewView::Image { source_width, source_height, .. } => details.push_str(&format!("\nDimensions: {source_width} × {source_height}")),
            PreviewView::Loading { .. } => details.push_str("\nLoading image…"),
            PreviewView::UnsupportedSelection { .. } => details.push_str("\nPreview unavailable for this item."),
            PreviewView::Unavailable { .. } => details.push_str("\nThis item is unavailable."),
            PreviewView::DecodeFailed { .. } => details.push_str("\nThe image could not be read."),
            PreviewView::Collapsed | PreviewView::NoSelection | PreviewView::MultipleSelection => {}
        }
        details
    }

    /// The selected rows in ascending order.
    pub fn selected_rows(&self) -> Vec<usize> {
        match self.selection.entries() {
            [] => Vec::new(),
            [only] => self.entries.iter().position(|entry| entry.name() == only).into_iter().collect(),
            selected => {
                let selected = selected.iter().collect::<HashSet<_>>();
                self.entries.iter().enumerate().filter(|(_, entry)| selected.contains(entry.name())).map(|(row, _)| row).collect()
            }
        }
    }

    /// The row of the only selected Item, or `None` unless exactly one Item
    /// is selected.
    pub fn single_selected_row(&self) -> Option<usize> {
        let [only] = self.selection.entries() else { return None };
        self.entries.iter().position(|entry| entry.name() == only)
    }

    pub fn scroll_hint(&self) -> Option<&ScrollAnchor> {
        self.scroll_hint.as_ref()
    }

    /// The row the scroll hint names, if it is shown.
    pub fn scroll_hint_row(&self) -> Option<(usize, i32)> {
        let hint = self.scroll_hint.as_ref()?;
        self.entries.iter().position(|entry| entry.name() == hint.entry()).map(|row| (row, hint.offset()))
    }

    /// Changes only when this browser commits a replacement listing.
    pub fn folder_items_revision(&self) -> u64 {
        self.folder_items_revision
    }

    pub fn row(&self, index: usize) -> Option<RowViewModel> {
        self.entry(index).map(|entry| RowViewModel { name: entry.name().to_text_lossy().into_owned(), kind: row_kind(entry.kind()) })
    }

    /// The display text of one cell. The Item Icon Column has none; its icon
    /// is native and loaded by the view.
    pub fn cell_text(&self, row: usize, column: FolderItemsColumn) -> Option<String> {
        let entry = self.entry(row)?;
        let modified = entry.metadata().modified_unix_seconds();
        Some(match column {
            FolderItemsColumn::Icon => String::new(),
            FolderItemsColumn::Name => entry.name().to_text_lossy().into_owned(),
            FolderItemsColumn::Type => entry_type_text(entry),
            FolderItemsColumn::RelativeDate => modified.map_or_else(|| UNAVAILABLE.to_owned(), |modified| format_relative(modified, self.listed_at)),
            FolderItemsColumn::ExactDate => modified.map_or_else(|| UNAVAILABLE.to_owned(), |modified| format_exact(modified, &self.time_zone)),
            FolderItemsColumn::Size if entry.can_enter() => String::new(),
            FolderItemsColumn::Size => entry.metadata().size_bytes().map_or_else(|| UNAVAILABLE.to_owned(), format_size),
        })
    }

    pub fn accessible_cell_text(&self, row: usize, column: FolderItemsColumn) -> Option<String> {
        let mut text = self.cell_text(row, column)?;
        if column == FolderItemsColumn::Name && self.row_pending_cut(row) {
            text.push_str(", pending cut");
        }
        Some(text)
    }

    pub fn row_pending_cut(&self, row: usize) -> bool {
        let (Some(names), Some(entry)) = (self.pending_cut_names(), self.entry(row)) else { return false };
        is_pending_cut(names, entry)
    }

    /// The captured Cut roots that this folder holds, by exact name.
    fn pending_cut_names(&self) -> Option<&HashMap<EntryName, EntryKind>> {
        self.pending_cut_kinds.get(self.location.as_ref()?)
    }

    /// A visible row's position on the Relative Date color strip, or `None`
    /// when its modification time is unavailable.
    pub fn relative_age_position(&self, row: usize) -> Option<f64> {
        self.entry(row)?.metadata().modified_unix_seconds().map(|modified| relative_age_position(modified, self.listed_at))
    }

    /// A same-folder listing acquired at a different time needs its Relative
    /// Date cells repainted even when its entry delta is empty.
    pub fn relative_dates_changed_from(&self, shown: &Self) -> bool {
        self.folder_items_revision != shown.folder_items_revision && self.location == shown.location && self.listed_at != shown.listed_at && !self.entries.is_empty()
    }

    /// The full path of the Item in `row`, for loading its native icon.
    pub fn row_path(&self, row: usize) -> Option<String> {
        let entry = self.entry(row)?;
        let location = self.location.as_ref()?;
        Some(location_text(&location.join(entry.name())))
    }

    /// The exact file-system path and directory flag for a native preview.
    /// Display paths can replace invalid UTF-8 bytes and must not identify an Item.
    pub fn row_native_path(&self, row: usize) -> Option<(Vec<u8>, bool)> {
        let entry = self.entry(row)?;
        let location = self.location.as_ref()?;
        let mut path = Vec::new();
        for component in location.components() {
            path.push(b'/');
            path.extend_from_slice(component.as_bytes());
        }
        path.push(b'/');
        path.extend_from_slice(entry.name().as_bytes());
        Some((path, entry.kind() == EntryKind::Directory))
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn active_tab(&self) -> Option<TabId> {
        self.active_tab
    }

    /// The Browser Tabs in display order.
    pub fn tabs(&self) -> &[TabViewModel] {
        &self.tabs
    }

    /// Changes whenever a tab's label, path, state, order, or activation does.
    pub fn tabs_revision(&self) -> u64 {
        self.tabs_revision
    }

    pub fn toolbar(&self) -> Toolbar {
        self.toolbar
    }

    /// The smallest update that turns a list showing `shown` into one showing
    /// `self`. Anything other than the single next revision of a same-folder
    /// reload is a reset.
    pub fn update_from(&self, shown: &BrowserViewModel) -> FolderItemsUpdate {
        if self.folder_items_revision == shown.folder_items_revision {
            return FolderItemsUpdate::Unchanged;
        }
        if self.folder_items_revision != shown.folder_items_revision.wrapping_add(1) {
            return FolderItemsUpdate::Reset;
        }
        match self.folder_items_delta.as_deref() {
            Some([]) if shown.row_count() == self.row_count() => FolderItemsUpdate::Unchanged,
            Some([change]) if change.row + change.removed <= shown.row_count() && shown.row_count() - change.removed + change.inserted == self.row_count() => {
                let RowChange { row, removed, inserted } = *change;
                let common = removed.min(inserted);
                FolderItemsUpdate::Rows { changed: (common > 0).then(|| (row, row + common - 1)), inserted: (inserted > removed).then(|| (row + removed, row + inserted - 1)), removed: (removed > inserted).then(|| (row + inserted, row + removed - 1)) }
            }
            _ => FolderItemsUpdate::Reset,
        }
    }

    /// Forgets pending and failed path entry, bumping the revision when
    /// anything was shown.
    fn clear_path_entry(&mut self) {
        if self.path_entry.pending || self.path_entry.error.is_some() || self.path_entry.tab.is_some() {
            self.path_entry = PathEntryView { revision: self.path_entry.revision.wrapping_add(1), accepted: self.path_entry.accepted, ..PathEntryView::default() };
            self.refresh_status();
        }
    }

    fn refresh_status(&mut self) {
        self.status_text = if self.path_entry.pending { "Loading…".to_owned() } else { self.path_entry.error.clone().or_else(|| self.status_message.clone()).unwrap_or_else(|| self.base_status.clone()) };
    }

    fn refresh_base_status(&mut self) {
        self.base_status = if self.loading {
            "Loading…".to_owned()
        } else if let Some(error) = &self.error {
            error.clone()
        } else if self.filter_status == FilterStatus::Updating {
            "Updating filter…".to_owned()
        } else if self.filter_status == FilterStatus::Failed {
            "Filter unavailable — edit the text to retry.".to_owned()
        } else {
            match self.monitoring {
                Some(MonitoringStatus::Periodic) => "Checking for changes periodically".to_owned(),
                Some(MonitoringStatus::Unavailable) => "Automatic refresh unavailable — Command+R to Refresh".to_owned(),
                Some(MonitoringStatus::Native) | None => match self.pending_cut_count() {
                    0 => self.location_text.clone(),
                    1 => "1 item ready to move".to_owned(),
                    count => format!("{count} items ready to move"),
                },
            }
        };
    }

    /// The number of captured Cut roots, shown only where at least one of
    /// them is a row.
    fn pending_cut_count(&self) -> usize {
        let Some(names) = self.pending_cut_names() else { return 0 };
        if self.entries.iter().any(|entry| is_pending_cut(names, entry)) { self.pending_cut.len() } else { 0 }
    }

    fn refresh_summary(&mut self) {
        if self.location.is_none() {
            self.summary = Summary::default();
            return;
        }
        let count = if self.filter_text.is_empty() { item_count(self.entries.len()) } else { format!("{} of {} Items", self.entries.len(), self.total_count) };
        let total = total_size(self.entries.iter());
        let names = self.selection.entries().iter().collect::<HashSet<_>>();
        let selected = self.entries.iter().filter(|entry| names.contains(entry.name())).count();
        self.summary = match selected {
            0 => Summary { count, selected: "0 selected".to_owned(), size: total },
            selected => Summary { count, selected: format!("{selected} selected"), size: format!("{} / {total}", total_size(self.entries.iter().filter(|entry| names.contains(entry.name())))) },
        };
    }

    fn tab_label(&self, location: Option<&Location>) -> String {
        match location {
            Some(location) if location.is_root() => {
                if self.root_label.is_empty() {
                    "/".to_owned()
                } else {
                    self.root_label.clone()
                }
            }
            Some(location) => location.components().last().map_or_else(|| "/".to_owned(), |name| name.to_text_lossy().into_owned()),
            None => String::new(),
        }
    }
}

/// Keeps a [`BrowserViewModel`] up to date from application outputs and the
/// Browser's projection.
pub struct BrowserPresenter {
    browser: BrowserSide,
    view: BrowserViewModel,
    clock: Clock,
    chrome: Option<BrowserChrome>,
    listed_at_by_tab: HashMap<TabId, i64>,
}

impl std::fmt::Debug for BrowserPresenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserPresenter").field("browser", &self.browser).field("view", &self.view).finish_non_exhaustive()
    }
}

impl Default for BrowserPresenter {
    fn default() -> Self {
        Self::new(BrowserSide::Left)
    }
}

impl BrowserPresenter {
    /// A presenter that formats dates for the system time zone and clock.
    pub fn new(browser: BrowserSide) -> Self {
        Self::with_clock(browser, Arc::new(|| SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))), TimeZone::system())
    }

    pub fn with_clock(browser: BrowserSide, clock: Clock, time_zone: TimeZone) -> Self {
        Self { browser, view: BrowserViewModel { time_zone, ..BrowserViewModel::default() }, clock, chrome: None, listed_at_by_tab: HashMap::new() }
    }

    pub fn view(&self) -> &BrowserViewModel {
        &self.view
    }

    /// Names the root volume for root tab labels and titles.
    pub fn set_root_label(&mut self, label: &str) {
        if self.view.root_label != label {
            self.view.root_label = label.to_owned();
            if let Some(chrome) = self.chrome.take() {
                self.apply_chrome(&chrome);
            }
        }
    }

    /// Records the text a submission carries, which names the subject of a
    /// failure message.
    pub fn note_entered_path(&mut self, text: &str) {
        text.clone_into(&mut self.view.path_entry.entered);
    }

    /// Ends pending or failed path entry: the text was edited, Escape was
    /// pressed, focus left, or the tab changed.
    pub fn discard_path_entry(&mut self) {
        self.view.clear_path_entry();
    }

    /// Clears a refusal reason once its display time passed, unless a newer
    /// one replaced it.
    pub fn expire_status(&mut self, token: u64) {
        if token == self.view.status_token && self.view.status_message.take().is_some() {
            self.view.refresh_status();
        }
    }

    /// Applies the Browser's tab strip, toolbar availability, and sort.
    pub fn apply_chrome(&mut self, chrome: &BrowserChrome) {
        if self.chrome.as_ref() == Some(chrome) {
            return;
        }
        self.listed_at_by_tab.retain(|id, _| chrome.tabs.iter().any(|tab| tab.id == *id));
        self.view.active_tab = Some(chrome.active_tab);
        let tabs = chrome.tabs.iter().map(|tab| TabViewModel { id: tab.id, label: self.view.tab_label(tab.location.as_ref()), path: tab.location.as_ref().map(location_text).unwrap_or_default(), active: tab.id == chrome.active_tab, loading: tab.loading, failed: tab.failed }).collect::<Vec<_>>();
        if tabs != self.view.tabs {
            self.view.tabs = tabs;
            self.view.tabs_revision = self.view.tabs_revision.wrapping_add(1);
        }
        self.view.toolbar = Toolbar { can_open_tab: chrome.can_open_tab, can_go_back: chrome.can_go_back, can_go_forward: chrome.can_go_forward, can_go_up: chrome.can_go_up, can_refresh: chrome.can_refresh, sort: chrome.sort };
        self.view.monitoring = chrome.monitoring;
        self.view.refresh_base_status();
        self.view.refresh_status();
        self.chrome = Some(chrome.clone());
    }

    pub fn apply(&mut self, output: &Output) {
        if let Output::ClipboardPresentationChanged { pending_cut, .. } = output {
            if *self.view.pending_cut != **pending_cut {
                let view = &mut self.view;
                view.pending_cut = Arc::clone(pending_cut);
                view.pending_cut_kinds.clear();
                for root in pending_cut.iter() {
                    if let (Some(parent), Some(name)) = (root.source.parent(), root.source.components().last()) {
                        view.pending_cut_kinds.entry(parent).or_default().insert(name.clone(), root.kind);
                    }
                }
                // Only the cue changes, so every row repaints in place.
                let rows = view.entries.len();
                view.folder_items_revision = view.folder_items_revision.wrapping_add(1);
                view.folder_items_delta = Some(if rows == 0 { Vec::new() } else { vec![RowChange { row: 0, removed: rows, inserted: rows }] });
                view.refresh_base_status();
                view.refresh_status();
            }
            return;
        }
        let output_browser = match output {
            Output::ClipboardRejected { browser: Some(browser), .. } | Output::ClipboardPrepared { browser, .. } | Output::ClipboardPasteStarted { browser, .. } | Output::LoadingStarted { browser, .. } | Output::PathEntryPending { browser, .. } | Output::PathEntryAccepted { browser, .. } | Output::PathEntryRejected { browser, .. } | Output::PathEntryCancelled { browser, .. } | Output::FolderItemsLoaded { browser, .. } | Output::VisibleItemsChanged { browser, .. } | Output::SelectionChanged { browser, .. } | Output::PreviewChanged { browser, .. } | Output::FolderItemsFailed { browser, .. } | Output::FolderItemsCancelled { browser, .. } | Output::ActiveBrowserChanged { browser } | Output::ActiveTabChanged { browser, .. } | Output::TabsChanged { browser, .. } | Output::TabViewChanged { browser, .. } | Output::OperationRejected { browser, .. } | Output::NameEditorOpened { browser, .. } => *browser,
            Output::FolderItemsMenuReady { .. } | Output::ContextMenuPreferenceRejected { .. } | Output::ClipboardPresentationChanged { .. } | Output::ClipboardRejected { browser: None, .. } | Output::ShortcutEditRejected { .. } | Output::OperationChanged { .. } | Output::OperationDismissed { .. } | Output::OpenItem { .. } | Output::RevealInFinder { .. } | Output::QuitConfirmationRequired { .. } | Output::QuitAccepted | Output::ActivateWindow { .. } | Output::SessionRestored { .. } | Output::LayoutReset | Output::FavoritesChanged { .. } | Output::FavoriteEditRejected { .. } | Output::SettingsSaveFailed { .. } | Output::SettingsLoadFailed { .. } | Output::NoticeAdded { .. } | Output::OpenNotices => return,
        };
        if output_browser != self.browser {
            return;
        }
        // Keep each tab's most recent listing time even when it is inactive.
        // A later tab switch must reuse it instead of aging that view early.
        let loaded_at = if let Output::FolderItemsLoaded { tab, .. } = output {
            let listed_at = (self.clock)();
            self.listed_at_by_tab.insert(*tab, listed_at);
            Some(listed_at)
        } else {
            None
        };
        match output {
            Output::ActiveTabChanged { tab, .. } | Output::TabsChanged { active_tab: tab, .. } | Output::TabViewChanged { tab, .. } => {
                if self.view.active_tab != Some(*tab) {
                    self.view.clear_path_entry();
                }
                self.view.active_tab = Some(*tab);
            }
            Output::LoadingStarted { tab, .. } | Output::FolderItemsLoaded { tab, .. } | Output::VisibleItemsChanged { tab, .. } | Output::SelectionChanged { tab, .. } | Output::FolderItemsFailed { tab, .. } | Output::FolderItemsCancelled { tab, .. } => {
                if self.view.active_tab.is_some_and(|active| active != *tab) {
                    return;
                }
                self.view.active_tab = Some(*tab);
            }
            _ => {}
        }
        let view = &mut self.view;
        match output {
            Output::ClipboardRejected { reason, .. } => {
                view.status_message = Some(clipboard_rejection_text(*reason).to_owned());
                view.status_token = view.status_token.wrapping_add(1);
            }
            Output::ClipboardPrepared { action, count, .. } => {
                let verb = if *action == ClipboardAction::Copy { "copied" } else { "cut" };
                view.status_message = Some(format!("{count} {} {verb}.", if *count == 1 { "item" } else { "items" }));
                view.status_token = view.status_token.wrapping_add(1);
            }
            Output::ClipboardPasteStarted { .. } => {
                view.status_message = Some("Paste prepared.".to_owned());
                view.status_token = view.status_token.wrapping_add(1);
            }
            Output::PreviewChanged { view: preview, .. } => {
                view.preview = preview.clone();
                view.preview_revision = view.preview_revision.wrapping_add(1);
            }
            Output::OperationRejected { reason, .. } => {
                view.status_message = Some(rejection_text(*reason).to_owned());
                view.status_token = view.status_token.wrapping_add(1);
            }
            Output::NameEditorOpened { command, target, .. } => {
                let row = target.as_ref().and_then(|target| view.entries.iter().position(|entry| entry.name() == target));
                if target.is_some() && row.is_none() {
                    return;
                }
                view.editor = Some(EditorRequest { command: *command, row, text: target.as_ref().map(|target| target.to_text_lossy().into_owned()).unwrap_or_default() });
                view.editor_revision = view.editor_revision.wrapping_add(1);
            }
            Output::TabViewChanged { tab, location, entries, selection, row, scroll_hint, loading, error, filter_text, filter_status, total_count, visible_revision, .. } => {
                view.location.clone_from(location);
                view.location_text = location.as_ref().map_or_else(String::new, location_text);
                view.entries = Arc::clone(entries);
                view.filter_text.clone_from(filter_text);
                view.filter_status = *filter_status;
                view.total_count = *total_count;
                view.visible_revision = *visible_revision;
                view.selection = selection.clone();
                view.cursor_row = *row;
                view.selection_revision = view.selection_revision.wrapping_add(1);
                view.scroll_hint = scroll_hint.clone();
                view.loading = *loading;
                view.error = error.as_ref().map(error_message);
                view.base_status = if *loading { "Loading…".to_owned() } else { view.error.clone().unwrap_or_else(|| view.location_text.clone()) };
                view.missing_folder = location.is_some() && error.as_ref().is_some_and(|error| Some(error.location()) == location.as_ref() && is_missing(error.kind()));
                view.folder_items_revision = view.folder_items_revision.wrapping_add(1);
                view.folder_items_delta = None;
                view.listed_at = *self.listed_at_by_tab.entry(*tab).or_insert_with(|| (self.clock)());
                view.refresh_summary();
            }
            Output::PathEntryPending { tab, .. } => {
                if view.active_tab == Some(*tab) {
                    let entered = std::mem::take(&mut view.path_entry.entered);
                    view.path_entry = PathEntryView { tab: Some(*tab), entered, pending: true, error: None, revision: view.path_entry.revision.wrapping_add(1), accepted: view.path_entry.accepted };
                }
            }
            Output::PathEntryRejected { tab, reason, .. } => {
                if view.active_tab == Some(*tab) {
                    let message = path_entry_message(*reason, &view.path_entry.entered);
                    let entered = std::mem::take(&mut view.path_entry.entered);
                    view.path_entry = PathEntryView { tab: Some(*tab), entered, pending: false, error: Some(message), revision: view.path_entry.revision.wrapping_add(1), accepted: view.path_entry.accepted };
                }
            }
            Output::PathEntryAccepted { tab, .. } => {
                // An unedited Return is accepted without a pending state, so the shown tab decides.
                if view.active_tab == Some(*tab) {
                    view.clear_path_entry();
                    view.path_entry.accepted = view.path_entry.accepted.wrapping_add(1);
                    view.path_entry.revision = view.path_entry.revision.wrapping_add(1);
                }
            }
            Output::PathEntryCancelled { tab, .. } => {
                if view.path_entry.tab == Some(*tab) {
                    view.clear_path_entry();
                }
            }
            Output::LoadingStarted { .. } => {
                view.loading = true;
                view.error = None;
                view.base_status = "Loading…".to_owned();
            }
            Output::FolderItemsLoaded { location, entries, changes, scroll_hint, visible_revision, .. } => {
                view.location = Some(location.clone());
                view.location_text = location_text(location);
                view.entries = Arc::clone(entries);
                view.filter_text.clear();
                view.filter_status = FilterStatus::Ready;
                view.total_count = entries.len();
                view.visible_revision = *visible_revision;
                view.scroll_hint = scroll_hint.clone();
                view.loading = false;
                view.error = None;
                view.base_status = view.location_text.clone();
                view.missing_folder = false;
                view.folder_items_revision = view.folder_items_revision.wrapping_add(1);
                view.folder_items_delta = changes.clone();
                view.listed_at = loaded_at.unwrap_or_else(|| (self.clock)());
                view.refresh_summary();
            }
            Output::VisibleItemsChanged { location, entries, selection, row, scroll_hint, filter_text, filter_status, total_count, visible_revision, loading, error, .. } => {
                view.location = Some(location.clone());
                view.location_text = location_text(location);
                view.entries = Arc::clone(entries);
                view.selection = selection.clone();
                view.cursor_row = *row;
                view.scroll_hint = scroll_hint.clone();
                view.filter_text.clone_from(filter_text);
                view.filter_status = *filter_status;
                view.total_count = *total_count;
                view.visible_revision = *visible_revision;
                view.loading = *loading;
                view.error = error.as_ref().map(error_message);
                view.base_status = if *loading { "Loading…".to_owned() } else { view.error.clone().unwrap_or_else(|| view.location_text.clone()) };
                view.missing_folder = error.as_ref().is_some_and(|error| error.location() == location && is_missing(error.kind()));
                view.folder_items_revision = view.folder_items_revision.wrapping_add(1);
                view.folder_items_delta = None;
                view.selection_revision = view.selection_revision.wrapping_add(1);
                view.refresh_summary();
            }
            Output::SelectionChanged { selection, row, .. } => {
                view.selection = selection.clone();
                view.cursor_row = *row;
                view.selection_revision = view.selection_revision.wrapping_add(1);
                view.refresh_summary();
            }
            Output::FolderItemsFailed { error, .. } => {
                view.loading = false;
                view.error = Some(error_message(error));
                view.base_status = view.error.clone().unwrap_or_default();
                if view.location.as_ref() == Some(error.location()) {
                    view.missing_folder = is_missing(error.kind());
                }
            }
            Output::FolderItemsCancelled { .. } => {
                view.loading = false;
                view.error = None;
                view.base_status = view.location_text.clone();
            }
            _ => {}
        }
        view.refresh_base_status();
        view.refresh_status();
    }
}

/// A row is a pending Cut only when both its exact name and its kind match.
fn is_pending_cut(names: &HashMap<EntryName, EntryKind>, entry: &Entry) -> bool {
    names.get(entry.name()) == Some(&entry.kind())
}

/// A folder that is gone or no longer a folder shows the Missing Folder
/// Overlay over its last rows.
fn is_missing(kind: ListingErrorKind) -> bool {
    matches!(kind, ListingErrorKind::ItemMissing | ListingErrorKind::NotADirectory)
}

fn clipboard_rejection_text(reason: ClipboardRejection) -> &'static str {
    match reason {
        ClipboardRejection::StaleTab | ClipboardRejection::StaleObservation => "The folder view changed. Try again.",
        ClipboardRejection::NoSelection => "Select one or more items first.",
        ClipboardRejection::SourceUnavailable => "The selected items are not available.",
        ClipboardRejection::DestinationUnavailable => "Paste needs a ready destination folder.",
        ClipboardRejection::PublicationFailed => "The clipboard could not be updated.",
        ClipboardRejection::ClipboardBusy => "The clipboard is busy. Try again.",
        ClipboardRejection::ClipboardReplaced => "The clipboard changed before Paste was ready.",
        ClipboardRejection::UnknownPrivateTransfer => "That Dual Pane transfer is no longer available.",
        ClipboardRejection::TransferReserved => "That Cut is already being pasted.",
        ClipboardRejection::UnsupportedContent => "The clipboard does not contain files that can be pasted.",
        ClipboardRejection::MalformedExternalContent => "The clipboard contains malformed file references.",
        ClipboardRejection::DuplicateSource => "The clipboard contains the same item more than once.",
        ClipboardRejection::SourceChanged => "A clipboard source changed before Paste could start.",
        ClipboardRejection::SameSourceAndDestination => "Items cannot be pasted into their current folder.",
        ClipboardRejection::DestinationWithinSource => "A folder cannot be pasted inside itself.",
        ClipboardRejection::JournalUnavailable => "Copy is unavailable until the safety journal is ready.",
    }
}

/// Safe wording for failures before a browser session can be created.
pub fn reader_start_failure_status() -> &'static str {
    "Dual Pane couldn’t start reading folders."
}

fn row_kind(kind: EntryKind) -> RowKind {
    match kind {
        EntryKind::Directory => RowKind::Folder,
        EntryKind::File => RowKind::File,
        EntryKind::Symlink { points_to_directory: true } => RowKind::FolderLink,
        EntryKind::Symlink { points_to_directory: false } => RowKind::Link,
        EntryKind::Other => RowKind::Other,
    }
}

fn error_message(error: &ListingError) -> String {
    listing_kind_message(error.kind(), &location_text(error.location()))
}

/// Failure wording for entered text, with the entered path as the subject.
fn path_entry_message(reason: PathEntryFailure, entered: &str) -> String {
    match reason {
        PathEntryFailure::Rejected(PathRejection::Malformed) => format!("“{entered}” isn’t a valid path."),
        PathEntryFailure::Rejected(PathRejection::NoConfirmedFolder) => "This tab has no folder yet. Enter a full path that starts with / or ~.".to_owned(),
        PathEntryFailure::Rejected(PathRejection::UndecodableText) => format!("“{entered}” can’t be matched to a file name. Use a relative path, Up, or Back."),
        PathEntryFailure::Listing(kind) => listing_kind_message(kind, entered),
    }
}

fn listing_kind_message(kind: ListingErrorKind, path: &str) -> String {
    match kind {
        ListingErrorKind::ItemMissing => format!("“{path}” no longer exists."),
        ListingErrorKind::NotADirectory => format!("“{path}” is not a folder."),
        ListingErrorKind::PermissionDenied => format!("You don’t have permission to open “{path}”."),
        ListingErrorKind::PrivacyRestricted => format!("macOS privacy settings don’t allow Dual Pane to open “{path}”."),
        ListingErrorKind::Internal => "Dual Pane couldn’t finish reading this folder unexpectedly.".to_owned(),
        ListingErrorKind::Busy => "Dual Pane is busy reading folders. Try again shortly.".to_owned(),
        ListingErrorKind::VisibilityClassificationFailed => "Dual Pane couldn’t determine an item’s macOS visibility. Refresh to try again.".to_owned(),
        ListingErrorKind::Unknown => format!("“{path}” couldn’t be opened."),
    }
}
