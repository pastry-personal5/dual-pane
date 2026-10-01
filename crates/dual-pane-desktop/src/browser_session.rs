use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dual_pane_adapters::{BrowserPresenter, BrowserViewModel, InputController, UiEvent};
use dual_pane_application::{Command, Input, Workspace};
use dual_pane_domain::{BrowserSide, Location};

use crate::runtime::{FolderItemsSourceFactory, Runtime, WorkRunner};
use crate::settings_storage::{SettingsJob, SettingsResult, SettingsWorker};

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
pub struct BrowserStartup {
    pub location: Location,
    pub home: Location,
    pub screenshots_exists: bool,
    pub settings_path: PathBuf,
    pub source_factory: FolderItemsSourceFactory,
}

/// GUI-thread coordinator for the Standard Layout. It owns one workspace and
/// one runtime while keeping the two presentation models independent.
pub struct WorkspaceSession<R = Runtime> {
    workspace: Workspace,
    controller: InputController,
    left: BrowserPresenter,
    right: BrowserPresenter,
    runner: R,
    settings_worker: Option<SettingsWorker>,
    drain_slice: usize,
    drain_time_budget: Duration,
    buffered_events: VecDeque<dual_pane_application::Event>,
    runner_more_pending: bool,
}

impl<R: WorkRunner> WorkspaceSession<R> {
    #[cfg(test)]
    pub fn new(runner: R, home: Location, drain_slice: usize, drain_time_budget: Duration) -> Self {
        Self::with_settings(runner, home, false, None, drain_slice, drain_time_budget)
    }

    pub fn with_settings(runner: R, home: Location, screenshots_exists: bool, settings_worker: Option<SettingsWorker>, drain_slice: usize, drain_time_budget: Duration) -> Self {
        assert!(drain_slice > 0, "a drain must be able to handle at least one event");
        if let Some(worker) = &settings_worker {
            worker.submit(SettingsJob::Load);
        }
        Self { workspace: Workspace::with_context(home, screenshots_exists), controller: InputController::new(), left: BrowserPresenter::new(BrowserSide::Left), right: BrowserPresenter::new(BrowserSide::Right), runner, settings_worker, drain_slice, drain_time_budget, buffered_events: VecDeque::new(), runner_more_pending: false }
    }

