use dual_pane_application::{ActionBinding, FavoriteEdit, FavoriteRejection, Notice, NoticeKind, Output, SettingsFailure, WorkspaceChrome};
use dual_pane_domain::BrowserSide;

use crate::format::location_text;

/// Workspace-wide presentation state: the Sidebar Favorites, Notices, the
/// effective shortcuts, and which Browser is active.
#[derive(Debug, Clone)]
pub struct WorkspaceViewModel {
    active_browser: BrowserSide,
    groups: Vec<FavoriteGroupViewModel>,
    favorites_ready: bool,
    favorites_revision: u64,
    bindings: Vec<ActionBinding>,
    bindings_revision: u64,
    notices: Vec<NoticeViewModel>,
    notices_revision: u64,
    open_requests: u64,
    last_rejection: Option<(FavoriteEdit, FavoriteRejection)>,
}

impl Default for WorkspaceViewModel {
    fn default() -> Self {
        Self { active_browser: BrowserSide::Left, groups: Vec::new(), favorites_ready: false, favorites_revision: 0, bindings: Vec::new(), bindings_revision: 0, notices: Vec::new(), notices_revision: 0, open_requests: 0, last_rejection: None }
    }
}

/// One Favorite Group row and its Items in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteGroupViewModel {
    pub id: i64,
    pub name: String,
    pub items: Vec<FavoriteItemViewModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteItemViewModel {
    pub id: i64,
    pub alias: String,
    /// The target's full path, for the tooltip.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeViewModel {
    pub id: u64,
    pub text: String,
    pub offers_reset: bool,
}

impl WorkspaceViewModel {
    pub fn active_browser(&self) -> BrowserSide {
        self.active_browser
    }
    pub fn groups(&self) -> &[FavoriteGroupViewModel] {
        &self.groups
    }
    /// Whether Favorites edits can be accepted; editing controls stay
    /// disabled until then.
    pub fn favorites_ready(&self) -> bool {
        self.favorites_ready
    }
    /// Changes whenever the groups, Items, or readiness change.
    pub fn favorites_revision(&self) -> u64 {
        self.favorites_revision
    }
    pub fn bindings(&self) -> &[ActionBinding] {
        &self.bindings
    }
    /// Changes whenever an effective shortcut changes.
    pub fn bindings_revision(&self) -> u64 {
        self.bindings_revision
    }
    pub fn notices(&self) -> &[NoticeViewModel] {
        &self.notices
    }
    pub fn notices_revision(&self) -> u64 {
        self.notices_revision
    }
    /// Counts requests to show Notices, such as an actionable storage error.
    pub fn open_requests(&self) -> u64 {
        self.open_requests
    }
    /// The most recent rejected Favorites edit.
    pub fn last_rejection(&self) -> Option<(FavoriteEdit, FavoriteRejection)> {
        self.last_rejection
    }
    pub fn group(&self, id: i64) -> Option<(usize, &FavoriteGroupViewModel)> {
        self.groups.iter().enumerate().find(|(_, group)| group.id == id)
    }
    /// The Group holding Item `id` and the Item's index in it.
    pub fn item(&self, id: i64) -> Option<(&FavoriteGroupViewModel, usize)> {
        self.groups.iter().find_map(|group| group.items.iter().position(|item| item.id == id).map(|index| (group, index)))
    }
}

/// Keeps a [`WorkspaceViewModel`] up to date.
#[derive(Debug, Default)]
pub struct WorkspacePresenter {
    view: WorkspaceViewModel,
    chrome: Option<WorkspaceChrome>,
}

