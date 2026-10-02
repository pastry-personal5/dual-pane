use std::sync::Arc;

use crate::{Command, Event, FavoriteGroupRecord, FavoriteItemRecord, FavoriteProbeOutcome, FavoritesRecords, Input, Output, RowChange, SettingsFailure, SettingsState, SettingsStatus, WorkRequest, fresh_profile_favorites, fresh_profile_screenshots};
use dual_pane_domain::{BrowserSide, BrowserTabs, Entry, EntryName, ListingError, ListingErrorKind, Location, RequestToken, ScrollAnchor, Selection, SortSpec, TabHistory, TabId, Visit, VisitState, valid_favorite_name};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Transition {
    pub outputs: Vec<Output>,
    pub work: Vec<WorkRequest>,
}

#[derive(Debug)]
pub struct Workspace {
    left: BrowserState,
    right: BrowserState,
    active_browser: BrowserSide,
    home: Location,
    settings: SettingsState,
    settings_status: SettingsStatus,
    /// Sort choices made before the stored settings loaded. They are replayed
    /// over the loaded snapshot so the person's latest choice wins.
    pending_sorts: Vec<(Location, SortSpec)>,
    /// The Screenshots folder being probed before uninitialized Favorites are
    /// seeded. Favorites edits wait for the probe, as they wait for a load.
    screenshots_probe: Option<Location>,
    last_token: Option<RequestToken>,
    next_tab: u64,
    next_favorite_group_id: i64,
    next_favorite_item_id: i64,
}
/// One Browser: the domain's ordered tab identities and active tab, plus the
/// application's per-tab state, which is stored in no particular order.
#[derive(Debug)]
struct BrowserState {
    order: BrowserTabs,
    tabs: Vec<TabState>,
}
/// One tab: its domain history of visited folders, the Folder Items it shows,
/// and its outstanding read.
#[derive(Debug)]
struct TabState {
    id: TabId,
    history: TabHistory,
    folder_items: Option<FolderItems>,
    pending: Option<PendingRead>,
    error: Option<ListingError>,
}
#[derive(Debug)]
struct FolderItems {
    location: Location,
    entries: Arc<[Entry]>,
}
#[derive(Debug)]
struct PendingRead {
    token: RequestToken,
    location: Location,
    history_target: Option<usize>,
    /// The shown Folder Items the read was asked to diff against.
    base: Option<Arc<[Entry]>>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::with_home(Location::root())
    }
}
impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_home(home: Location) -> Self {
        let left_id = TabId::new(0);
        let right_id = TabId::new(1);
        Self { left: BrowserState::new(left_id), right: BrowserState::new(right_id), active_browser: BrowserSide::Left, home, settings: SettingsState::new(), settings_status: SettingsStatus::Loading, pending_sorts: Vec::new(), screenshots_probe: None, last_token: None, next_tab: 2, next_favorite_group_id: 1, next_favorite_item_id: 1 }
    }
    pub fn handle(&mut self, input: Input) -> Transition {
        match input {
            Input::Command(Command::ActivateBrowser { browser }) => self.activate_browser(browser),
            Input::Command(Command::ActivateTab { browser, tab }) => self.activate_tab(browser, tab),
            Input::Command(Command::NewTab { browser }) => self.new_tab(browser),
            Input::Command(Command::CloseTab { browser, tab }) => self.close_tab(browser, tab),
            Input::Command(Command::ReorderTab { browser, tab, position }) => self.reorder_tab(browser, tab, position),
            Input::Command(Command::Navigate { browser, location }) => self.navigate(browser, location, None),
            Input::Command(Command::GoBack { browser }) => self.travel(browser, false),
            Input::Command(Command::GoForward { browser }) => self.travel(browser, true),
            Input::Command(Command::Refresh { browser }) => self.refresh_tab(browser, self.active_tab(browser)),
            Input::Command(Command::SetSort { location, sort }) => self.set_sort(location, sort),
            Input::Command(Command::CreateFavoriteGroup { name }) => self.create_favorite_group(name),
            Input::Command(Command::RenameFavoriteGroup { id, name }) => self.rename_favorite_group(id, name),
            Input::Command(Command::ReorderFavoriteGroup { id, position }) => self.reorder_favorite_group(id, position),
            Input::Command(Command::DeleteFavoriteGroup { id }) => self.delete_favorite_group(id),
            Input::Command(Command::CreateFavoriteItem { group_id, name, target }) => self.create_favorite_item(group_id, name, target),
            Input::Command(Command::RenameFavoriteItem { id, name }) => self.rename_favorite_item(id, name),
            Input::Command(Command::MoveFavoriteItem { id, group_id, position }) => self.move_favorite_item(id, group_id, position),
            Input::Command(Command::DeleteFavoriteItem { id }) => self.delete_favorite_item(id),
            Input::Command(Command::OpenFavoriteItem { browser, id }) => self.open_favorite_item(browser, id),
            Input::Command(Command::SelectEntry { browser, row, name }) => self.select(browser, row, &name, false),
            Input::Command(Command::ToggleEntry { browser, row, name }) => self.select(browser, row, &name, true),
            Input::Command(Command::SelectAll { browser }) => self.select_all(browser),
            Input::Command(Command::ClearSelection { browser }) => self.clear_selection(browser),
            Input::Command(Command::OpenEntry { browser, row, name }) => self.open_entry(browser, row, &name),
            Input::Command(Command::GoToParent { browser }) => self.go_to_parent(browser),
            Input::Command(Command::UpdateScrollHint { browser, scroll }) => self.scroll(browser, scroll),
            Input::Command(Command::SelectRange { browser, row, name }) => self.select_range(browser, row, &name),
            Input::Command(Command::MoveSelection { browser, row, name }) => self.move_selection(browser, row, &name),
            Input::Command(Command::SecondarySelect { browser, row, name }) => self.secondary_select(browser, row, &name),
            Input::Event(Event::LocationInvalidated { location }) => self.refresh_location(&location),
            Input::Event(Event::FolderItemsLoaded { browser, tab, token, entries, changes }) => self.loaded(browser, tab, token, entries, changes),
            Input::Event(Event::FolderItemsFailed { browser, tab, token, kind }) => self.failed(browser, tab, token, kind),
            Input::Event(Event::FolderItemsCancelled { browser, tab, token }) => self.cancelled(browser, tab, token),
            Input::Event(Event::FavoriteTargetProbed { item_id, target, outcome }) => self.favorite_target_probed(item_id, target, outcome),
            Input::Event(Event::ScreenshotsFolderProbed { location, outcome }) => self.screenshots_folder_probed(&location, outcome),
            Input::Event(Event::SettingsSaved { revision }) => {
                self.settings.mark_saved(revision);
                Transition::default()
            }
            Input::Event(Event::SettingsSaveFailed { revision }) => Transition { outputs: vec![Output::SettingsSaveFailed { revision }], work: vec![] },
            Input::Event(Event::SettingsLoaded { snapshot }) => self.settings_loaded(snapshot),
            Input::Event(Event::SettingsLoadFailed { failure }) => self.settings_load_failed(failure),
        }
    }
    pub fn active_browser(&self) -> BrowserSide {
        self.active_browser
    }
    pub fn active_tab(&self, side: BrowserSide) -> TabId {
        self.browser(side).order.active()
    }
    /// The Browser's tabs in display order.
    pub fn tabs(&self, side: BrowserSide) -> impl ExactSizeIterator<Item = TabId> + '_ {
        self.browser(side).order.tabs().iter().copied()
    }
    /// Whether Back would move the active tab, counting a Back or Forward that
    /// is still loading as already taken.
    pub fn can_go_back(&self, side: BrowserSide) -> bool {
        self.history_step(side, false).is_some()
    }
    /// Whether Forward would move the active tab; see [`Self::can_go_back`].
    pub fn can_go_forward(&self, side: BrowserSide) -> bool {
        self.history_step(side, true).is_some()
    }
    pub fn location(&self, side: BrowserSide) -> Option<&Location> {
        self.tab(side, self.active_tab(side)).and_then(|t| t.folder_items.as_ref().map(|x| &x.location))
    }
    pub fn entries(&self, side: BrowserSide) -> &[Entry] {
        self.tab(side, self.active_tab(side)).and_then(|t| t.folder_items.as_ref()).map_or(&[], |x| &x.entries)
    }
    pub fn loading_location(&self, side: BrowserSide) -> Option<&Location> {
        self.tab(side, self.active_tab(side)).and_then(|t| t.pending.as_ref().map(|x| &x.location))
    }
    pub fn selection(&self, side: BrowserSide) -> &Selection {
        static NONE: Selection = Selection::new();
        self.tab(side, self.active_tab(side)).and_then(|tab| tab.history.current()).map_or(&NONE, |visit| visit.state().selection())
    }
    pub fn favorites(&self) -> &FavoritesRecords {
        self.settings.favorites()
    }
    pub fn settings_status(&self) -> SettingsStatus {
        self.settings_status
    }
    /// The save to flush before the application exits, if loaded settings
    /// have changed since the last confirmed save.
    pub fn final_settings_save(&self) -> Option<WorkRequest> {
        self.settings.has_unsaved_changes().then(|| self.save_settings()).flatten()
    }
    fn browser(&self, side: BrowserSide) -> &BrowserState {
        match side {
            BrowserSide::Left => &self.left,
            BrowserSide::Right => &self.right,
        }
    }
    fn browser_mut(&mut self, side: BrowserSide) -> &mut BrowserState {
        match side {
            BrowserSide::Left => &mut self.left,
            BrowserSide::Right => &mut self.right,
        }
    }
    fn tab(&self, side: BrowserSide, id: TabId) -> Option<&TabState> {
        self.browser(side).tabs.iter().find(|t| t.id == id)
    }
    fn tab_mut(&mut self, side: BrowserSide, id: TabId) -> Option<&mut TabState> {
        self.browser_mut(side).tabs.iter_mut().find(|t| t.id == id)
    }
    fn activate_browser(&mut self, browser: BrowserSide) -> Transition {
        if self.active_browser == browser {
            Transition::default()
        } else {
            self.active_browser = browser;
            Transition { outputs: vec![Output::ActiveBrowserChanged { browser }], work: vec![] }
        }
    }
    fn activate_tab(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        if !self.browser_mut(browser).order.activate(tab) {
            return Transition::default();
        }
        let mut outputs = vec![Output::ActiveTabChanged { browser, tab }];
        if self.active_browser != browser {
            self.active_browser = browser;
            outputs.insert(0, Output::ActiveBrowserChanged { browser });
        }
        outputs.push(self.tab_view(browser, tab));
        Transition { outputs, work: vec![] }
    }
    fn new_tab(&mut self, browser: BrowserSide) -> Transition {
        let source = self.location(browser).cloned();
        let Some(location) = source else {
            return Transition::default();
        };
        let id = TabId::new(self.next_tab);
        self.next_tab += 1;
        let state = self.browser_mut(browser);
        state.tabs.push(TabState::new(id));
        state.order.insert_active(id);
        let mut transition = self.navigate(browser, location, None);
        transition.outputs.insert(0, Output::TabsChanged { browser, active_tab: id });
        transition.outputs.insert(1, self.tab_view(browser, id));
        transition
    }
    fn close_tab(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        let Some(pending) = self.tab(browser, tab).map(|state| state.pending.as_ref().map(|p| p.token)) else {
            return Transition::default();
        };
        let was_active = self.active_tab(browser) == tab;
        let final_tab = self.browser(browser).order.tabs().len() == 1;
        let replacement = if final_tab {
            let id = TabId::new(self.next_tab);
            self.next_tab += 1;
            let state = self.browser_mut(browser);
            state.order.replace_final(tab, id);
            state.tabs = vec![TabState::new(id)];
            id
        } else {
            let state = self.browser_mut(browser);
            state.tabs.retain(|item| item.id != tab);
            match state.order.close(tab) {
                Some(active) => active,
                None => return Transition::default(),
            }
        };
        let mut work: Vec<WorkRequest> = pending.into_iter().map(|token| WorkRequest::Cancel { browser, tab, token }).collect();
        let mut outputs = vec![Output::TabsChanged { browser, active_tab: replacement }];
        if was_active {
            outputs.push(self.tab_view(browser, replacement));
        }
        if final_tab {
            let mut load = self.navigate(browser, self.home.clone(), None);
            outputs.append(&mut load.outputs);
            work.append(&mut load.work);
        }
        Transition { outputs, work }
    }
    fn reorder_tab(&mut self, browser: BrowserSide, tab: TabId, position: usize) -> Transition {
        let state = self.browser_mut(browser);
        if !state.order.reorder(tab, position) {
            return Transition::default();
        }
        Transition { outputs: vec![Output::TabsChanged { browser, active_tab: state.order.active() }], work: vec![] }
    }
    fn navigate(&mut self, browser: BrowserSide, location: Location, history_target: Option<usize>) -> Transition {
        let tab = self.active_tab(browser);
        if self.tab(browser, tab).and_then(|t| t.pending.as_ref()).is_some_and(|p| p.location == location && p.history_target == history_target) {
            return Transition::default();
        }
        self.start_read(browser, tab, location, history_target)
    }
    fn start_read(&mut self, browser: BrowserSide, tab: TabId, location: Location, history_target: Option<usize>) -> Transition {
        let token = self.last_token.map_or_else(RequestToken::first, RequestToken::next);
        self.last_token = Some(token);
        let sort = self.settings.sort_for(&location);
        let previous = self.tab(browser, tab).and_then(|state| state.folder_items.as_ref()).filter(|items| items.location == location).map(|items| Arc::clone(&items.entries));
        let old = self.tab_mut(browser, tab).and_then(|state| {
            state.error = None;
            state.pending.replace(PendingRead { token, location: location.clone(), history_target, base: previous.clone() })
        });
        let mut work = old.into_iter().map(|old| WorkRequest::Cancel { browser, tab, token: old.token }).collect::<Vec<_>>();
        work.push(WorkRequest::ReadDirectory { browser, tab, token, location: location.clone(), sort, previous });
        Transition { outputs: vec![Output::LoadingStarted { browser, tab, location }], work }
    }
    /// The history entry one step back or forward from the active tab. A Back
    /// or Forward that is still loading counts as taken, so repeated presses
    /// keep moving instead of re-requesting the same entry.
    fn history_step(&self, browser: BrowserSide, forward: bool) -> Option<(usize, Location)> {
        let tab = self.tab(browser, self.active_tab(browser))?;
        let base = tab.pending.as_ref().and_then(|pending| pending.history_target).or(tab.history.current_index())?;
        let target = tab.history.neighbor(base, forward)?;
        tab.history.get(target).map(|visit| (target, visit.location().clone()))
    }
    fn travel(&mut self, browser: BrowserSide, forward: bool) -> Transition {
        let tab = self.active_tab(browser);
        self.history_step(browser, forward).map_or_else(Transition::default, |(target, location)| self.start_read(browser, tab, location, Some(target)))
    }
    fn refresh_tab(&mut self, browser: BrowserSide, tab: TabId) -> Transition {
        let target = self.tab(browser, tab).and_then(|state| state.pending.as_ref().map(|pending| (pending.location.clone(), pending.history_target)).or_else(|| state.folder_items.as_ref().map(|items| (items.location.clone(), None))));
        target.map_or_else(Transition::default, |(location, history_target)| self.start_read(browser, tab, location, history_target))
    }
    fn refresh_location(&mut self, location: &Location) -> Transition {
        let targets = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|side| self.browser(side).tabs.iter().filter(move |tab| tab.pending.as_ref().map(|pending| &pending.location).or_else(|| tab.folder_items.as_ref().map(|items| &items.location)) == Some(location)).map(move |tab| (side, tab.id))).collect::<Vec<_>>();
        targets.into_iter().fold(Transition::default(), |mut total, (side, tab)| {
            let next = self.refresh_tab(side, tab);
            total.outputs.extend(next.outputs);
            total.work.extend(next.work);
            total
        })
    }
    fn set_sort(&mut self, location: Location, sort: SortSpec) -> Transition {
        self.settings.set_sort(location.clone(), sort);
        if self.settings_status == SettingsStatus::Loading {
            self.pending_sorts.push((location.clone(), sort));
        }
        let mut transition = self.refresh_location(&location);
        transition.work.extend(self.save_settings());
        transition
    }
    fn settings_loaded(&mut self, snapshot: crate::SettingsSnapshot) -> Transition {
        self.settings.apply(snapshot);
        // The loaded snapshot is what storage already holds.
        self.settings.mark_saved(self.settings.revision());
        self.settings_status = SettingsStatus::Loaded;
        let replayed = !self.pending_sorts.is_empty();
        for (location, sort) in std::mem::take(&mut self.pending_sorts) {
            self.settings.set_sort(location, sort);
        }
        // A fresh profile is seeded once its optional Screenshots folder has
        // been probed off the GUI thread.
        self.screenshots_probe = (!self.settings.favorites().initialized).then(|| fresh_profile_screenshots(&self.home));
        self.reserve_favorite_ids();
        let tabs = [BrowserSide::Left, BrowserSide::Right].into_iter().flat_map(|side| self.browser(side).tabs.iter().map(move |tab| (side, tab.id))).collect::<Vec<_>>();
        let mut transition = Transition { outputs: vec![Output::FavoritesChanged { favorites: self.settings.favorites().clone() }], work: if replayed { self.save_settings().into_iter().collect() } else { Vec::new() } };
        transition.work.extend(self.screenshots_probe.clone().map(|location| WorkRequest::ProbeScreenshotsFolder { location }));
        for (side, tab) in tabs {
            let refresh = self.refresh_tab(side, tab);
            transition.outputs.extend(refresh.outputs);
            transition.work.extend(refresh.work);
        }
        transition
    }
    /// Seeds the fresh-profile Favorites once the matching Screenshots probe
    /// completes. Only a probe that found the folder adds Screenshots.
    fn screenshots_folder_probed(&mut self, location: &Location, outcome: FavoriteProbeOutcome) -> Transition {
        if self.screenshots_probe.as_ref() != Some(location) {
            return Transition::default();
        }
        self.screenshots_probe = None;
        if self.settings_status != SettingsStatus::Loaded || self.settings.favorites().initialized {
            return Transition::default();
        }
        self.settings.replace_favorites(fresh_profile_favorites(&self.home, outcome == FavoriteProbeOutcome::Available));
        self.reserve_favorite_ids();
        Transition { outputs: vec![Output::FavoritesChanged { favorites: self.settings.favorites().clone() }], work: self.save_settings().into_iter().collect() }
    }
    /// Keeps new Favorite IDs above every ID in the current collection.
    fn reserve_favorite_ids(&mut self) {
        self.next_favorite_group_id = self.next_favorite_group_id.max(self.settings.favorites().groups.iter().map(|group| group.id.saturating_add(1)).max().unwrap_or(1));
        self.next_favorite_item_id = self.next_favorite_item_id.max(self.settings.favorites().items.iter().map(|item| item.id.saturating_add(1)).max().unwrap_or(1));
    }
    fn settings_load_failed(&mut self, failure: SettingsFailure) -> Transition {
        self.settings_status = SettingsStatus::LoadFailed(failure);
        self.screenshots_probe = None;
        self.pending_sorts.clear();
        Transition { outputs: vec![Output::SettingsLoadFailed { failure }], work: vec![] }
    }
    fn create_favorite_group(&mut self, name: String) -> Transition {
        if !valid_favorite_name(&name) || self.settings.favorites().groups.iter().any(|group| group.name == name) {
            return Transition::default();
        }
        let mut favorites = self.settings.favorites().clone();
        let Some(next_id) = self.next_favorite_group_id.checked_add(1) else { return Transition::default() };
        let id = self.next_favorite_group_id;
        let position = favorites.groups.len() as i64;
        favorites.groups.push(FavoriteGroupRecord { id, name, position });
        let transition = self.commit_favorites(favorites);
        if !transition.outputs.is_empty() {
            self.next_favorite_group_id = next_id;
        }
        transition
    }
    fn rename_favorite_group(&mut self, id: i64, name: String) -> Transition {
        if !valid_favorite_name(&name) {
            return Transition::default();
        }
        let mut favorites = self.settings.favorites().clone();
        if favorites.groups.iter().any(|group| group.id != id && group.name == name) {
            return Transition::default();
        }
        let Some(group) = favorites.groups.iter_mut().find(|group| group.id == id) else { return Transition::default() };
        if group.name == name {
            return Transition::default();
        }
        group.name = name;
        self.commit_favorites(favorites)
    }
    fn reorder_favorite_group(&mut self, id: i64, position: usize) -> Transition {
        let mut favorites = self.settings.favorites().clone();
        let Some(index) = favorites.groups.iter().position(|group| group.id == id) else { return Transition::default() };
        let group = favorites.groups.remove(index);
        favorites.groups.insert(position.min(favorites.groups.len()), group);
        normalize_group_positions(&mut favorites);
        self.commit_favorites(favorites)
    }
    fn delete_favorite_group(&mut self, id: i64) -> Transition {
        let mut favorites = self.settings.favorites().clone();
        let old = favorites.groups.len();
        favorites.groups.retain(|group| group.id != id);
        if old == favorites.groups.len() {
            return Transition::default();
        }
        favorites.items.retain(|item| item.group_id != id);
        normalize_group_positions(&mut favorites);
        self.commit_favorites(favorites)
    }
    fn create_favorite_item(&mut self, group_id: i64, name: String, target: Location) -> Transition {
        if !valid_favorite_name(&name) {
            return Transition::default();
        }
        let mut favorites = self.settings.favorites().clone();
        if !favorites.groups.iter().any(|group| group.id == group_id) || favorites.items.iter().any(|item| item.group_id == group_id && item.name == name) {
            return Transition::default();
        }
        let Some(next_id) = self.next_favorite_item_id.checked_add(1) else { return Transition::default() };
        let id = self.next_favorite_item_id;
        let position = favorites.items.iter().filter(|item| item.group_id == group_id).count() as i64;
        favorites.items.push(FavoriteItemRecord { id, group_id, name, target, position });
        let transition = self.commit_favorites(favorites);
        if !transition.outputs.is_empty() {
            self.next_favorite_item_id = next_id;
        }
        transition
    }
    fn rename_favorite_item(&mut self, id: i64, name: String) -> Transition {
        if !valid_favorite_name(&name) {
            return Transition::default();
        }
        let mut favorites = self.settings.favorites().clone();
        let Some(index) = favorites.items.iter().position(|item| item.id == id) else { return Transition::default() };
        let group_id = favorites.items[index].group_id;
        if favorites.items.iter().any(|item| item.id != id && item.group_id == group_id && item.name == name) {
            return Transition::default();
        }
        if favorites.items[index].name == name {
            return Transition::default();
        }
        favorites.items[index].name = name;
        self.commit_favorites(favorites)
    }
    fn move_favorite_item(&mut self, id: i64, group_id: i64, position: usize) -> Transition {
        let mut favorites = self.settings.favorites().clone();
        if !favorites.groups.iter().any(|group| group.id == group_id) {
            return Transition::default();
        }
        let Some(index) = favorites.items.iter().position(|item| item.id == id) else { return Transition::default() };
        let mut item = favorites.items.remove(index);
        if favorites.items.iter().any(|other| other.group_id == group_id && other.name == item.name) {
            return Transition::default();
        }
        let same_group = item.group_id == group_id;
        item.group_id = group_id;
        // Past the group's end, the Item goes right after the group's last
        // Item, or back to its own index, so a move to the current position
        // leaves the records unchanged.
        let group_rows = favorites.items.iter().enumerate().filter(|(_, other)| other.group_id == group_id).map(|(row, _)| row).collect::<Vec<_>>();
        let target_index = group_rows.get(position).copied().or_else(|| group_rows.last().map(|last| last + 1)).unwrap_or(if same_group { index } else { favorites.items.len() });
        favorites.items.insert(target_index, item);
        normalize_item_positions(&mut favorites);
        self.commit_favorites(favorites)
    }
    fn delete_favorite_item(&mut self, id: i64) -> Transition {
        let mut favorites = self.settings.favorites().clone();
        let old = favorites.items.len();
        favorites.items.retain(|item| item.id != id);
        if old == favorites.items.len() {
            return Transition::default();
        }
        normalize_item_positions(&mut favorites);
        self.commit_favorites(favorites)
    }
    fn open_favorite_item(&mut self, browser: BrowserSide, id: i64) -> Transition {
        self.settings.favorites().items.iter().find(|item| item.id == id).map(|item| item.target.clone()).map_or_else(Transition::default, |target| self.navigate(browser, target, None))
    }
    fn favorite_target_probed(&mut self, item_id: i64, target: Location, outcome: FavoriteProbeOutcome) -> Transition {
        if outcome != FavoriteProbeOutcome::Unavailable {
            return Transition::default();
        }
        let Some(item) = self.settings.favorites().items.iter().find(|item| item.id == item_id && item.target == target) else { return Transition::default() };
        self.delete_favorite_item(item.id)
    }
    fn commit_favorites(&mut self, mut favorites: FavoritesRecords) -> Transition {
        // Favorites are unknown until the stored collection loads and, for a
        // fresh profile, is seeded, so an edit now could not be merged with it.
        if self.settings_status == SettingsStatus::Loading || self.screenshots_probe.is_some() {
            return Transition::default();
        }
        favorites.initialized = true;
        // An edit that changes nothing, such as moving an Item to its own
        // position, neither reports a change nor queues a save.
        if favorites == *self.settings.favorites() || favorites.hierarchy().is_err() {
            return Transition::default();
        }
        self.settings.replace_favorites(favorites.clone());
        Transition { outputs: vec![Output::FavoritesChanged { favorites }], work: self.save_settings().into_iter().collect() }
    }
    /// A save of the current settings, or `None` unless the stored settings
    /// loaded successfully. Saving after a pending or failed load would replace
    /// records the application never read.
    fn save_settings(&self) -> Option<WorkRequest> {
        (self.settings_status == SettingsStatus::Loaded).then(|| WorkRequest::SaveSettings { revision: self.settings.revision(), snapshot: self.settings.snapshot() })
    }
    fn select(&mut self, browser: BrowserSide, row: usize, name: &EntryName, toggle: bool) -> Transition {
        self.change_visit(browser, Some(row), |visit, entries| if toggle { visit.toggle(entries, row, name) } else { visit.select(entries, row, name) })
    }
    fn select_all(&mut self, browser: BrowserSide) -> Transition {
        self.change_visit(browser, None, VisitState::select_all)
    }
    fn select_range(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        self.change_visit(browser, Some(row), |visit, entries| visit.select_range(entries, row, name))
    }
    fn move_selection(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        self.select(browser, row, name, false)
    }
    fn secondary_select(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        self.change_visit(browser, Some(row), |visit, entries| visit.select_secondary(entries, row, name))
    }
    fn clear_selection(&mut self, browser: BrowserSide) -> Transition {
        let tab = self.active_tab(browser);
        let Some(visit) = self.tab_mut(browser, tab).and_then(|state| state.history.current_mut()) else { return Transition::default() };
        if visit.state_mut().clear_selection() { Transition { outputs: vec![Output::SelectionChanged { browser, tab, selection: visit.state().selection().clone(), row: None }], work: vec![] } } else { Transition::default() }
    }
    /// Applies one selection gesture to the active tab's current visit against
    /// its shown Folder Items, and reports the selection when it changed.
    /// `cursor_row` is the validated row a gesture moves the cursor to; `None`
    /// looks the unchanged cursor up instead.
    fn change_visit(&mut self, browser: BrowserSide, cursor_row: Option<usize>, gesture: impl FnOnce(&mut VisitState, &[Entry]) -> bool) -> Transition {
        let tab = self.active_tab(browser);
        let Some(TabState { history, folder_items: Some(items), .. }) = self.tab_mut(browser, tab) else { return Transition::default() };
        let Some(visit) = history.current_mut() else { return Transition::default() };
        if !gesture(visit.state_mut(), &items.entries) {
            return Transition::default();
        }
        Transition { outputs: vec![Output::SelectionChanged { browser, tab, selection: visit.state().selection().clone(), row: cursor_row.or_else(|| visit.state().cursor_row(&items.entries)) }], work: vec![] }
    }
    fn open_entry(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        self.tab(browser, self.active_tab(browser)).and_then(|t| t.folder_items.as_ref()).and_then(|f| f.entries.get(row).filter(|e| e.name() == name && e.can_enter()).map(|_| f.location.join(name))).map_or_else(Transition::default, |location| self.navigate(browser, location, None))
    }
    fn go_to_parent(&mut self, browser: BrowserSide) -> Transition {
        self.location(browser).and_then(Location::parent).map_or_else(Transition::default, |location| self.navigate(browser, location, None))
    }
    fn scroll(&mut self, browser: BrowserSide, scroll: Option<ScrollAnchor>) -> Transition {
        let tab = self.active_tab(browser);
        if let Some(TabState { history, folder_items: Some(items), .. }) = self.tab_mut(browser, tab)
            && let Some(visit) = history.current_mut()
        {
            visit.state_mut().set_scroll(&items.entries, scroll);
        }
        Transition::default()
    }
    fn loaded(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, entries: Arc<[Entry]>, changes: Option<Vec<RowChange>>) -> Transition {
        let Some(pending) = self.take_pending(browser, tab, token) else { return Transition::default() };
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        state.history.arrive(&pending.location, pending.history_target);
        let Some(visit) = state.history.current_mut() else { return Transition::default() };
        visit.state_mut().reconcile(&entries);
        // A same-folder reload keeps the worker's row change so the view keeps
        // its rows and scroll position. A change computed against anything but
        // the shown Folder Items, or one that does not fit them, replaces every
        // row instead, as does a new folder.
        let changes = match (state.folder_items.as_ref(), pending.base.as_ref(), changes) {
            (Some(old), Some(base), Some(changes)) if Arc::ptr_eq(&old.entries, base) && changes_fit(&changes, old.entries.len(), entries.len()) => Some(changes),
            _ => None,
        };
        state.folder_items = Some(FolderItems { location: pending.location.clone(), entries: Arc::clone(&entries) });
        state.error = None;
        let visit = visit.state();
        Transition { outputs: vec![Output::FolderItemsLoaded { browser, tab, location: pending.location, scroll_hint: visit.scroll().cloned(), changes, entries: Arc::clone(&entries) }, Output::SelectionChanged { browser, tab, selection: visit.selection().clone(), row: visit.cursor_row(&entries) }], work: vec![] }
    }
    fn failed(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken, kind: ListingErrorKind) -> Transition {
        let Some(pending) = self.take_pending(browser, tab, token) else { return Transition::default() };
        let error = ListingError::new(pending.location, kind);
        let Some(state) = self.tab_mut(browser, tab) else { return Transition::default() };
        state.error = Some(error.clone());
        Transition { outputs: vec![Output::FolderItemsFailed { browser, tab, error }], work: vec![] }
    }
    fn cancelled(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken) -> Transition {
        self.take_pending(browser, tab, token).map_or_else(Transition::default, |_| Transition { outputs: vec![Output::FolderItemsCancelled { browser, tab }], work: vec![] })
    }
    fn take_pending(&mut self, browser: BrowserSide, tab: TabId, token: RequestToken) -> Option<PendingRead> {
        self.tab_mut(browser, tab)?.pending.take_if(|pending| pending.token == token)
    }
    fn tab_view(&self, browser: BrowserSide, id: TabId) -> Output {
        let tab = self.tab(browser, id).expect("tab view needs a live tab");
        let entries = tab.folder_items.as_ref().map_or_else(|| Arc::from([]), |items| Arc::clone(&items.entries));
        let location = tab.folder_items.as_ref().map(|items| items.location.clone());
        let visit = tab.history.current().map(Visit::state);
        let selection = visit.map_or_else(Selection::default, |visit| visit.selection().clone());
        let row = visit.and_then(|visit| visit.cursor_row(&entries));
        Output::TabViewChanged { browser, tab: id, location, selection, row, scroll_hint: visit.and_then(|visit| visit.scroll().cloned()), entries, loading: tab.pending.is_some(), error: tab.error.clone() }
    }
}
impl BrowserState {
    fn new(id: TabId) -> Self {
        Self { order: BrowserTabs::new(id), tabs: vec![TabState::new(id)] }
    }
}
impl TabState {
    fn new(id: TabId) -> Self {
        Self { id, history: TabHistory::default(), folder_items: None, pending: None, error: None }
    }
}

fn normalize_group_positions(favorites: &mut FavoritesRecords) {
    for (position, group) in favorites.groups.iter_mut().enumerate() {
        group.position = position as i64;
    }
}

fn normalize_item_positions(favorites: &mut FavoritesRecords) {
    let groups = favorites.groups.iter().map(|group| group.id).collect::<Vec<_>>();
    for group_id in groups {
        for (position, item) in favorites.items.iter_mut().filter(|item| item.group_id == group_id).enumerate() {
            item.position = position as i64;
        }
    }
}

/// Whether `changes`, applied in order, turn `old_len` rows into `new_len`
/// rows without overlapping or leaving the list.
fn changes_fit(changes: &[RowChange], old_len: usize, new_len: usize) -> bool {
    let mut len = old_len;
    let mut next = 0;
    for change in changes {
        if change.row < next || change.row.checked_add(change.removed).is_none_or(|end| end > len) {
            return false;
        }
        len = (len - change.removed).saturating_add(change.inserted);
        next = change.row.saturating_add(change.inserted);
    }
    len == new_len
}
