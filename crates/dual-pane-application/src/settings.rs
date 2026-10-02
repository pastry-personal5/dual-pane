//! Qt-free durable settings values and their in-memory policy.

use std::collections::{HashMap, VecDeque};

use dual_pane_domain::{BrowserSide, EntryName, FavoriteGroup, FavoriteGroupId, FavoriteItem, FavoriteItemId, Favorites, Location, SortSpec};

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
    NewTab,
    CloseTab,
    NavigateBack,
    NavigateForward,
    RefreshFolder,
    CopyToOtherBrowser,
    MoveToOtherBrowser,
    RenameItem,
    MoveToTrash,
    DeletePermanently,
    ShowPackageContents,
}

impl ActionId {
    /// Every catalogued action. Later IDs are appended, so a shortcut held by
    /// an earlier action keeps it when bindings are validated in this order.
    pub const ALL: [Self; 24] = [Self::FocusOtherBrowser, Self::NavigateParent, Self::CloseWindow, Self::QuitApplication, Self::NewFolder, Self::SortByNameAscending, Self::SortByNameDescending, Self::SortByTypeAscending, Self::SortByTypeDescending, Self::SortByDateAscending, Self::SortByDateDescending, Self::SortBySizeAscending, Self::SortBySizeDescending, Self::NewTab, Self::CloseTab, Self::NavigateBack, Self::NavigateForward, Self::RefreshFolder, Self::CopyToOtherBrowser, Self::MoveToOtherBrowser, Self::RenameItem, Self::MoveToTrash, Self::DeletePermanently, Self::ShowPackageContents];

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
            Self::NewTab => "NewTab",
            Self::CloseTab => "CloseTab",
            Self::NavigateBack => "NavigateBack",
            Self::NavigateForward => "NavigateForward",
            Self::RefreshFolder => "RefreshFolder",
            Self::CopyToOtherBrowser => "CopyToOtherBrowser",
            Self::MoveToOtherBrowser => "MoveToOtherBrowser",
            Self::RenameItem => "RenameItem",
            Self::MoveToTrash => "MoveToTrash",
            Self::DeletePermanently => "DeletePermanently",
            Self::ShowPackageContents => "ShowPackageContents",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

/// A platform-neutral key. Characters are uppercase ASCII letters or digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Up,
    Down,
    Left,
    Right,
    Return,
    Delete,
    /// A function key, F1 through F12.
    Function(u8),
}