impl WorkspacePresenter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn view(&self) -> &WorkspaceViewModel {
        &self.view
    }

    pub fn apply(&mut self, output: &Output) {
        match output {
            Output::NoticeAdded { open: true, .. } => self.view.open_requests = self.view.open_requests.wrapping_add(1),
            Output::FavoriteEditRejected { edit, reason } => self.view.last_rejection = Some((*edit, *reason)),
            Output::FavoritesChanged { .. } => self.view.last_rejection = None,
            _ => {}
        }
    }

    pub fn apply_chrome(&mut self, chrome: &WorkspaceChrome) {
        let previous = self.chrome.as_ref();
        self.view.active_browser = chrome.active_browser;
        if previous.is_none_or(|previous| previous.favorites != chrome.favorites || previous.favorites_ready != chrome.favorites_ready) {
            let mut groups = chrome.favorites.groups.iter().collect::<Vec<_>>();
            groups.sort_by_key(|group| (group.position, group.id));
            self.view.groups = groups
                .into_iter()
                .map(|group| {
                    let mut items = chrome.favorites.items.iter().filter(|item| item.group_id == group.id).collect::<Vec<_>>();
                    items.sort_by_key(|item| (item.position, item.id));
                    FavoriteGroupViewModel { id: group.id, name: group.name.clone(), items: items.into_iter().map(|item| FavoriteItemViewModel { id: item.id, alias: item.name.clone(), path: location_text(&item.target) }).collect() }
                })
                .collect();
            self.view.favorites_ready = chrome.favorites_ready;
            self.view.favorites_revision = self.view.favorites_revision.wrapping_add(1);
        }
        if previous.is_none_or(|previous| previous.bindings != chrome.bindings) {
            self.view.bindings.clone_from(&chrome.bindings);
            self.view.bindings_revision = self.view.bindings_revision.wrapping_add(1);
        }
        if previous.is_none_or(|previous| !std::sync::Arc::ptr_eq(&previous.notices, &chrome.notices)) {
            self.view.notices = chrome.notices.iter().map(|notice| NoticeViewModel { id: notice.id, text: notice_text(notice), offers_reset: notice.offers_reset }).collect();
            self.view.notices_revision = self.view.notices_revision.wrapping_add(1);
        }
        self.chrome = Some(chrome.clone());
    }
}

/// Inline wording for a rejected Favorites edit, or `None` when the edit
/// needs no feedback because it changed nothing.
pub fn favorite_rejection_text(edit: FavoriteEdit, reason: FavoriteRejection) -> Option<&'static str> {
    match (edit, reason) {
        (_, FavoriteRejection::Unchanged) => None,
        (_, FavoriteRejection::InvalidName) => Some("Enter a name."),
        (FavoriteEdit::CreateGroup | FavoriteEdit::Group(_), FavoriteRejection::DuplicateName) => Some("A group with this name already exists."),
        (FavoriteEdit::CreateItem { .. } | FavoriteEdit::Item(_), FavoriteRejection::DuplicateName) => Some("Already in this group"),
        (_, FavoriteRejection::Stale) => Some("This Favorite no longer exists."),
        (_, FavoriteRejection::NotReady) => Some("Favorites are still loading."),
    }
}

fn notice_text(notice: &Notice) -> String {
    let unsaved = "Changes made now are kept until you quit but aren’t saved.";
    match &notice.kind {
        NoticeKind::SettingsLoadFailed { failure } => match failure {
            SettingsFailure::WorkerUnavailable => format!("Dual Pane couldn’t start its settings service. {unsaved}"),
            SettingsFailure::Unavailable => format!("Dual Pane couldn’t open its settings. {unsaved}"),
            SettingsFailure::Corrupt => format!("Dual Pane’s settings are damaged, so default settings are in use. {unsaved}"),
            SettingsFailure::UnsupportedSchema => format!("Dual Pane’s settings were saved by a newer version, so default settings are in use. {unsaved}"),
        },
        NoticeKind::SettingsSaveFailed { .. } => "Dual Pane couldn’t save settings. Your changes stay in effect until you quit.".to_owned(),
        NoticeKind::SettingsReset { backup: Some(backup) } => format!("Settings were reset. The previous settings were preserved at “{}”.", location_text(backup)),
        NoticeKind::SettingsReset { backup: None } => "Settings were reset.".to_owned(),
        NoticeKind::SettingsResetFailed { .. } => "Settings couldn’t be reset. The stored settings were not changed.".to_owned(),
        NoticeKind::FavoriteRemoved { name, target } => format!("Removed Favorite “{name}” because “{}” is unavailable.", location_text(target)),
        NoticeKind::FavoriteProbeFailed { name, target } => format!("Couldn’t check Favorite “{name}” at “{}”.", location_text(target)),
    }
}
