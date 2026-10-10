use dual_pane_application::{ActionBinding, FavoriteEdit, FavoriteRejection, Notice, NoticeKind, Output, SettingsFailure, WorkspaceChrome};
use dual_pane_domain::BrowserSide;

use crate::format::location_text;
use crate::operation_presenter::operation_summary;

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
    hide_notices_at_startup: bool,
    notices_startup_ready: bool,
    settings_interaction_available: bool,
    last_rejection: Option<(FavoriteEdit, FavoriteRejection)>,
}

impl Default for WorkspaceViewModel {
    fn default() -> Self {
        Self { active_browser: BrowserSide::Left, groups: Vec::new(), favorites_ready: false, favorites_revision: 0, bindings: Vec::new(), bindings_revision: 0, notices: Vec::new(), notices_revision: 0, open_requests: 0, hide_notices_at_startup: false, notices_startup_ready: false, settings_interaction_available: false, last_rejection: None }
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
    /// Whether the message offers Try Again for the safety journal.
    pub offers_journal_retry: bool,
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
    pub fn hide_notices_at_startup(&self) -> bool {
        self.hide_notices_at_startup
    }
    pub fn notices_startup_ready(&self) -> bool {
        self.notices_startup_ready
    }
    /// Whether the desktop may open the Settings Window.
    pub fn settings_interaction_available(&self) -> bool {
        self.settings_interaction_available
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
            Output::NoticeAdded { open: true, .. } | Output::OpenNotices => self.view.open_requests = self.view.open_requests.wrapping_add(1),
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
        self.view.hide_notices_at_startup = chrome.hide_notices_at_startup;
        self.view.notices_startup_ready = chrome.notices_startup_ready;
        self.view.settings_interaction_available = chrome.settings_interaction_available;
        if previous.is_none_or(|previous| !std::sync::Arc::ptr_eq(&previous.notices, &chrome.notices)) {
            self.view.notices = chrome.notices.iter().map(|notice| NoticeViewModel { id: notice.id, text: notice_text(notice), offers_reset: notice.offers_reset, offers_journal_retry: notice.kind == NoticeKind::JournalUnavailable }).collect();
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
        NoticeKind::OperationFinished { intent, outcome, progress, failure } => operation_summary(intent, *outcome, *progress, failure.as_ref()),
        NoticeKind::JournalUnavailable => "Dual Pane couldn’t open its file-operation safety journal, so Copy is unavailable.".to_owned(),
        NoticeKind::TemporariesSwept { removed, failed } if failed.is_empty() => format!("Removed {} left by an earlier session.", temporary_items(*removed)),
        NoticeKind::TemporariesSwept { removed, failed } => {
            let folders = failed.iter().map(|folder| format!("“{}”", location_text(folder))).collect::<Vec<_>>().join(", ");
            let removed = if *removed > 0 { format!("Removed {}. ", temporary_items(*removed)) } else { String::new() };
            format!("{removed}Couldn’t remove temporary items left by an earlier session in {folders}.")
        }
        NoticeKind::OpenFailed { item } => format!("Couldn’t open “{}”.", location_text(item)),
        NoticeKind::TabDiscarded { location, reason } => format!("Couldn’t restore “{}”: {reason:?}.", location_text(location)),
        NoticeKind::SessionNotRestored => "The previous workspace could not be restored. Opened Home folders instead.".to_owned(),
        NoticeKind::SettingsLoadTimedOut => "Settings took too long to load. This launch’s tabs and window layout will not be saved; the previous workspace is protected.".to_owned(),
    }
}

fn temporary_items(count: usize) -> String {
    if count == 1 { "1 temporary item".to_owned() } else { format!("{count} temporary items") }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dual_pane_application::{Notice, NoticeKind, WorkspaceChrome};
    use dual_pane_domain::{BrowserSide, EntryName, Location};

    use super::*;

    fn texts(kinds: Vec<NoticeKind>) -> Vec<NoticeViewModel> {
        let notices = kinds.into_iter().enumerate().map(|(id, kind)| Notice { id: id as u64, kind, offers_reset: false }).collect::<Arc<[_]>>();
        let mut presenter = WorkspacePresenter::new();
        presenter.apply_chrome(&WorkspaceChrome { active_browser: BrowserSide::Left, favorites: Default::default(), favorites_ready: true, bindings: vec![], hide_notices_at_startup: false, notices_startup_ready: true, settings_interaction_available: true, notices });
        presenter.view().notices().to_vec()
    }

    #[test]
    fn settings_interaction_availability_is_projected_without_a_binding_change() {
        let notices = Arc::from([]);
        let mut chrome = WorkspaceChrome { active_browser: BrowserSide::Left, favorites: Default::default(), favorites_ready: false, bindings: vec![], hide_notices_at_startup: false, notices_startup_ready: false, settings_interaction_available: false, notices };
        let mut presenter = WorkspacePresenter::new();
        presenter.apply_chrome(&chrome);
        assert!(!presenter.view().settings_interaction_available());
        assert_eq!(presenter.view().bindings_revision(), 1);

        chrome.settings_interaction_available = true;
        presenter.apply_chrome(&chrome);
        assert!(presenter.view().settings_interaction_available());
        assert_eq!(presenter.view().bindings_revision(), 1, "availability does not rebind workspace actions");
    }

    #[test]
    fn journal_sweep_and_open_notices_are_worded_and_only_the_journal_offers_try_again() {
        let folder = Location::root().join(&EntryName::new("vol").unwrap());
        let notices = texts(vec![NoticeKind::JournalUnavailable, NoticeKind::TemporariesSwept { removed: 1, failed: vec![] }, NoticeKind::TemporariesSwept { removed: 2, failed: vec![folder.clone()] }, NoticeKind::OpenFailed { item: folder }]);
        let texts = notices.iter().map(|notice| notice.text.as_str()).collect::<Vec<_>>();
        assert_eq!(texts, ["Dual Pane couldn’t open its file-operation safety journal, so Copy is unavailable.", "Removed 1 temporary item left by an earlier session.", "Removed 2 temporary items. Couldn’t remove temporary items left by an earlier session in “/vol”.", "Couldn’t open “/vol”."]);
        assert_eq!(notices.iter().map(|notice| notice.offers_journal_retry).collect::<Vec<_>>(), [true, false, false, false]);
    }
}
