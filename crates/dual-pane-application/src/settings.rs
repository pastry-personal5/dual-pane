//! Qt-free durable settings values and their in-memory policy.

use std::collections::{HashMap, VecDeque};

use dual_pane_domain::{BrowserSide, EntryName, Location, SortSpec};

pub const FOLDER_SORT_LIMIT: usize = 100;

/// Stable, persisted compatibility keys for commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionId {
    FocusOtherBrowser,
    NavigateParent,
    CloseWindow,
    QuitApplication,
    NewFolder,
    SortByNameAscending,
    SortByNameDescending,
    SortByTypeAscending,
    SortByTypeDescending,
    SortByDateAscending,
    SortByDateDescending,
    SortBySizeAscending,
    SortBySizeDescending,
}

impl ActionId {
    pub const ALL: [Self; 13] = [Self::FocusOtherBrowser, Self::NavigateParent, Self::CloseWindow, Self::QuitApplication, Self::NewFolder, Self::SortByNameAscending, Self::SortByNameDescending, Self::SortByTypeAscending, Self::SortByTypeDescending, Self::SortByDateAscending, Self::SortByDateDescending, Self::SortBySizeAscending, Self::SortBySizeDescending];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FocusOtherBrowser => "FocusOtherBrowser",
            Self::NavigateParent => "NavigateParent",
            Self::CloseWindow => "CloseWindow",
            Self::QuitApplication => "QuitApplication",
            Self::NewFolder => "NewFolder",
            Self::SortByNameAscending => "SortByNameAscending",
            Self::SortByNameDescending => "SortByNameDescending",
            Self::SortByTypeAscending => "SortByTypeAscending",
            Self::SortByTypeDescending => "SortByTypeDescending",
            Self::SortByDateAscending => "SortByDateAscending",
            Self::SortByDateDescending => "SortByDateDescending",
            Self::SortBySizeAscending => "SortBySizeAscending",
            Self::SortBySizeDescending => "SortBySizeDescending",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

/// A platform-neutral normalized shortcut. `key` is an uppercase ASCII key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub command: bool,
    pub shift: bool,
    pub option: bool,
    pub control: bool,
    pub key: char,
}

impl Shortcut {
    pub const fn new(command: bool, shift: bool, option: bool, control: bool, key: char) -> Self {
        Self { command, shift, option, control, key }
    }
    pub fn is_valid(self) -> bool {
        self.key.is_ascii_uppercase() || self.key.is_ascii_digit()
    }
    pub fn is_reserved(self) -> bool {
        self.command && matches!(self.key, 'H' | 'M')
    }
}

pub const fn default_shortcut(action: ActionId) -> Option<Shortcut> {
    match action {
        ActionId::FocusOtherBrowser => Some(Shortcut::new(false, false, true, false, 'F')),
        ActionId::NavigateParent => Some(Shortcut::new(false, false, false, false, 'L')),
        ActionId::CloseWindow => Some(Shortcut::new(true, false, false, false, 'W')),
        ActionId::QuitApplication => Some(Shortcut::new(true, false, false, false, 'Q')),
        ActionId::NewFolder => Some(Shortcut::new(true, true, false, false, 'N')),
        ActionId::SortByNameAscending | ActionId::SortByNameDescending | ActionId::SortByTypeAscending | ActionId::SortByTypeDescending | ActionId::SortByDateAscending | ActionId::SortByDateDescending | ActionId::SortBySizeAscending | ActionId::SortBySizeDescending => None,
    }
}