impl Key {
    const NAMED: [(Self, &'static str); 6] = [(Self::Up, "Up"), (Self::Down, "Down"), (Self::Left, "Left"), (Self::Right, "Right"), (Self::Return, "Return"), (Self::Delete, "Delete")];

    pub fn is_valid(self) -> bool {
        match self {
            Self::Character(key) => key.is_ascii_uppercase() || key.is_ascii_digit(),
            Self::Function(number) => (1..=12).contains(&number),
            Self::Up | Self::Down | Self::Left | Self::Right | Self::Return | Self::Delete => true,
        }
    }

    /// The stable, non-localized text stored for this key.
    pub fn as_text(self) -> String {
        match self {
            Self::Character(key) => key.to_string(),
            Self::Function(number) => format!("F{number}"),
            named => Self::NAMED.iter().find(|(key, _)| *key == named).map_or_else(String::new, |(_, text)| (*text).to_owned()),
        }
    }

    /// Reads stored key text. Unknown text is `None`, never another key.
    pub fn parse(text: &str) -> Option<Self> {
        let mut characters = text.chars();
        if let (Some(key), None) = (characters.next(), characters.next()) {
            return Some(Self::Character(key));
        }
        if let Some((key, _)) = Self::NAMED.iter().find(|(_, name)| *name == text) {
            return Some(*key);
        }
        text.strip_prefix('F').and_then(|number| number.parse().ok()).map(Self::Function)
    }
}

/// A platform-neutral normalized shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub command: bool,
    pub shift: bool,
    pub option: bool,
    pub control: bool,
    pub key: Key,
}

impl Shortcut {
    pub const fn new(command: bool, shift: bool, option: bool, control: bool, key: Key) -> Self {
        Self { command, shift, option, control, key }
    }
    pub fn is_valid(self) -> bool {
        self.key.is_valid()
    }
    pub fn is_reserved(self) -> bool {
        self.command && matches!(self.key, Key::Character('H' | 'M'))
    }
}

pub const fn default_shortcut(action: ActionId) -> Option<Shortcut> {
    match action {
        ActionId::FocusOtherBrowser => Some(Shortcut::new(false, false, true, false, Key::Character('F'))),
        ActionId::NavigateParent => Some(Shortcut::new(true, false, false, false, Key::Up)),
        ActionId::QuitApplication => Some(Shortcut::new(true, false, false, false, Key::Character('Q'))),
        ActionId::NewFolder => Some(Shortcut::new(true, true, false, false, Key::Character('N'))),
        ActionId::NewTab => Some(Shortcut::new(true, false, false, false, Key::Character('T'))),
        ActionId::CloseTab => Some(Shortcut::new(true, false, false, false, Key::Character('W'))),
        ActionId::NavigateBack => Some(Shortcut::new(false, false, true, false, Key::Left)),
        ActionId::NavigateForward => Some(Shortcut::new(false, false, true, false, Key::Right)),
        ActionId::RefreshFolder => Some(Shortcut::new(true, false, false, false, Key::Character('R'))),
        ActionId::CopyToOtherBrowser => Some(Shortcut::new(false, false, true, false, Key::Character('C'))),
        ActionId::MoveToOtherBrowser => Some(Shortcut::new(false, false, true, false, Key::Character('M'))),
        ActionId::RenameItem => Some(Shortcut::new(false, false, false, false, Key::Function(2))),
        ActionId::MoveToTrash => Some(Shortcut::new(true, false, false, false, Key::Delete)),
        ActionId::DeletePermanently => Some(Shortcut::new(true, false, true, false, Key::Delete)),
        // Command+W closes a tab; Close Window keeps its meaning but has no
        // default shortcut. Show Package Contents is a Context Menu command.
        ActionId::CloseWindow | ActionId::ShowPackageContents | ActionId::SortByNameAscending | ActionId::SortByNameDescending | ActionId::SortByTypeAscending | ActionId::SortByTypeDescending | ActionId::SortByDateAscending | ActionId::SortByDateDescending | ActionId::SortBySizeAscending | ActionId::SortBySizeDescending => None,
    }
}

pub fn default_bindings() -> Vec<ActionBinding> {
    ActionId::ALL.into_iter().map(|action| ActionBinding { action, shortcut: default_shortcut(action) }).collect()
}

/// Adds each action missing from `accepted`, using its default unless that
/// default is already taken by an accepted binding.
fn complete_bindings(accepted: Vec<ActionBinding>) -> HashMap<ActionId, Option<Shortcut>> {
    let mut bindings = accepted.into_iter().map(|binding| (binding.action, binding.shortcut)).collect::<HashMap<_, _>>();
    for action in ActionId::ALL {
        if !bindings.contains_key(&action) {
            let default = default_shortcut(action).filter(|shortcut| !bindings.values().any(|taken| *taken == Some(*shortcut)));
            bindings.insert(action, default);
        }
    }
    bindings
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

fn fixed_name(value: &[u8]) -> EntryName {
    EntryName::new(value).unwrap_or_else(|_| unreachable!("fixed Favorite name is valid"))
}

/// The optional fresh-profile Screenshots target, `~/Documents/Screenshots`.
/// A worker probes it before the fresh-profile hierarchy is seeded.
pub fn fresh_profile_screenshots(home: &Location) -> Location {
    home.join(&fixed_name(b"Documents")).join(&fixed_name(b"Screenshots"))
}

/// The product's fresh-profile hierarchy. The caller reports whether the
/// optional Screenshots location exists, so this Qt-free model does no path
/// probing.
pub fn fresh_profile_favorites(home: &Location, screenshots_exists: bool) -> FavoritesRecords {
    let name = |value: Vec<u8>| fixed_name(&value);
    let home_location = |value| home.join(&name(value));
    let documents = home_location(b"Documents".to_vec());
    let mut items = vec![FavoriteItemRecord { id: 1, group_id: 1, name: "Applications".into(), target: Location::root().join(&name(b"Applications".to_vec())), position: 0 }, FavoriteItemRecord { id: 2, group_id: 1, name: "Desktop".into(), target: home_location(b"Desktop".to_vec()), position: 1 }, FavoriteItemRecord { id: 3, group_id: 1, name: "Documents".into(), target: documents.clone(), position: 2 }];
    if screenshots_exists {
        items.push(FavoriteItemRecord { id: 4, group_id: 1, name: "Screenshots".into(), target: fresh_profile_screenshots(home), position: 3 });
    }
    items.push(FavoriteItemRecord { id: 5, group_id: 1, name: "Downloads".into(), target: home_location(b"Downloads".to_vec()), position: if screenshots_exists { 4 } else { 3 } });
    FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "Favorites".into(), position: 0 }], items }
}

