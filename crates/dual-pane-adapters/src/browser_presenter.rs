use std::sync::Arc;

use dual_pane_application::Output;
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, ListingError, ListingErrorKind, Location, Selection, TabId};

/// What a Browser shows. Rows are formatted when requested, so updating the view
/// model costs the same for any directory size.
#[derive(Debug, Clone, Default)]
pub struct BrowserViewModel {
    location_text: String,
    loading: bool,
    error: Option<String>,
    status_text: String,
    entries: Arc<[Entry]>,
    selection: Selection,
    selected_row: Option<usize>,
    scroll_hint: Option<(EntryName, i32)>,
    folder_items_revision: u64,
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

    pub fn row_count(&self) -> usize {
        self.entries.len()
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn selected_row(&self) -> Option<usize> {
        self.selected_row
    }

    pub fn scroll_hint(&self) -> Option<&(EntryName, i32)> {
        self.scroll_hint.as_ref()
    }

    /// Changes only when this browser commits a replacement listing.
    pub fn folder_items_revision(&self) -> u64 {
        self.folder_items_revision
    }

    pub fn row(&self, index: usize) -> Option<RowViewModel> {
        self.entry(index).map(|entry| RowViewModel { name: entry.name().to_text_lossy().into_owned(), kind: row_kind(entry.kind()) })
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }
}

/// Keeps a [`BrowserViewModel`] up to date from application outputs.
#[derive(Debug)]
pub struct BrowserPresenter {
    browser: BrowserSide,
    active_tab: Option<TabId>,
    view: BrowserViewModel,
}

impl Default for BrowserPresenter {
    fn default() -> Self {
        Self::new(BrowserSide::Left)
    }
}

impl BrowserPresenter {
    pub fn new(browser: BrowserSide) -> Self {
        Self { browser, active_tab: None, view: BrowserViewModel::default() }
    }

    pub fn view(&self) -> &BrowserViewModel {
        &self.view
    }

    pub fn apply(&mut self, output: &Output) {
        let output_browser = match output {
            Output::LoadingStarted { browser, .. } | Output::FolderItemsReplaced { browser, .. } | Output::FolderItemsRowsChanged { browser, .. } | Output::SelectionChanged { browser, .. } | Output::FolderItemsFailed { browser, .. } | Output::FolderItemsCancelled { browser, .. } | Output::ActiveBrowserChanged { browser } | Output::ActiveTabChanged { browser, .. } | Output::TabsChanged { browser, .. } | Output::TabViewChanged { browser, .. } => *browser,
            Output::FavoritesChanged { .. } | Output::SettingsSaveFailed { .. } => return,
        };
        if output_browser != self.browser {
            return;
        }
        match output {
            Output::ActiveTabChanged { tab, .. } | Output::TabsChanged { active_tab: tab, .. } | Output::TabViewChanged { tab, .. } => self.active_tab = Some(*tab),
            Output::LoadingStarted { tab, .. } | Output::FolderItemsReplaced { tab, .. } | Output::FolderItemsRowsChanged { tab, .. } | Output::SelectionChanged { tab, .. } | Output::FolderItemsFailed { tab, .. } | Output::FolderItemsCancelled { tab, .. } => {
                if self.active_tab.is_some_and(|active| active != *tab) {
                    return;
                }
                self.active_tab = Some(*tab);
            }
            Output::ActiveBrowserChanged { .. } => {}
            Output::FavoritesChanged { .. } | Output::SettingsSaveFailed { .. } => {}
        }
        match output {
            Output::TabViewChanged { location, entries, selection, row, scroll_hint, loading, error, .. } => {
                self.view.location_text = location.as_ref().map_or_else(String::new, location_text);
                self.view.entries = Arc::clone(entries);
                self.view.selection = selection.clone();
                self.view.selected_row = *row;
                self.view.scroll_hint = scroll_hint.clone();
                self.view.loading = *loading;
                self.view.error = error.as_ref().map(error_message);
                self.view.status_text = if *loading { "Loading…".to_owned() } else { self.view.error.clone().unwrap_or_else(|| self.view.location_text.clone()) };
                self.view.folder_items_revision = self.view.folder_items_revision.wrapping_add(1);
            }
            Output::LoadingStarted { .. } => {
                self.view.loading = true;
                self.view.error = None;
                self.view.status_text = "Loading…".to_owned();
            }
            Output::FolderItemsReplaced { location, entries, scroll_hint, .. } => {
                self.view.location_text = location_text(location);
                self.view.entries = Arc::clone(entries);
                self.view.scroll_hint = scroll_hint.clone();
                self.view.loading = false;
                self.view.error = None;
                self.view.status_text = self.view.location_text.clone();
                self.view.folder_items_revision = self.view.folder_items_revision.wrapping_add(1);
            }
            Output::FolderItemsRowsChanged { .. } => {}
            Output::SelectionChanged { selection, row, .. } => {
                self.view.selection = selection.clone();
                self.view.selected_row = *row;
            }
            Output::FolderItemsFailed { error, .. } => {
                self.view.loading = false;
                self.view.error = Some(error_message(error));
                self.view.status_text = self.view.error.clone().unwrap_or_default();
            }
            Output::FolderItemsCancelled { .. } => {
                self.view.loading = false;
                self.view.error = None;
                self.view.status_text = self.view.location_text.clone();
            }
            Output::ActiveBrowserChanged { .. } | Output::ActiveTabChanged { .. } | Output::TabsChanged { .. } | Output::FavoritesChanged { .. } | Output::SettingsSaveFailed { .. } => {}
        }
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

fn location_text(location: &Location) -> String {
    if location.is_root() {
        return "/".to_owned();
    }
    location.components().iter().fold(String::new(), |mut text, name| {
        text.push('/');
        text.push_str(&name.to_text_lossy());
        text
    })
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