pub fn default_bindings() -> Vec<ActionBinding> {
    ActionId::ALL.into_iter().map(|action| ActionBinding { action, shortcut: default_shortcut(action) }).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionBinding {
    pub action: ActionId,
    pub shortcut: Option<Shortcut>,
}

pub fn validate_bindings(bindings: &[ActionBinding]) -> Vec<ActionBinding> {
    let mut accepted = Vec::new();
    for binding in bindings {
        if accepted.iter().any(|existing: &ActionBinding| existing.action == binding.action) {
            continue;
        }
        let valid = binding.shortcut.is_none_or(|shortcut| shortcut.is_valid() && !shortcut.is_reserved() && !accepted.iter().any(|existing: &ActionBinding| existing.shortcut == Some(shortcut)));
        if valid {
            accepted.push(binding.clone());
        }
    }
    accepted
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteGroupRecord {
    pub id: i64,
    pub name: String,
    pub position: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteItemRecord {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub target: Location,
    pub position: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FavoritesRecords {
    pub initialized: bool,
    pub groups: Vec<FavoriteGroupRecord>,
    pub items: Vec<FavoriteItemRecord>,
}

/// The product's fresh-profile hierarchy. The caller determines whether the
/// optional Screenshots location exists without making this Qt-free model do
/// any path probing.
pub fn fresh_profile_favorites(home: &Location, screenshots_exists: bool) -> FavoritesRecords {
    let name = |value| EntryName::new(value).unwrap_or_else(|_| unreachable!("fixed Favorite name is valid"));
    let home_location = |value| home.join(&name(value));
    let documents = home_location(b"Documents".to_vec());
    let mut items = vec![FavoriteItemRecord { id: 1, group_id: 1, name: "Applications".into(), target: Location::root().join(&name(b"Applications".to_vec())), position: 0 }, FavoriteItemRecord { id: 2, group_id: 1, name: "Desktop".into(), target: home_location(b"Desktop".to_vec()), position: 1 }, FavoriteItemRecord { id: 3, group_id: 1, name: "Documents".into(), target: documents.clone(), position: 2 }];
    if screenshots_exists {
        items.push(FavoriteItemRecord { id: 4, group_id: 1, name: "Screenshots".into(), target: documents.join(&name(b"Screenshots".to_vec())), position: 3 });
    }
    items.push(FavoriteItemRecord { id: 5, group_id: 1, name: "Downloads".into(), target: home_location(b"Downloads".to_vec()), position: if screenshots_exists { 4 } else { 3 } });
    FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "Favorites".into(), position: 0 }], items }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoriteTargetValidation {
    Available { item_id: i64 },
    Unavailable { item_id: i64 },
    ProbeFailed { item_id: i64 },
}

/// The deliberately narrow P3-M9 handoff. History is intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub left: BrowserSnapshot,
    pub right: BrowserSnapshot,
    pub active_browser: BrowserSide,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowserSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSnapshot {
    pub location: Location,
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettingsSnapshot {
    pub bindings: Vec<ActionBinding>,
    pub folder_sorts: Vec<(Location, SortSpec)>,
    pub favorites: FavoritesRecords,
}

/// Application memory. It applies choices immediately and retains the latest
/// replaceable snapshot for an asynchronous settings service to persist.
#[derive(Debug, Default)]
pub struct SettingsState {
    bindings: HashMap<ActionId, Option<Shortcut>>,
    folder_sorts: HashMap<Location, SortSpec>,
    lru: VecDeque<Location>,
    favorites: FavoritesRecords,
    revision: u64,
    saved_revision: u64,
}

impl SettingsState {
    pub fn new() -> Self {
        let mut state = Self::default();
        state.apply(SettingsSnapshot { bindings: default_bindings(), ..SettingsSnapshot::default() });
        state.saved_revision = state.revision;
        state
    }
    pub fn sort_for(&mut self, location: &Location) -> SortSpec {
        if self.folder_sorts.contains_key(location) {
            self.touch(location);
        }
        self.folder_sorts.get(location).copied().unwrap_or_default()
    }
    pub fn set_sort(&mut self, location: Location, spec: SortSpec) {
        self.folder_sorts.insert(location.clone(), spec);
        self.touch(&location);
        while self.lru.len() > FOLDER_SORT_LIMIT {
            if let Some(oldest) = self.lru.pop_front() {
                self.folder_sorts.remove(&oldest);
            }
        }
        self.revision += 1;
    }
    pub fn binding(&self, action: ActionId) -> Option<Shortcut> {
        self.bindings.get(&action).copied().flatten().or_else(|| default_shortcut(action))
    }
    pub fn update_binding(&mut self, binding: ActionBinding) -> bool {
        let mut all = self.snapshot().bindings;
        all.retain(|current| current.action != binding.action);
        all.push(binding);
        let valid = validate_bindings(&all);
        if valid.len() != all.len() {
            return false;
        }
        self.bindings = valid.into_iter().map(|item| (item.action, item.shortcut)).collect();
        self.revision += 1;
        true
    }
    pub fn replace_favorites(&mut self, favorites: FavoritesRecords) {
        self.favorites = favorites;
        self.revision += 1;
    }
    pub fn snapshot(&self) -> SettingsSnapshot {
        let mut bindings = ActionId::ALL.into_iter().map(|action| ActionBinding { action, shortcut: self.bindings.get(&action).copied().flatten() }).collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.action.as_str());
        let folder_sorts = self.lru.iter().filter_map(|location| self.folder_sorts.get(location).copied().map(|sort| (location.clone(), sort))).collect();
        SettingsSnapshot { bindings, folder_sorts, favorites: self.favorites.clone() }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn mark_saved(&mut self, revision: u64) -> bool {
        if revision != self.revision || revision < self.saved_revision {
            return false;
        }
        self.saved_revision = revision;
        true
    }
    pub fn apply(&mut self, snapshot: SettingsSnapshot) {
        self.bindings = validate_bindings(&snapshot.bindings).into_iter().map(|binding| (binding.action, binding.shortcut)).collect();
        self.folder_sorts.clear();
        self.lru.clear();
        for (location, sort) in snapshot.folder_sorts.into_iter().take(FOLDER_SORT_LIMIT) {
            self.folder_sorts.insert(location.clone(), sort);
            self.lru.push_back(location);
        }
        self.favorites = snapshot.favorites;
        self.revision += 1;
    }
    fn touch(&mut self, location: &Location) {
        self.lru.retain(|candidate| candidate != location);
        self.lru.push_back(location.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::{EntryName, SortDirection, SortField};
    fn location(number: usize) -> Location {
        Location::root().join(&EntryName::new(format!("folder-{number}")).unwrap())
    }
    #[test]
    fn catalogue_is_stable_and_preloaded() {
        assert_eq!(ActionId::NewFolder.as_str(), "NewFolder");
        assert_eq!(ActionId::parse("NewFolder"), Some(ActionId::NewFolder));
        assert_eq!(SettingsState::new().snapshot().bindings.len(), ActionId::ALL.len());
    }
    #[test]
    fn invalid_shortcuts_fall_back_and_conflicts_are_rejected() {
        let mut state = SettingsState::new();
        assert!(!state.update_binding(ActionBinding { action: ActionId::NewFolder, shortcut: default_shortcut(ActionId::CloseWindow) }));
        assert_eq!(state.binding(ActionId::NewFolder), default_shortcut(ActionId::NewFolder));
    }
    #[test]
    fn lru_keeps_explicit_name_ascending_and_evicts_oldest() {
        let mut state = SettingsState::new();
        for i in 0..=FOLDER_SORT_LIMIT {
            state.set_sort(location(i), SortSpec::new(SortField::Name, SortDirection::Ascending));
        }
        assert_eq!(state.sort_for(&location(0)), SortSpec::default());
        assert_eq!(state.snapshot().folder_sorts.len(), FOLDER_SORT_LIMIT);
    }
    #[test]
    fn stale_save_is_rejected() {
        let mut state = SettingsState::new();
        let old = state.revision();
        state.set_sort(location(1), SortSpec::default());
        assert!(!state.mark_saved(old));
        assert!(state.mark_saved(state.revision()));
    }
    #[test]
    fn fresh_profile_has_the_product_order() {
        let favorites = fresh_profile_favorites(&location(42), false);
        assert_eq!(favorites.items.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), ["Applications", "Desktop", "Documents", "Downloads"]);
        assert!(favorites.initialized);
    }
}
