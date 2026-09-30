use dual_pane_adapters::{PanePresenter, PaneViewModel};
use dual_pane_application::{Input, Output, Workspace};
use dual_pane_domain::Location;

use crate::runtime::{ListingSource, Runtime, WorkRunner};

/// How many delivered events one drain handles, so that input and painting
/// run between slices.
pub const DRAIN_SLICE: usize = 32;

/// What the pane needs to start: where it opens and how it reads listings.
pub struct PaneStartup {
    pub location: Location,
    pub source: ListingSource,
}

/// What the Qt model must notify after handling inputs, from least to most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViewChange {
    None,
    Status,
    Reset,
}

/// The outcome of one drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drained {
    pub change: ViewChange,
    /// Delivered events remain for another drain.
    pub more_pending: bool,
}

/// One pane's application state on the GUI thread: the workspace, its
/// presenter, and the runner that carries out its work.
pub struct PaneSession<R = Runtime> {
    workspace: Workspace,
    presenter: PanePresenter,
    runner: R,
    drain_slice: usize,
}

impl<R: WorkRunner> PaneSession<R> {
    pub fn new(runner: R, drain_slice: usize) -> Self {
        Self { workspace: Workspace::new(), presenter: PanePresenter::new(), runner, drain_slice }
    }

    /// Handles one input, dispatches the work it requests, and reports what
    /// the view must notify.
    pub fn submit(&mut self, input: impl Into<Input>) -> ViewChange {
        let transition = self.workspace.handle(input.into());
        let mut change = ViewChange::None;
        for output in &transition.outputs {
            self.presenter.apply(output);
            change = change.max(match output {
                Output::ListingReplaced { .. } => ViewChange::Reset,
                Output::LoadingStarted { .. } | Output::ListingFailed { .. } => ViewChange::Status,
            });
        }
        for request in transition.work {
            self.runner.dispatch(request);
        }
        change
    }

    /// Handles at most one slice of delivered events.
    pub fn drain(&mut self) -> Drained {
        let (events, more_pending) = self.runner.take_events(self.drain_slice);
        let change = events.into_iter().map(|event| self.submit(event)).max().unwrap_or(ViewChange::None);
        Drained { change, more_pending }
    }

    pub fn view(&self) -> &PaneViewModel {
        self.presenter.view()
    }
}

/// The status line for `view`: the error, loading, or the shown location.
pub fn status_text(view: &PaneViewModel) -> String {
    match view.error() {
        Some(error) => error.to_owned(),
        None if view.is_loading() => "Loading…".to_owned(),
        None => view.location_text().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use dual_pane_application::{Command, Event, WorkRequest};
    use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, RequestToken};

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        dispatched: Vec<WorkRequest>,
        delivered: VecDeque<Event>,
    }

    impl WorkRunner for FakeRunner {
        fn dispatch(&mut self, request: WorkRequest) {
            self.dispatched.push(request);
        }

        fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
            let count = max.min(self.delivered.len());
            (self.delivered.drain(..count).collect(), !self.delivered.is_empty())
        }
    }

    fn name(text: &str) -> EntryName {
        EntryName::new(text).unwrap()
    }

    fn location(text: &str) -> Location {
        Location::root().join(&name(text))
    }

    fn entries(count: usize) -> Arc<[Entry]> {
        (0..count).map(|index| Entry::new(name(&format!("file {index}")), EntryKind::File)).collect()
    }

    fn loaded(token: RequestToken, count: usize) -> Event {
        Event::ListingLoaded { token, entries: entries(count) }
    }

    fn session(drain_slice: usize) -> PaneSession<FakeRunner> {
        PaneSession::new(FakeRunner::default(), drain_slice)
    }

    #[test]
    fn the_initial_navigation_dispatches_one_read() {
        let mut session = session(DRAIN_SLICE);
        let change = session.submit(Command::Navigate(location("alpha")));
        assert_eq!(change, ViewChange::Status);
        assert_eq!(session.runner.dispatched, vec![WorkRequest::ReadDirectory { token: RequestToken::first(), location: location("alpha") }]);
        assert_eq!(status_text(session.view()), "Loading…");
    }

    #[test]
    fn a_delivered_result_updates_the_presenter_and_reports_a_reset() {
        let mut session = session(DRAIN_SLICE);
        session.submit(Command::Navigate(location("alpha")));
        session.runner.delivered.push_back(loaded(RequestToken::first(), 3));
        assert_eq!(session.drain(), Drained { change: ViewChange::Reset, more_pending: false });
        assert_eq!(session.view().row_count(), 3);
        assert_eq!(status_text(session.view()), "/alpha");
    }

    #[test]
    fn a_delivered_failure_reports_a_status_change() {
        let mut session = session(DRAIN_SLICE);
        session.submit(Command::Navigate(location("alpha")));
        session.runner.delivered.push_back(Event::ListingFailed { token: RequestToken::first(), kind: ListingErrorKind::PermissionDenied });
        assert_eq!(session.drain().change, ViewChange::Status);
        assert_eq!(status_text(session.view()), "You don’t have permission to open “/alpha”.");
    }

    #[test]
    fn a_stale_result_reports_nothing() {
        let mut session = session(DRAIN_SLICE);
        session.submit(Command::Navigate(location("alpha")));
        session.submit(Command::Navigate(location("beta")));
        session.runner.delivered.push_back(loaded(RequestToken::first(), 3));
        assert_eq!(session.drain(), Drained { change: ViewChange::None, more_pending: false });
        assert_eq!(session.view().row_count(), 0);
        assert!(session.view().is_loading());
    }

    #[test]
    fn one_drain_handles_at_most_one_slice() {
        let mut session = session(2);
        session.submit(Command::Navigate(location("alpha")));
        session.submit(Command::Navigate(location("beta")));
        let current = RequestToken::first().next();
        session.runner.delivered.extend([loaded(RequestToken::first(), 1), loaded(RequestToken::first(), 1), loaded(current, 2)]);
        assert_eq!(session.drain(), Drained { change: ViewChange::None, more_pending: true });
        assert_eq!(session.runner.delivered.len(), 1);
        assert_eq!(session.drain(), Drained { change: ViewChange::Reset, more_pending: false });
        assert_eq!(session.view().row_count(), 2);
    }
}
