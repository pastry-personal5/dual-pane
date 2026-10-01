use std::sync::Arc;

use dual_pane_domain::{BrowserSide, Entry, EntryName, ListingError, ListingErrorKind, Location, RequestToken, Selection};

use crate::{Command, Event, Input, Output, WorkRequest};

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
    last_token: Option<RequestToken>,
}

#[derive(Debug, Default)]
struct BrowserState {
    folder_items: Option<FolderItems>,
    pending: Option<PendingNavigation>,
    selection: Selection,
}

#[derive(Debug)]
struct FolderItems {
    location: Location,
    entries: Arc<[Entry]>,
}

#[derive(Debug)]
struct PendingNavigation {
    token: RequestToken,
    location: Location,
}

impl Default for Workspace {
    fn default() -> Self {
        Self { left: BrowserState::default(), right: BrowserState::default(), active_browser: BrowserSide::Left, last_token: None }
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, input: Input) -> Transition {
        match input {
            Input::Command(Command::ActivateBrowser { browser }) => self.activate_browser(browser),
            Input::Command(Command::Navigate { browser, location }) => self.navigate(browser, location),
            Input::Command(Command::SelectEntry { browser, row, name }) => self.select_entry(browser, row, &name),
            Input::Command(Command::ClearSelection { browser }) => self.clear_selection(browser),
            Input::Command(Command::OpenEntry { browser, row, name }) => self.open_entry(browser, row, &name),
            Input::Command(Command::GoToParent { browser }) => self.go_to_parent(browser),
            Input::Event(Event::FolderItemsLoaded { browser, token, entries }) => self.folder_items_loaded(browser, token, entries),
            Input::Event(Event::FolderItemsFailed { browser, token, kind }) => self.folder_items_failed(browser, token, kind),
            Input::Event(Event::FolderItemsCancelled { browser, token }) => self.folder_items_cancelled(browser, token),
        }
    }

    pub fn active_browser(&self) -> BrowserSide {
        self.active_browser
    }

    pub fn location(&self, side: BrowserSide) -> Option<&Location> {
        self.browser(side).folder_items.as_ref().map(|folder_items| &folder_items.location)
    }

    pub fn entries(&self, side: BrowserSide) -> &[Entry] {
        self.browser(side).folder_items.as_ref().map_or(&[], |folder_items| &folder_items.entries)
    }

    pub fn loading_location(&self, side: BrowserSide) -> Option<&Location> {
        self.browser(side).pending.as_ref().map(|pending| &pending.location)
    }

    pub fn selection(&self, side: BrowserSide) -> &Selection {
        &self.browser(side).selection
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

    fn activate_browser(&mut self, browser: BrowserSide) -> Transition {
        if self.active_browser == browser {
            Transition::default()
        } else {
            self.active_browser = browser;
            Transition { outputs: vec![Output::ActiveBrowserChanged { browser }], work: Vec::new() }
        }
    }

    fn navigate(&mut self, browser: BrowserSide, location: Location) -> Transition {
        if self.loading_location(browser) == Some(&location) {
            return Transition::default();
        }
        let token = self.last_token.map_or_else(RequestToken::first, RequestToken::next);
        self.last_token = Some(token);
        let superseded = self.browser_mut(browser).pending.replace(PendingNavigation { token, location: location.clone() });
        let mut work = superseded.into_iter().map(|pending| WorkRequest::Cancel { browser, token: pending.token }).collect::<Vec<_>>();
        work.push(WorkRequest::ReadDirectory { browser, token, location: location.clone() });
        Transition { outputs: vec![Output::LoadingStarted { browser, location }], work }
    }

    fn select_entry(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        let selected = self.browser(browser).folder_items.as_ref().and_then(|folder_items| folder_items.entries.get(row)).filter(|entry| entry.name() == name).map(|entry| entry.name().clone());
        let Some(selected) = selected else {
            return Transition::default();
        };
        let state = self.browser_mut(browser);
        if !state.selection.select(selected) {
            return Transition::default();
        }
        Transition { outputs: vec![Output::SelectionChanged { browser, selection: state.selection.clone(), row: Some(row) }], work: Vec::new() }
    }

    fn clear_selection(&mut self, browser: BrowserSide) -> Transition {
        let state = self.browser_mut(browser);
        if !state.selection.clear() {
            return Transition::default();
        }
        Transition { outputs: vec![Output::SelectionChanged { browser, selection: state.selection.clone(), row: None }], work: Vec::new() }
    }

    fn open_entry(&mut self, browser: BrowserSide, row: usize, name: &EntryName) -> Transition {
        let target = self.browser(browser).folder_items.as_ref().and_then(|folder_items| folder_items.entries.get(row).filter(|entry| entry.name() == name && entry.can_enter()).map(|_| folder_items.location.join(name)));
        target.map_or_else(Transition::default, |location| self.navigate(browser, location))
    }

    fn go_to_parent(&mut self, browser: BrowserSide) -> Transition {
        self.location(browser).and_then(Location::parent).map_or_else(Transition::default, |location| self.navigate(browser, location))
    }

    fn folder_items_loaded(&mut self, browser: BrowserSide, token: RequestToken, entries: Arc<[Entry]>) -> Transition {
        let Some(pending) = self.take_pending(browser, token) else {
            return Transition::default();
        };
        let state = self.browser_mut(browser);
        state.folder_items = Some(FolderItems { location: pending.location.clone(), entries: Arc::clone(&entries) });
        let mut outputs = vec![Output::FolderItemsReplaced { browser, location: pending.location, entries }];
        if state.selection.clear() {
            outputs.push(Output::SelectionChanged { browser, selection: state.selection.clone(), row: None });
        }
        Transition { outputs, work: Vec::new() }
    }

    fn folder_items_failed(&mut self, browser: BrowserSide, token: RequestToken, kind: ListingErrorKind) -> Transition {
        self.take_pending(browser, token).map_or_else(Transition::default, |pending| Transition { outputs: vec![Output::FolderItemsFailed { browser, error: ListingError::new(pending.location, kind) }], work: Vec::new() })
    }

    fn folder_items_cancelled(&mut self, browser: BrowserSide, token: RequestToken) -> Transition {
        self.take_pending(browser, token).map_or_else(Transition::default, |_| Transition { outputs: vec![Output::FolderItemsCancelled { browser }], work: Vec::new() })
    }

    fn take_pending(&mut self, browser: BrowserSide, token: RequestToken) -> Option<PendingNavigation> {
        self.browser_mut(browser).pending.take_if(|pending| pending.token == token)
    }
}