    pub fn start(&mut self, location: Location) {
        self.submit(Command::Navigate { browser: BrowserSide::Left, location: location.clone() });
        self.submit(Command::Navigate { browser: BrowserSide::Right, location });
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
                if let dual_pane_application::WorkRequest::SaveSettings { revision, snapshot } = request {
                    if let Some(worker) = &self.settings_worker {
                        if !worker.submit(SettingsJob::Save { revision, snapshot }) {
                            inputs.push_back(dual_pane_application::Event::SettingsSaveFailed { revision }.into());
                        }
                    } else {
                        inputs.push_back(dual_pane_application::Event::SettingsSaveFailed { revision }.into());
                    }
                } else if let Some(event) = self.runner.dispatch(request) {
                    inputs.push_back(event.into());
                }
            }
        }
    }

    pub fn submit_ui(&mut self, browser: BrowserSide, event: UiEvent) {
        self.submit(Command::ActivateBrowser { browser });
        let view = self.view(browser).clone();
        if let Some(command) = self.controller.command(browser, event, &view) {
            self.submit(command);
        }
    }

    pub fn activate(&mut self, browser: BrowserSide) {
        self.submit(Command::ActivateBrowser { browser });
    }

    pub fn clear_selection(&mut self, browser: BrowserSide) {
        self.submit(Command::ActivateBrowser { browser });
        self.submit(Command::ClearSelection { browser });
    }

    pub(crate) fn move_selection(&mut self, browser: BrowserSide, movement: SelectionMovement) {
        let view = self.view(browser);
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
        self.submit_ui(browser, UiEvent::SelectRow { row: target });
    }

    pub(crate) fn activate_selection(&mut self, browser: BrowserSide) {
        if let Some(row) = self.view(browser).selected_row() {
            self.submit_ui(browser, UiEvent::ActivateRow { row });
        }
    }

    pub fn drain(&mut self) -> bool {
        if let Some(worker) = &self.settings_worker {
            for result in worker.take_results() {
                match result {
                    SettingsResult::Loaded(snapshot) => self.submit(dual_pane_application::Event::SettingsLoaded { snapshot }),
                    SettingsResult::Saved { revision } => self.submit(dual_pane_application::Event::SettingsSaved { revision }),
                    SettingsResult::Failed(_) | SettingsResult::Reset { .. } => self.submit(dual_pane_application::Event::SettingsSaveFailed { revision: 0 }),
                }
            }
        }
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

    pub fn view(&self, browser: BrowserSide) -> &BrowserViewModel {
        match browser {
            BrowserSide::Left => self.left.view(),
            BrowserSide::Right => self.right.view(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dual_pane_application::{Command, Event, WorkRequest};
    use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, RequestToken, Selection, SortSpec, TabId};

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

    fn assert_view(session: &WorkspaceSession<FakeRunner>, browser: BrowserSide, expected: (&str, &[&str], Option<&str>, &str, bool, u64)) {
        let (path, rows, selected, status, loading, revision) = expected;
        let view = session.view(browser);
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
        assert_eq!(view.folder_items_revision(), revision);
    }

    #[test]
    fn loading_failure_cancellation_and_late_results_preserve_the_other_browser() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(path("base"));
        assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 0));
        assert_view(&session, BrowserSide::Right, ("", &[], None, "Loading…", true, 0));
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]) });
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]) });
        session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
        session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
        assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

        session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("older") });
        let older = RequestToken::first().next().next();
        session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("missing") });
        let missing = older.next();
        assert_eq!(session.runner.dispatched[3], WorkRequest::Cancel { browser: BrowserSide::Left, tab: TabId::new(0), token: older });
        session.submit(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: older });
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: older, entries: Arc::from(vec![entry("late")]) });
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: missing, entries: Arc::from(vec![entry("wrong")]) });
        assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "Loading…", true, 1));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

        session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: TabId::new(0), token: missing, kind: ListingErrorKind::ItemMissing });
        assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "“/missing” no longer exists.", false, 1));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
        session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("cancelled") });
        let cancelled = missing.next();
        session.submit(Event::FolderItemsCancelled { browser: BrowserSide::Left, tab: TabId::new(0), token: cancelled });
        assert_view(&session, BrowserSide::Left, ("/base", &["left"], Some("left"), "/base", false, 1));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));

        session.submit(Command::Navigate { browser: BrowserSide::Left, location: path("final") });
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: cancelled.next(), entries: Arc::from(vec![entry("fresh")]) });
        assert_view(&session, BrowserSide::Left, ("/final", &["fresh"], None, "/final", false, 2));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], Some("right"), "/base", false, 1));
    }

    #[test]
    fn bounded_drain_applies_one_browsers_result_without_touching_the_other() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), 1, DRAIN_TIME_BUDGET);
        session.start(path("base"));
        session.runner.delivered.push_back(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]) });
        session.runner.delivered.push_back(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]) });
        assert!(session.drain());
        assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 0));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], None, "/base", false, 1));
        assert!(!session.drain());
        assert_view(&session, BrowserSide::Left, ("/base", &["left"], None, "/base", false, 1));
        assert_view(&session, BrowserSide::Right, ("/base", &["right"], None, "/base", false, 1));
    }

    #[test]
    fn start_dispatches_distinct_reads_for_both_browsers() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());
        assert_eq!(session.runner.dispatched, vec![WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), location: Location::root(), sort: SortSpec::default() }, WorkRequest::ReadDirectory { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), location: Location::root(), sort: SortSpec::default() }]);
    }

    #[test]
    fn folder_items_revisions_and_selection_are_browser_local() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("left")]) });
        assert_eq!(session.view(BrowserSide::Left).folder_items_revision(), 1);
        assert_eq!(session.view(BrowserSide::Right).folder_items_revision(), 0);

        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("right")]) });
        session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
        assert_eq!(session.view(BrowserSide::Left).folder_items_revision(), 1);
        assert_eq!(session.view(BrowserSide::Right).folder_items_revision(), 1);
        assert_eq!(session.view(BrowserSide::Left).selected_row(), None);
        assert_eq!(session.view(BrowserSide::Right).selected_row(), Some(0));
    }

    #[test]
    fn vertical_arrow_movement_obeys_empty_unselected_and_boundary_cases() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());

        session.move_selection(BrowserSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(BrowserSide::Left).selected_row(), None);

        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("first"), entry("second"), entry("third")]) });
        session.move_selection(BrowserSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(BrowserSide::Left).selected_row(), Some(0));
        session.move_selection(BrowserSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(BrowserSide::Left).selected_row(), Some(0));
        session.move_selection(BrowserSide::Left, SelectionMovement::Next);
        session.move_selection(BrowserSide::Left, SelectionMovement::Next);
        session.move_selection(BrowserSide::Left, SelectionMovement::Next);
        assert_eq!(session.view(BrowserSide::Left).selected_row(), Some(2));
        assert_eq!(session.view(BrowserSide::Right).selected_row(), None);
    }

    #[test]
    fn movement_at_a_boundary_collapses_a_range_selection() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(Location::root());
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("first"), entry("second")]) });
        session.submit_ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
        session.submit(Command::SelectAll { browser: BrowserSide::Left });
        assert_eq!(session.view(BrowserSide::Left).selection().entries().len(), 2);
        session.move_selection(BrowserSide::Left, SelectionMovement::Previous);
        assert_eq!(session.view(BrowserSide::Left).selection().entries(), &[EntryName::new("first").unwrap()]);
        assert_eq!(session.view(BrowserSide::Left).selected_row(), Some(0));
    }

    #[test]
    fn horizontal_arrow_actions_navigate_the_addressed_browser() {
        let folder = EntryName::new("folder").unwrap();
        let location = Location::root().join(&EntryName::new("parent").unwrap());
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(location.clone());
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first(), entries: Arc::from(vec![entry("folder")]) });
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next(), entries: Arc::from(vec![entry("folder")]) });
        session.runner.dispatched.clear();

        session.submit_ui(BrowserSide::Left, UiEvent::GoToParent);
        session.submit_ui(BrowserSide::Right, UiEvent::SelectRow { row: 0 });
        session.activate_selection(BrowserSide::Right);

        assert_eq!(session.runner.dispatched, vec![WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab: TabId::new(0), token: RequestToken::first().next().next(), location: Location::root(), sort: SortSpec::default() }, WorkRequest::ReadDirectory { browser: BrowserSide::Right, tab: TabId::new(1), token: RequestToken::first().next().next().next(), location: location.join(&folder), sort: SortSpec::default() }]);
    }

    #[test]
    fn switching_tabs_restores_the_visible_listing_and_ignores_inactive_results() {
        let mut session = WorkspaceSession::new(FakeRunner::default(), Location::root(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(path("first"));
        let first = session.workspace.active_tab(BrowserSide::Left);
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: first, token: RequestToken::first(), entries: Arc::from(vec![entry("one")]) });
        session.submit(Command::NewTab { browser: BrowserSide::Left });
        let second = session.workspace.active_tab(BrowserSide::Left);
        assert_ne!(first, second);
        assert_view(&session, BrowserSide::Left, ("", &[], None, "Loading…", true, 2));

        session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: first });
        assert_view(&session, BrowserSide::Left, ("/first", &["one"], None, "/first", false, 3));
        let token = match session.runner.dispatched.last() {
            Some(WorkRequest::ReadDirectory { token, .. }) => *token,
            _ => panic!("new tab read"),
        };
        session.submit(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: second, token, entries: Arc::from(vec![entry("two")]) });
        assert_view(&session, BrowserSide::Left, ("/first", &["one"], None, "/first", false, 3));

        session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: second });
        assert_view(&session, BrowserSide::Left, ("/first", &["two"], None, "/first", false, 4));

        session.submit(Command::Refresh { browser: BrowserSide::Left });
        let refresh = match session.runner.dispatched.last() {
            Some(WorkRequest::ReadDirectory { token, .. }) => *token,
            _ => panic!("refresh read"),
        };
        session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab: second, token: refresh, kind: ListingErrorKind::PermissionDenied });
        session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: first });
        session.submit(Command::ActivateTab { browser: BrowserSide::Left, tab: second });
        assert_view(&session, BrowserSide::Left, ("/first", &["two"], None, "You don’t have permission to open “/first”.", false, 6));
    }

    #[test]
    fn closing_the_final_tab_reads_the_supplied_home() {
        let home = path("synthetic-home");
        let mut session = WorkspaceSession::new(FakeRunner::default(), home.clone(), DRAIN_SLICE, DRAIN_TIME_BUDGET);
        session.start(path("start"));
        let old = session.workspace.active_tab(BrowserSide::Left);
        session.submit(Command::CloseTab { browser: BrowserSide::Left, tab: old });
        let new = session.workspace.active_tab(BrowserSide::Left);
        assert_ne!(old, new);
        assert!(matches!(session.runner.dispatched.last(), Some(WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, location, .. }) if *tab == new && *location == home));
    }
}
