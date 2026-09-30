use std::sync::Arc;

use dual_pane_domain::{Entry, EntryName, ListingError, ListingErrorKind, Location, RequestToken};

use crate::{Command, Event, Input, Output, WorkRequest};

/// The result of handling one input.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Transition {
    pub outputs: Vec<Output>,
    pub work: Vec<WorkRequest>,
}

/// The single owner of workspace state. It holds one pane; more panes and
/// tabs can be added without changing the meaning of existing inputs.
#[derive(Debug, Default)]
pub struct Workspace {
    pane: Pane,
    last_token: Option<RequestToken>,
}

#[derive(Debug, Default)]
struct Pane {
    listing: Option<Listing>,
    pending: Option<PendingNavigation>,
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

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, input: Input) -> Transition {
        match input {
            Input::Command(Command::Navigate(location)) => self.navigate(location),
            Input::Command(Command::OpenEntry(name)) => self.open_entry(&name),
            Input::Command(Command::GoToParent) => self.go_to_parent(),
            Input::Event(Event::ListingLoaded { token, entries }) => self.listing_loaded(token, entries),
            Input::Event(Event::ListingFailed { token, kind }) => self.listing_failed(token, kind),
            Input::Event(Event::ListingCancelled { token }) => self.listing_cancelled(token),
        }
    }

    /// The location of the listing the pane shows, if any.
    pub fn location(&self) -> Option<&Location> {
        self.pane.listing.as_ref().map(|listing| &listing.location)
    }

    /// The entries the pane shows, in listing order.
    pub fn entries(&self) -> &[Entry] {
        self.pane.listing.as_ref().map_or(&[], |listing| &listing.entries)
    }

    /// The location being loaded, if a navigation is pending.
    pub fn loading_location(&self) -> Option<&Location> {
        self.pane.pending.as_ref().map(|pending| &pending.location)
    }

    fn navigate(&mut self, location: Location) -> Transition {
        // Restarting a read that is already under way would only delay it.
        if self.loading_location() == Some(&location) {
            return Transition::default();
        }
        let token = self.last_token.map_or_else(RequestToken::first, RequestToken::next);
        self.last_token = Some(token);
        let mut work = Vec::new();
        if let Some(superseded) = self.pane.pending.replace(PendingNavigation { token, location: location.clone() }) {
            work.push(WorkRequest::Cancel { token: superseded.token });
        }
        work.push(WorkRequest::ReadDirectory { token, location: location.clone() });
        Transition { outputs: vec![Output::LoadingStarted { location }], work }
    }

    fn open_entry(&mut self, name: &EntryName) -> Transition {
        let Some(listing) = &self.pane.listing else {
            return Transition::default();
        };
        match listing.entries.iter().find(|entry| entry.name() == name) {
            Some(entry) if entry.can_enter() => {
                let target = listing.location.join(name);
                self.navigate(target)
            }
            _ => Transition::default(),
        }
    }

    fn go_to_parent(&mut self) -> Transition {
        match self.location().and_then(Location::parent) {
            Some(parent) => self.navigate(parent),
            None => Transition::default(),
        }
    }

    fn listing_loaded(&mut self, token: RequestToken, entries: Arc<[Entry]>) -> Transition {
        let Some(pending) = self.take_pending(token) else {
            return Transition::default();
        };
        let location = pending.location;
        self.pane.listing = Some(Listing { location: location.clone(), entries: Arc::clone(&entries) });
        Transition { outputs: vec![Output::ListingReplaced { location, entries }], work: Vec::new() }
    }

    fn listing_failed(&mut self, token: RequestToken, kind: ListingErrorKind) -> Transition {
        let Some(pending) = self.take_pending(token) else {
            return Transition::default();
        };
        Transition { outputs: vec![Output::ListingFailed { error: ListingError::new(pending.location, kind) }], work: Vec::new() }
    }

    fn listing_cancelled(&mut self, token: RequestToken) -> Transition {
        self.take_pending(token).map_or_else(Transition::default, |_| Transition { outputs: vec![Output::ListingCancelled], work: Vec::new() })
    }

    /// Ends the pending navigation if `token` identifies it.
    fn take_pending(&mut self, token: RequestToken) -> Option<PendingNavigation> {
        self.pane.pending.take_if(|pending| pending.token == token)
    }
}
