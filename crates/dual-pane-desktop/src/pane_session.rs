use std::collections::VecDeque;
use std::time::{Duration, Instant};

use dual_pane_adapters::{PanePresenter, PaneViewModel};
use dual_pane_application::{Input, Output, Workspace};
use dual_pane_domain::Location;

use crate::runtime::{ListingSourceFactory, Runtime, WorkRunner};

/// How many delivered events one drain handles, so that input and painting
/// run between slices.
pub const DRAIN_SLICE: usize = 32;

/// How long one drain may spend handling events before yielding to Qt.
pub const DRAIN_TIME_BUDGET: Duration = Duration::from_millis(4);

/// What the pane needs to start: where it opens and how it reads listings.
pub struct PaneStartup {
    pub location: Location,
    pub source_factory: ListingSourceFactory,
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
    drain_time_budget: Duration,
    buffered_events: VecDeque<dual_pane_application::Event>,
    runner_more_pending: bool,
}

impl<R: WorkRunner> PaneSession<R> {
    pub fn new(runner: R, drain_slice: usize, drain_time_budget: Duration) -> Self {
        assert!(drain_slice > 0, "a drain must be able to handle at least one event");
        Self { workspace: Workspace::new(), presenter: PanePresenter::new(), runner, drain_slice, drain_time_budget, buffered_events: VecDeque::new(), runner_more_pending: false }
    }

    /// Handles one input, dispatches the work it requests, and reports what
    /// the view must notify.
    pub fn submit(&mut self, input: impl Into<Input>) -> ViewChange {
        let mut inputs = VecDeque::from([input.into()]);
        let mut change = ViewChange::None;
        while let Some(input) = inputs.pop_front() {
            let transition = self.workspace.handle(input);
            for output in &transition.outputs {
                self.presenter.apply(output);
                change = change.max(match output {
                    Output::ListingReplaced { .. } => ViewChange::Reset,
                    Output::LoadingStarted { .. } | Output::ListingFailed { .. } | Output::ListingCancelled => ViewChange::Status,
                });
            }
            for request in transition.work {
                if let Some(event) = self.runner.dispatch(request) {
                    inputs.push_back(event.into());
                }
            }
        }
        change
    }

    /// Handles delivered events within both the count and elapsed-time limits.
    pub fn drain(&mut self) -> Drained {
        let started = Instant::now();
        if self.buffered_events.is_empty() {
            let (events, more_pending) = self.runner.take_events(self.drain_slice);
            self.buffered_events.extend(events);
            self.runner_more_pending = more_pending;
        }

        let mut change = ViewChange::None;
        let mut handled = 0;
        while handled < self.drain_slice && !self.buffered_events.is_empty() {
            if handled > 0 && started.elapsed() >= self.drain_time_budget {
                break;
            }
            if let Some(event) = self.buffered_events.pop_front() {
                change = change.max(self.submit(event));
                handled += 1;
            }
        }

        // A worker wake may arrive while an earlier batch is buffered. Poll
        // again when that batch empties so the wake cannot be consumed by a
        // turn that never reaches the runtime queue.
        if self.buffered_events.is_empty() {
            let (events, more_pending) = self.runner.take_events(self.drain_slice);
            self.buffered_events.extend(events);
            self.runner_more_pending = more_pending;
        }
        Drained { change, more_pending: !self.buffered_events.is_empty() || self.runner_more_pending }
    }

    pub fn view(&self) -> &PaneViewModel {
        self.presenter.view()
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
        immediate: VecDeque<Event>,
    }

    impl WorkRunner for FakeRunner {
        fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
            self.dispatched.push(request);
            self.immediate.pop_front()
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
        PaneSession::new(FakeRunner::default(), drain_slice, Duration::from_secs(60))
    }

    #[test]
    fn the_initial_navigation_dispatches_one_read() {
        let mut session = session(DRAIN_SLICE);
        let change = session.submit(Command::Navigate(location("alpha")));
        assert_eq!(change, ViewChange::Status);
        assert_eq!(session.runner.dispatched, vec![WorkRequest::ReadDirectory { token: RequestToken::first(), location: location("alpha") }]);
        assert_eq!(session.view().status_text(), "Loading…");
    }

    #[test]
    fn a_delivered_result_updates_the_presenter_and_reports_a_reset() {
        let mut session = session(DRAIN_SLICE);
        session.submit(Command::Navigate(location("alpha")));
        session.runner.delivered.push_back(loaded(RequestToken::first(), 3));
        assert_eq!(session.drain(), Drained { change: ViewChange::Reset, more_pending: false });
        assert_eq!(session.view().row_count(), 3);
        assert_eq!(session.view().status_text(), "/alpha");
    }

    #[test]
    fn a_delivered_failure_reports_a_status_change() {
        let mut session = session(DRAIN_SLICE);
        session.submit(Command::Navigate(location("alpha")));
        session.runner.delivered.push_back(Event::ListingFailed { token: RequestToken::first(), kind: ListingErrorKind::PermissionDenied });
        assert_eq!(session.drain().change, ViewChange::Status);
        assert_eq!(session.view().status_text(), "You don’t have permission to open “/alpha”.");
    }

    #[test]
    fn an_immediate_dispatch_failure_is_handled_without_a_wake() {
        let mut runner = FakeRunner::default();
        runner.immediate.push_back(Event::ListingFailed { token: RequestToken::first(), kind: ListingErrorKind::Internal });
        let mut session = PaneSession::new(runner, DRAIN_SLICE, DRAIN_TIME_BUDGET);

        assert_eq!(session.submit(Command::Navigate(location("alpha"))), ViewChange::Status);
        assert_eq!(session.view().status_text(), "Dual Pane couldn’t finish reading this folder unexpectedly.");
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
        assert!(session.view().is_loading());
        assert_eq!(session.drain(), Drained { change: ViewChange::Reset, more_pending: false });
        assert_eq!(session.view().row_count(), 2);
    }

    #[test]
    fn one_drain_stops_after_its_elapsed_time_budget() {
        let mut session = PaneSession::new(FakeRunner::default(), 3, Duration::ZERO);
        session.submit(Command::Navigate(location("alpha")));
        session.submit(Command::Navigate(location("beta")));
        let current = RequestToken::first().next();
        session.runner.delivered.extend([loaded(RequestToken::first(), 1), loaded(RequestToken::first(), 1), loaded(current, 2)]);

        assert_eq!(session.drain(), Drained { change: ViewChange::None, more_pending: true });
        assert!(session.view().is_loading());
        assert_eq!(session.drain(), Drained { change: ViewChange::None, more_pending: true });
        assert_eq!(session.drain(), Drained { change: ViewChange::Reset, more_pending: false });
        assert_eq!(session.view().row_count(), 2);
    }

    #[test]
    fn a_new_event_is_not_stranded_behind_a_time_limited_batch() {
        let mut session = PaneSession::new(FakeRunner::default(), 3, Duration::ZERO);
        session.submit(Command::Navigate(location("alpha")));
        session.submit(Command::Navigate(location("beta")));
        let current = RequestToken::first().next();
        session.runner.delivered.extend([loaded(RequestToken::first(), 1), loaded(RequestToken::first(), 1)]);

        assert_eq!(session.drain(), Drained { change: ViewChange::None, more_pending: true });
        session.runner.delivered.push_back(loaded(current, 2));
        assert_eq!(session.drain(), Drained { change: ViewChange::None, more_pending: true });
        assert_eq!(session.drain(), Drained { change: ViewChange::Reset, more_pending: false });
        assert_eq!(session.view().row_count(), 2);
    }
}
