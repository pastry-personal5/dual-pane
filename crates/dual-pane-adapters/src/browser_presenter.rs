use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dual_pane_application::{BrowserChrome, Output, RowChange};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, ListingError, ListingErrorKind, Location, OperationCommand, ScrollAnchor, Selection, SortSpec, TabId, entry_type_text};
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
    missing_folder: bool,
    editor: Option<EditorRequest>,
    editor_revision: u64,
    entries: Arc<[Entry]>,
    selection: Selection,
    cursor_row: Option<usize>,
    selection_revision: u64,
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
}

impl Default for BrowserViewModel {
    fn default() -> Self {
        Self { location: None, location_text: String::new(), root_label: String::new(), loading: false, error: None, base_status: String::new(), status_text: String::new(), status_message: None, status_token: 0, missing_folder: false, editor: None, editor_revision: 0, entries: Arc::from([]), selection: Selection::default(), cursor_row: None, selection_revision: 0, summary: Summary::default(), scroll_hint: None, folder_items_revision: 0, folder_items_delta: None, listed_at: 0, time_zone: TimeZone::UTC, active_tab: None, tabs: Vec::new(), tabs_revision: 0, toolbar: Toolbar::default() }
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

    fn refresh_status(&mut self) {
        self.status_text = self.status_message.clone().unwrap_or_else(|| self.base_status.clone());
    }

    fn refresh_summary(&mut self) {
        if self.location.is_none() {
            self.summary = Summary::default();
            return;
        }
        let count = item_count(self.entries.len());
        let total = total_size(self.entries.iter());
        self.summary = match self.selection.entries().len() {
            0 => Summary { count, selected: String::new(), size: total },
            selected => {
                let names = self.selection.entries().iter().collect::<HashSet<_>>();
                Summary { count, selected: format!("{selected} selected"), size: format!("{} / {total}", total_size(self.entries.iter().filter(|entry| names.contains(entry.name())))) }
            }
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
        self.chrome = Some(chrome.clone());
    }

    pub fn apply(&mut self, output: &Output) {
        let output_browser = match output {
            Output::LoadingStarted { browser, .. } | Output::FolderItemsLoaded { browser, .. } | Output::SelectionChanged { browser, .. } | Output::FolderItemsFailed { browser, .. } | Output::FolderItemsCancelled { browser, .. } | Output::ActiveBrowserChanged { browser } | Output::ActiveTabChanged { browser, .. } | Output::TabsChanged { browser, .. } | Output::TabViewChanged { browser, .. } | Output::OperationRejected { browser, .. } | Output::NameEditorOpened { browser, .. } => *browser,
            Output::OperationChanged { .. } | Output::OperationDismissed { .. } | Output::OpenItem { .. } | Output::QuitConfirmationRequired { .. } | Output::QuitAccepted | Output::SessionRestored { .. } | Output::LayoutReset | Output::FavoritesChanged { .. } | Output::FavoriteEditRejected { .. } | Output::SettingsSaveFailed { .. } | Output::SettingsLoadFailed { .. } | Output::NoticeAdded { .. } => return,
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
            Output::ActiveTabChanged { tab, .. } | Output::TabsChanged { active_tab: tab, .. } | Output::TabViewChanged { tab, .. } => self.view.active_tab = Some(*tab),
            Output::LoadingStarted { tab, .. } | Output::FolderItemsLoaded { tab, .. } | Output::SelectionChanged { tab, .. } | Output::FolderItemsFailed { tab, .. } | Output::FolderItemsCancelled { tab, .. } => {
                if self.view.active_tab.is_some_and(|active| active != *tab) {
                    return;
                }
                self.view.active_tab = Some(*tab);
            }
            _ => {}
        }
        let view = &mut self.view;
        match output {
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
            Output::TabViewChanged { tab, location, entries, selection, row, scroll_hint, loading, error, .. } => {
                view.location.clone_from(location);
                view.location_text = location.as_ref().map_or_else(String::new, location_text);
                view.entries = Arc::clone(entries);
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
            Output::LoadingStarted { .. } => {
                view.loading = true;
                view.error = None;
                view.base_status = "Loading…".to_owned();
            }
            Output::FolderItemsLoaded { location, entries, changes, scroll_hint, .. } => {
                view.location = Some(location.clone());
                view.location_text = location_text(location);
                view.entries = Arc::clone(entries);
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
        view.refresh_status();
    }
}

/// A folder that is gone or no longer a folder shows the Missing Folder
/// Overlay over its last rows.
fn is_missing(kind: ListingErrorKind) -> bool {
    matches!(kind, ListingErrorKind::ItemMissing | ListingErrorKind::NotADirectory)
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
    let path = location_text(error.location());
    match error.kind() {
        ListingErrorKind::ItemMissing => format!("“{path}” no longer exists."),
        ListingErrorKind::NotADirectory => format!("“{path}” is not a folder."),
        ListingErrorKind::PermissionDenied => format!("You don’t have permission to open “{path}”."),
        ListingErrorKind::PrivacyRestricted => format!("macOS privacy settings don’t allow Dual Pane to open “{path}”."),
        ListingErrorKind::Internal => "Dual Pane couldn’t finish reading this folder unexpectedly.".to_owned(),
        ListingErrorKind::Busy => "Dual Pane is busy reading folders. Try again shortly.".to_owned(),
        ListingErrorKind::Unknown => format!("“{path}” couldn’t be opened."),
    }
}
