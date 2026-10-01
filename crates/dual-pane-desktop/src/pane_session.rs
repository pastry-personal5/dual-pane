use std::collections::VecDeque;
use std::time::{Duration, Instant};

use dual_pane_adapters::{InputController, PanePresenter, PaneViewModel, UiEvent};
use dual_pane_application::{Command, Input, Workspace};
use dual_pane_domain::{Location, PaneSide};

use crate::runtime::{ListingSourceFactory, Runtime, WorkRunner};

/// How many delivered events one drain handles, so input and painting run
/// between slices.
pub const DRAIN_SLICE: usize = 32;

/// How long one drain may spend handling events before yielding to Qt.
pub const DRAIN_TIME_BUDGET: Duration = Duration::from_millis(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionMovement {
    Previous,
    Next,
}

/// What the desktop needs to start: where both Browsers open and how they read
/// listings.
pub struct PaneStartup {
    pub location: Location,
    pub source_factory: ListingSourceFactory,
}

/// GUI-thread coordinator for the Standard Layout. It owns one workspace and
/// one runtime while keeping the two presentation models independent.
pub struct WorkspaceSession<R = Runtime> {
    workspace: Workspace,
    controller: InputController,
    left: PanePresenter,
    right: PanePresenter,
    runner: R,
    drain_slice: usize,
    drain_time_budget: Duration,
    buffered_events: VecDeque<dual_pane_application::Event>,
    runner_more_pending: bool,
}

impl<R: WorkRunner> WorkspaceSession<R> {
    pub fn new(runner: R, drain_slice: usize, drain_time_budget: Duration) -> Self {
        assert!(drain_slice > 0, "a drain must be able to handle at least one event");
        Self { workspace: Workspace::new(), controller: InputController::new(), left: PanePresenter::new(PaneSide::Left), right: PanePresenter::new(PaneSide::Right), runner, drain_slice, drain_time_budget, buffered_events: VecDeque::new(), runner_more_pending: false }
    }

    pub fn start(&mut self, location: Location) {
        self.submit(Command::Navigate { pane: PaneSide::Left, location: location.clone() });
        self.submit(Command::Navigate { pane: PaneSide::Right, location });
    }

    pub fn submit(&mut self, input: impl Into<Input>) {
        let mut inputs = VecDeque::from([input.into()]);
        while let Some(input) = inputs.pop_front() {
            let transition = self.workspace.handle(input);
            for output in &transition.outputs {
                self.left.apply(output);
                self.right.apply(output);
            }
            for request in transition.work {
                if let Some(event) = self.runner.dispatch(request) {
                    inputs.push_back(event.into());
                }
            }
        }
    }

    pub fn submit_ui(&mut self, pane: PaneSide, event: UiEvent) {
        self.submit(Command::ActivatePane { pane });
        let view = self.view(pane).clone();
        if let Some(command) = self.controller.command(pane, event, &view) {
            self.submit(command);
        }
    }

    pub fn activate(&mut self, pane: PaneSide) {
        self.submit(Command::ActivatePane { pane });
    }

    pub fn clear_selection(&mut self, pane: PaneSide) {
        self.submit(Command::ActivatePane { pane });
        self.submit(Command::ClearSelection { pane });
    }

    pub(crate) fn move_selection(&mut self, pane: PaneSide, movement: SelectionMovement) {
        let view = self.view(pane);
        let row_count = view.row_count();
        if row_count == 0 {
            return;
        }
        let selected = view.selected_row();
        let target = match (selected, movement) {
            (None, _) => 0,
            (Some(0), SelectionMovement::Previous) => 0,
            (Some(row), SelectionMovement::Previous) => row - 1,
            (Some(row), SelectionMovement::Next) => row.saturating_add(1).min(row_count - 1),
        };
        if selected != Some(target) {
            self.submit_ui(pane, UiEvent::SelectRow { row: target });
        }
    }

    pub(crate) fn activate_selection(&mut self, pane: PaneSide) {
        if let Some(row) = self.view(pane).selected_row() {
            self.submit_ui(pane, UiEvent::ActivateRow { row });
        }
    }

    pub fn drain(&mut self) -> bool {
        let started = Instant::now();
        if self.buffered_events.is_empty() {
            let (events, more) = self.runner.take_events(self.drain_slice);
            self.buffered_events.extend(events);
            self.runner_more_pending = more;
        }
        let mut handled = 0;
        while handled < self.drain_slice && !self.buffered_events.is_empty() {
            if handled > 0 && started.elapsed() >= self.drain_time_budget {
                break;
            }
            if let Some(event) = self.buffered_events.pop_front() {
                self.submit(event);
                handled += 1;
            }
        }
        if self.buffered_events.is_empty() {
            let (events, more) = self.runner.take_events(self.drain_slice);
            self.buffered_events.extend(events);
            self.runner_more_pending = more;
        }
        !self.buffered_events.is_empty() || self.runner_more_pending
    }

    pub fn view(&self, pane: PaneSide) -> &PaneViewModel {
        match pane {
            PaneSide::Left => self.left.view(),
            PaneSide::Right => self.right.view(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dual_pane_application::{Command, Event, WorkRequest};
    use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, RequestToken, Selection};

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        dispatched: Vec<WorkRequest>,
        delivered: VecDeque<Event>,
    }

    impl WorkRunner for FakeRunner {
        fn dispatch(&mut self, request: WorkRequest) -> Option<Event> {
            self.dispatched.push(request);
            None
        }

        fn take_events(&mut self, max: usize) -> (Vec<Event>, bool) {
            let count = max.min(self.delivered.len());
            (self.delivered.drain(..count).collect(), !self.delivered.is_empty())
        }
    }

    fn entry(name: &str) -> Entry {
        Entry::new(EntryName::new(name).unwrap(), EntryKind::Directory)
    }

    fn path(name: &str) -> Location {
        Location::root().join(&EntryName::new(name).unwrap())
    }

    fn assert_view(session: &WorkspaceSession<FakeRunner>, pane: PaneSide, expected: (&str, &[&str], Option<&str>, &str, bool, u64)) {
        let (path, rows, selected, status, loading, revision) = expected;
        let view = session.view(pane);
        assert_eq!(view.location_text(), path);
        assert_eq!(view.row_count(), rows.len());
        for (index, name) in rows.iter().enumerate() {
            assert_eq!(view.row(index).map(|row| row.name), Some((*name).to_owned()));
        }
        let expected_row = selected.and_then(|name| rows.iter().position(|row| *row == name));
        assert_eq!(view.selected_row(), expected_row);
        let mut selection = Selection::default();
        if let Some(name) = selected {
            selection.select(EntryName::new(name).unwrap());
        }
        assert_eq!(view.selection(), &selection);
        assert_eq!(view.status_text(), status);
        assert_eq!(view.is_loading(), loading);
        assert_eq!(view.listing_revision(), revision);
    }

    #[test]
    fn loading_failure_cancellation_and_late_results_preserve_the_other_browser() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(path("base"));
        assert_view(&session, PaneSide::Left, ("", &[], None, "Loading…", true, 0));
        assert_view(&session, PaneSide::Right, ("", &[], None, "Loading…", true, 0));
        session.submit(Event::ListingLoaded { pane: PaneSide::Left, token: RequestToken::first(), entries: Arc::from(vec![entry("left")]) });
        session.submit(Event::ListingLoaded { pane: PaneSide::Right, token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]) });
        session.submit_ui(PaneSide::Left, UiEvent::SelectRow { row: 0 });
        session.submit_ui(PaneSide::Right, UiEvent::SelectRow { row: 0 });
        assert_view(&session, PaneSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

        session.submit(Command::Navigate { pane: PaneSide::Left, location: path("older") });
        let older = RequestToken::first().next().next();
        session.submit(Command::Navigate { pane: PaneSide::Left, location: path("missing") });
        let missing = older.next();
        assert_eq!(session.runner.dispatched[3], WorkRequest::Cancel { pane: PaneSide::Left, token: older });
        session.submit(Event::ListingCancelled { pane: PaneSide::Left, token: older });
        session.submit(Event::ListingLoaded { pane: PaneSide::Left, token: older, entries: Arc::from(vec![entry("late")]) });
        session.submit(Event::ListingLoaded { pane: PaneSide::Right, token: missing, entries: Arc::from(vec![entry("wrong")]) });
        assert_view(&session, PaneSide::Left, ("/base", &["left"], Some("left"), "Loading…", true, 1));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

        session.submit(Event::ListingFailed { pane: PaneSide::Left, token: missing, kind: ListingErrorKind::ItemMissing });
        assert_view(&session, PaneSide::Left, ("/base", &["left"], Some("left"), "“/missing” no longer exists.", false, 1));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
        session.submit(Command::Navigate { pane: PaneSide::Left, location: path("cancelled") });
        let cancelled = missing.next();
        session.submit(Event::ListingCancelled { pane: PaneSide::Left, token: cancelled });
        assert_view(&session, PaneSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

        session.submit(Command::Navigate { pane: PaneSide::Left, location: path("final") });
        session.submit(Event::ListingLoaded { pane: PaneSide::Left, token: cancelled.next(), entries: Arc::from(vec![entry("fresh")]) });
        assert_view(&session, PaneSide::Left, ("/final", &["fresh"], None, "/final", false, 2));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
    }

    #[test]
    fn bounded_drain_applies_one_panes_result_without_touching_the_other() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), 1, DRAIN_TIME_BUDGET);
        session.start(path("base"));
        session.runner.delivered.push_back(Event::ListingLoaded { pane: PaneSide::Right, token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]) });
        session.runner.delivered.push_back(Event::ListingLoaded { pane: PaneSide::Left, token: RequestToken::first(), entries: Arc::from(vec![entry("left")]) });
        assert!(session.drain());
        assert_view(&session, PaneSide::Left, ("", &[], None, "Loading…", true, 0));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], None, "/base", false, 1));
        assert!(!session.drain());
        assert_view(&session, PaneSide::Left, ("/base", &["left"], None, "/base", false, 1));
        assert_view(&session, PaneSide::Right, ("/base", &["right"], None, "/base", false, 1));
    }

    #[test]
    fn start_dispatches_distinct_reads_for_both_browsers() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());
        assert_eq!(session.runner.dispatched, vec![WorkRequest::ReadDirectory { pane: PaneSide::Left, token: RequestToken::first(), location: Location::root() }, WorkRequest::ReadDirectory { pane: PaneSide::Right, token: RequestToken::first().next(), location: Location::root() }]);
    }

    #[test]
    fn listing_revisions_and_selection_are_pane_local() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());
        session.submit(Event::ListingLoaded { pane: PaneSide::Left, token: RequestToken::first(), entries: Arc::from(vec![entry("left")]) });
        assert_eq!(session.view(PaneSide::Left).listing_revision(), 1);
        assert_eq!(session.view(PaneSide::Right).listing_revision(), 0);

        session.submit(Event::ListingLoaded { pane: PaneSide::Right, token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]) });
        session.submit_ui(PaneSide::Right, UiEvent::SelectRow { row: 0 });
        assert_eq!(session.view(PaneSide::Left).listing_revision(), 1);
        assert_eq!(session.view(PaneSide::Right).listing_revision(), 1);
        assert_eq!(session.view(PaneSide::Left).selected_row(), None);
        assert_eq!(session.view(PaneSide::Right).selected_row(), Some(0));
    }

    #[test]
    fn vertical_arrow_movement_obeys_empty_unselected_and_boundary_cases() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());

        session.move_selection(PaneSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(PaneSide::Left).selected_row(), None);

        session.submit(Event::ListingLoaded { pane: PaneSide::Left, token: RequestToken::first(), entries: Arc::from(vec![entry("first"), entry("second"), entry("third")]) });
        session.move_selection(PaneSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(PaneSide::Left).selected_row(), Some(0));
        session.move_selection(PaneSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(PaneSide::Left).selected_row(), Some(0));
        session.move_selection(PaneSide::Left, SelectionMovement::Next);
        session.move_selection(PaneSide::Left, SelectionMovement::Next);
        session.move_selection(PaneSide::Left, SelectionMovement::Next);
        assert_eq!(session.view(PaneSide::Left).selected_row(), Some(2));
        assert_eq!(session.view(PaneSide::Right).selected_row(), None);
    }

    #[test]
    fn horizontal_arrow_actions_navigate_the_addressed_pane() {
        let folder = EntryName::new("folder").unwrap();
        let location = Location::root().join(&EntryName::new("parent").unwrap());
        let mut session = WorkspaceSession::new(FakeRunner::default(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(location.clone());
        session.submit(Event::ListingLoaded { pane: PaneSide::Left, token: RequestToken::first(), entries: Arc::from(vec![entry("folder")]) });
        session.submit(Event::ListingLoaded { pane: PaneSide::Right, token: RequestToken::first().next(), entries: Arc::from(vec![entry("folder")]) });
        session.runner.dispatched.clear();

        session.submit_ui(PaneSide::Left, UiEvent::GoToParent);
        session.submit_ui(PaneSide::Right, UiEvent::SelectRow { row: 0 });
        session.activate_selection(PaneSide::Right);

        assert_eq!(session.runner.dispatched, vec![WorkRequest::ReadDirectory { pane: PaneSide::Left, token: RequestToken::first().next().next(), location: Location::root() }, WorkRequest::ReadDirectory { pane: PaneSide::Right, token: RequestToken::first().next().next().next(), location: location.join(&folder) }]);
    }
}
