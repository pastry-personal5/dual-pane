use std::sync::Arc;

use dual_pane_domain::{Entry, EntryName, ListingError, ListingErrorKind, Location, PaneSide, RequestToken, Selection};

use crate::{Command, Event, Input, Output, WorkRequest};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Transition {
    pub outputs: Vec<Output>,
    pub work: Vec<WorkRequest>,
}

#[derive(Debug)]
pub struct Workspace {
    left: Pane,
    right: Pane,
    active_pane: PaneSide,
    last_token: Option<RequestToken>,
}

#[derive(Debug, Default)]
struct Pane {
    listing: Option<Listing>,
    pending: Option<PendingNavigation>,
    selection: Selection,
}

#[derive(Debug)]
struct Listing {
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
        Self { left: Pane::default(), right: Pane::default(), active_pane: PaneSide::Left, last_token: None }
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, input: Input) -> Transition {
        match input {
            Input::Command(Command::ActivatePane { pane }) => self.activate_pane(pane),
            Input::Command(Command::Navigate { pane, location }) => self.navigate(pane, location),
            Input::Command(Command::SelectEntry { pane, row, name }) => self.select_entry(pane, row, &name),
            Input::Command(Command::ClearSelection { pane }) => self.clear_selection(pane),
            Input::Command(Command::OpenEntry { pane, row, name }) => self.open_entry(pane, row, &name),
            Input::Command(Command::GoToParent { pane }) => self.go_to_parent(pane),
            Input::Event(Event::ListingLoaded { pane, token, entries }) => self.listing_loaded(pane, token, entries),
            Input::Event(Event::ListingFailed { pane, token, kind }) => self.listing_failed(pane, token, kind),
            Input::Event(Event::ListingCancelled { pane, token }) => self.listing_cancelled(pane, token),
        }
    }

    pub fn active_pane(&self) -> PaneSide {
        self.active_pane
    }

    pub fn location(&self, side: PaneSide) -> Option<&Location> {
        self.pane(side).listing.as_ref().map(|listing| &listing.location)
    }

    pub fn entries(&self, side: PaneSide) -> &[Entry] {
        self.pane(side).listing.as_ref().map_or(&[], |listing| &listing.entries)
    }

    pub fn loading_location(&self, side: PaneSide) -> Option<&Location> {
        self.pane(side).pending.as_ref().map(|pending| &pending.location)
    }

    pub fn selection(&self, side: PaneSide) -> &Selection {
        &self.pane(side).selection
    }

    fn pane(&self, side: PaneSide) -> &Pane {
        match side {
            PaneSide::Left => &self.left,
            PaneSide::Right => &self.right,
        }
    }

    fn pane_mut(&mut self, side: PaneSide) -> &mut Pane {
        match side {
            PaneSide::Left => &mut self.left,
            PaneSide::Right => &mut self.right,
        }
    }

    fn activate_pane(&mut self, pane: PaneSide) -> Transition {
        if self.active_pane == pane {
            Transition::default()
        } else {
            self.active_pane = pane;
            Transition { outputs: vec![Output::ActivePaneChanged { pane }], work: Vec::new() }
        }
    }

    fn navigate(&mut self, pane: PaneSide, location: Location) -> Transition {
        if self.loading_location(pane) == Some(&location) {
            return Transition::default();
        }
        let token = self.last_token.map_or_else(RequestToken::first, RequestToken::next);
        self.last_token = Some(token);
        let superseded = self.pane_mut(pane).pending.replace(PendingNavigation { token, location: location.clone() });
        let mut work = superseded.into_iter().map(|pending| WorkRequest::Cancel { pane, token: pending.token }).collect::<Vec<_>>();
        work.push(WorkRequest::ReadDirectory { pane, token, location: location.clone() });
        Transition { outputs: vec![Output::LoadingStarted { pane, location }], work }
    }

    fn select_entry(&mut self, pane: PaneSide, row: usize, name: &EntryName) -> Transition {
        let selected = self.pane(pane).listing.as_ref().and_then(|listing| listing.entries.get(row)).filter(|entry| entry.name() == name).map(|entry| entry.name().clone());
        let Some(selected) = selected else {
            return Transition::default();
        };
        let state = self.pane_mut(pane);
        if !state.selection.select(selected) {
            return Transition::default();
        }
        Transition { outputs: vec![Output::SelectionChanged { pane, selection: state.selection.clone(), row: Some(row) }], work: Vec::new() }
    }

    fn clear_selection(&mut self, pane: PaneSide) -> Transition {
        let state = self.pane_mut(pane);
        if !state.selection.clear() {
            return Transition::default();
        }
        Transition { outputs: vec![Output::SelectionChanged { pane, selection: state.selection.clone(), row: None }], work: Vec::new() }
    }

    fn open_entry(&mut self, pane: PaneSide, row: usize, name: &EntryName) -> Transition {
        let target = self.pane(pane).listing.as_ref().and_then(|listing| listing.entries.get(row).filter(|entry| entry.name() == name && entry.can_enter()).map(|_| listing.location.join(name)));
        target.map_or_else(Transition::default, |location| self.navigate(pane, location))
    }

    fn go_to_parent(&mut self, pane: PaneSide) -> Transition {
        self.location(pane).and_then(Location::parent).map_or_else(Transition::default, |location| self.navigate(pane, location))
    }

    fn listing_loaded(&mut self, pane: PaneSide, token: RequestToken, entries: Arc<[Entry]>) -> Transition {
        let Some(pending) = self.take_pending(pane, token) else {
            return Transition::default();
        };
        let state = self.pane_mut(pane);
        state.listing = Some(Listing { location: pending.location.clone(), entries: Arc::clone(&entries) });
        let mut outputs = vec![Output::ListingReplaced { pane, location: pending.location, entries }];
        if state.selection.clear() {
            outputs.push(Output::SelectionChanged { pane, selection: state.selection.clone(), row: None });
        }
        Transition { outputs, work: Vec::new() }
    }

    fn listing_failed(&mut self, pane: PaneSide, token: RequestToken, kind: ListingErrorKind) -> Transition {
        self.take_pending(pane, token).map_or_else(Transition::default, |pending| Transition { outputs: vec![Output::ListingFailed { pane, error: ListingError::new(pending.location, kind) }], work: Vec::new() })
    }

    fn listing_cancelled(&mut self, pane: PaneSide, token: RequestToken) -> Transition {
        self.take_pending(pane, token).map_or_else(Transition::default, |_| Transition { outputs: vec![Output::ListingCancelled { pane }], work: Vec::new() })
    }

    fn take_pending(&mut self, pane: PaneSide, token: RequestToken) -> Option<PendingNavigation> {
        self.pane_mut(pane).pending.take_if(|pending| pending.token == token)
    }
}