/// The result of checking a Favorite Item's target at launch. Only a
/// completed check that found the target missing or unavailable may remove
/// the Item; a failed or cancelled check proves nothing about the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteProbeOutcome {
    Available,
    Unavailable,
    Failed,
    Cancelled,
}

/// Why durable settings could not be loaded. The application continues with
/// compiled defaults in memory and never saves over the stored settings until
/// a later load succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFailure {
    /// No settings service could be started.
    WorkerUnavailable,
    /// The database could not be opened or read, for example because of
    /// permissions or a full disk.
    Unavailable,
    /// The database or one of its records is damaged.
    Corrupt,
    /// The database was written by a newer, unsupported version.
    UnsupportedSchema,
}

/// Whether the stored settings are known. Saves are allowed only after a
/// successful load, so a pending or failed load can never overwrite them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsStatus {
    Loading,
    Loaded,
    LoadFailed(SettingsFailure),
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
    /// The remembered sort for `location` without counting it as a use.
    pub fn peek_sort(&self, location: &Location) -> SortSpec {
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
    /// The effective shortcut. `None` means the action is deliberately
    /// unbound; an action with no recorded choice uses its default.
    pub fn binding(&self, action: ActionId) -> Option<Shortcut> {
        self.bindings.get(&action).copied().unwrap_or_else(|| default_shortcut(action))
    }
    /// The effective shortcut of every catalogued action, in catalogue order.
    pub fn bindings(&self) -> Vec<ActionBinding> {
        ActionId::ALL.into_iter().map(|action| ActionBinding { action, shortcut: self.binding(action) }).collect()
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
    pub fn favorites(&self) -> &FavoritesRecords {
        &self.favorites
    }
    pub fn snapshot(&self) -> SettingsSnapshot {
        let mut bindings = ActionId::ALL.into_iter().map(|action| ActionBinding { action, shortcut: self.binding(action) }).collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.action.as_str());
        let folder_sorts = self.lru.iter().filter_map(|location| self.folder_sorts.get(location).copied().map(|sort| (location.clone(), sort))).collect();
        SettingsSnapshot { bindings, folder_sorts, favorites: self.favorites.clone() }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn has_unsaved_changes(&self) -> bool {
        self.revision != self.saved_revision
    }
    pub fn mark_saved(&mut self, revision: u64) -> bool {
        if revision != self.revision || revision < self.saved_revision {
            return false;
        }
        self.saved_revision = revision;
        true
    }
    pub fn apply(&mut self, snapshot: SettingsSnapshot) {
        self.bindings = complete_bindings(validate_bindings(&snapshot.bindings));
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

impl FavoritesRecords {
    pub fn hierarchy(&self) -> Result<Favorites, dual_pane_domain::FavoriteError> {
        Favorites::new(self.groups.iter().map(|group| FavoriteGroup { id: FavoriteGroupId::new(group.id), name: group.name.clone() }).collect(), self.items.iter().map(|item| FavoriteItem { id: FavoriteItemId::new(item.id), group_id: FavoriteGroupId::new(item.group_id), name: item.name.clone(), target: item.target.clone() }).collect())
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
        assert!(!state.update_binding(ActionBinding { action: ActionId::NewFolder, shortcut: default_shortcut(ActionId::CloseTab) }));
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
    fn key_text_round_trips_and_unknown_text_is_rejected() {
        for key in [Key::Character('N'), Key::Character('7'), Key::Up, Key::Down, Key::Left, Key::Right, Key::Return, Key::Delete, Key::Function(2), Key::Function(12)] {
            assert_eq!(Key::parse(&key.as_text()), Some(key));
        }
        assert_eq!(Key::parse("Escape"), None);
        assert_eq!(Key::parse(""), None);
        assert!(!Key::Function(13).is_valid());
        assert!(!Key::Character('n').is_valid());
    }
    #[test]
    fn delivered_actions_have_their_documented_defaults_without_collisions() {
        let shortcut = |command, option, key| Some(Shortcut::new(command, false, option, false, key));
        assert_eq!(default_shortcut(ActionId::NewTab), shortcut(true, false, Key::Character('T')));
        assert_eq!(default_shortcut(ActionId::CloseTab), shortcut(true, false, Key::Character('W')));
        assert_eq!(default_shortcut(ActionId::NavigateBack), shortcut(false, true, Key::Left));
        assert_eq!(default_shortcut(ActionId::NavigateForward), shortcut(false, true, Key::Right));
        assert_eq!(default_shortcut(ActionId::RefreshFolder), shortcut(true, false, Key::Character('R')));
        assert_eq!(default_shortcut(ActionId::CloseWindow), None);
        assert_eq!(validate_bindings(&default_bindings()).len(), ActionId::ALL.len());
        assert!(ActionId::ALL.iter().all(|action| ActionId::parse(action.as_str()) == Some(*action)));
    }
    #[test]
    fn file_commands_have_their_decided_defaults() {
        let shortcut = |command, option, key| Some(Shortcut::new(command, false, option, false, key));
        assert_eq!(default_shortcut(ActionId::CopyToOtherBrowser), shortcut(false, true, Key::Character('C')));
        assert_eq!(default_shortcut(ActionId::MoveToOtherBrowser), shortcut(false, true, Key::Character('M')));
        assert_eq!(default_shortcut(ActionId::RenameItem), shortcut(false, false, Key::Function(2)));
        assert_eq!(default_shortcut(ActionId::MoveToTrash), shortcut(true, false, Key::Delete));
        assert_eq!(default_shortcut(ActionId::DeletePermanently), shortcut(true, true, Key::Delete));
        assert_eq!(default_shortcut(ActionId::ShowPackageContents), None);
        assert_eq!(&ActionId::ALL[ActionId::ALL.len() - 6..], &[ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::RenameItem, ActionId::MoveToTrash, ActionId::DeletePermanently, ActionId::ShowPackageContents], "new IDs are appended");
    }
    #[test]
    fn settings_saved_before_the_file_commands_load_their_defaults() {
        let older = ActionId::ALL[..18].iter().map(|action| ActionBinding { action: *action, shortcut: default_shortcut(*action) }).collect::<Vec<_>>();
        let mut state = SettingsState::new();
        state.apply(SettingsSnapshot { bindings: older, ..SettingsSnapshot::default() });
        for action in [ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::RenameItem, ActionId::MoveToTrash, ActionId::DeletePermanently, ActionId::ShowPackageContents] {
            assert_eq!(state.binding(action), default_shortcut(action), "{action:?}");
        }
        let taken = ActionBinding { action: ActionId::NewTab, shortcut: default_shortcut(ActionId::RenameItem) };
        state.apply(SettingsSnapshot { bindings: vec![taken], ..SettingsSnapshot::default() });
        assert_eq!(state.binding(ActionId::RenameItem), None, "a default already chosen for another action is not reused");
    }
    #[test]
    fn navigate_parent_defaults_to_command_up() {
        assert_eq!(default_shortcut(ActionId::NavigateParent), Some(Shortcut::new(true, false, false, false, Key::Up)));
    }
    #[test]
    fn an_unbound_action_stays_unbound_through_a_snapshot() {
        let mut state = SettingsState::new();
        assert!(state.update_binding(ActionBinding { action: ActionId::NewFolder, shortcut: None }));
        assert_eq!(state.binding(ActionId::NewFolder), None);
        let mut restored = SettingsState::new();
        restored.apply(state.snapshot());
        assert_eq!(restored.binding(ActionId::NewFolder), None);
        assert_eq!(restored.binding(ActionId::CloseTab), default_shortcut(ActionId::CloseTab));
    }
    #[test]
    fn a_missing_binding_uses_its_default_unless_that_default_is_taken() {
        let taken = ActionBinding { action: ActionId::NewFolder, shortcut: default_shortcut(ActionId::CloseTab) };
        let mut state = SettingsState::new();
        state.apply(SettingsSnapshot { bindings: vec![taken], ..SettingsSnapshot::default() });
        assert_eq!(state.binding(ActionId::NewFolder), default_shortcut(ActionId::CloseTab));
        assert_eq!(state.binding(ActionId::CloseTab), None);
        assert_eq!(state.binding(ActionId::QuitApplication), default_shortcut(ActionId::QuitApplication));
    }
    #[test]
    fn fresh_profile_has_the_product_order() {
        let favorites = fresh_profile_favorites(&location(42), false);
        assert_eq!(favorites.items.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), ["Applications", "Desktop", "Documents", "Downloads"]);
        assert!(favorites.initialized);
    }
}
