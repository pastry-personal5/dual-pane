//! View models and gesture routing for the tabbed desktop interface, driven
//! through a real `Workspace` without Qt.

use std::sync::Arc;

use dual_pane_adapters::{BrowserPresenter, BrowserViewModel, FavoritesEvent, FolderItemsColumn, GroupMotion, InputController, SelectionMovement, UiEvent, WorkspacePresenter, favorite_rejection_text};
use dual_pane_application::{Command, Event, FavoriteEdit, FavoriteGroupRecord, FavoriteItemRecord, FavoriteRejection, FavoritesRecords, Input, SettingsFailure, SettingsSnapshot, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryMetadata, EntryName, Location, ScrollAnchor, SortDirection, SortField, SortSpec};
use jiff::tz::TimeZone;

const NOW: i64 = 1_790_000_000;

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}
fn path(names: &[&str]) -> Location {
    Location::from_components(names.iter().map(|value| name(value)))
}
fn file(value: &str, modified: Option<i64>, size: Option<u64>) -> Entry {
    Entry::with_metadata(name(value), EntryKind::File, EntryMetadata::new(modified, size))
}
fn folder(value: &str) -> Entry {
    Entry::with_metadata(name(value), EntryKind::Directory, EntryMetadata::new(Some(NOW - 7_200), Some(96)))
}

/// A Qt-free stand-in for the desktop session: one workspace, both Browser
/// presenters, the workspace presenter, and the input controller.
struct Session {
    workspace: Workspace,
    left: BrowserPresenter,
    right: BrowserPresenter,
    sidebar: WorkspacePresenter,
    controller: InputController,
    work: Vec<WorkRequest>,
}

impl Session {
    fn new() -> Self {
        let presenter = |side| BrowserPresenter::with_clock(side, Arc::new(|| NOW), TimeZone::UTC);
        let mut session = Self { workspace: Workspace::new(), left: presenter(BrowserSide::Left), right: presenter(BrowserSide::Right), sidebar: WorkspacePresenter::new(), controller: InputController::new(), work: Vec::new() };
        session.submit(Event::SettingsLoaded { snapshot: SettingsSnapshot { favorites: FavoritesRecords { initialized: true, groups: vec![FavoriteGroupRecord { id: 1, name: "Work".into(), position: 1 }, FavoriteGroupRecord { id: 2, name: "Home".into(), position: 0 }], items: vec![FavoriteItemRecord { id: 10, group_id: 1, name: "Docs".into(), target: path(&["docs"]), position: 0 }, FavoriteItemRecord { id: 11, group_id: 1, name: "Notes".into(), target: path(&["notes"]), position: 1 }] }, ..SettingsSnapshot::default() } });
        session
    }
    fn submit(&mut self, input: impl Into<Input>) {
        let transition = self.workspace.handle(input.into());
        for output in &transition.outputs {
            self.left.apply(output);
            self.right.apply(output);
            self.sidebar.apply(output);
        }
        self.left.apply_chrome(&self.workspace.browser_chrome(BrowserSide::Left));
        self.right.apply_chrome(&self.workspace.browser_chrome(BrowserSide::Right));
        self.sidebar.apply_chrome(&self.workspace.workspace_chrome());
        self.work.extend(transition.work);
    }
    fn presenter(&self, browser: BrowserSide) -> &BrowserPresenter {
        match browser {
            BrowserSide::Left => &self.left,
            BrowserSide::Right => &self.right,
        }
    }
    fn ui(&mut self, browser: BrowserSide, event: UiEvent) {
        self.submit(Command::ActivateBrowser { browser });
        if let Some(command) = self.controller.command(browser, event, self.presenter(browser).view()) {
            self.submit(command);
        }
    }
    fn favorites(&mut self, event: FavoritesEvent) -> Option<Command> {
        let browser = self.workspace.active_browser();
        let command = self.controller.favorites_command(event, self.sidebar.view(), browser, self.presenter(browser).view());
        if let Some(command) = command.clone() {
            self.submit(command);
        }
        command
    }
    /// Completes the latest read with `entries`.
    fn deliver(&mut self, browser: BrowserSide, entries: Vec<Entry>) {
        let (tab, token) = self
            .work
            .iter()
            .rev()
            .find_map(|work| match work {
                WorkRequest::ReadDirectory { browser: requested, tab, token, .. } if *requested == browser => Some((*tab, *token)),
                _ => None,
            })
            .expect("a pending read");
        self.submit(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(entries), changes: None });
    }
    fn show(&mut self, browser: BrowserSide, location: Location, entries: Vec<Entry>) {
        self.submit(Command::Navigate { browser, location });
        self.deliver(browser, entries);
    }
}

#[test]
fn rows_show_six_columns_in_the_specified_formats() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![folder("Photos"), file("a.tar.GZ", Some(NOW - 3 * 86_400), Some(112_500)), file("Makefile", None, None), file("tiny", Some(NOW + 30), Some(512))]);
    let view = session.left.view();
    let row = |index| FolderItemsColumn::ALL.map(|column| view.cell_text(index, column).unwrap());
    assert_eq!(row(0), ["", "Photos", "[DIR]", "2 h", "2026-09-21 12:13", ""]);
    assert_eq!(row(1), ["", "a.tar.GZ", "gz", "3 d", "2026-09-18 14:13", "112.5 KB"]);
    assert_eq!(row(2), ["", "Makefile", "", "—", "—", "—"]);
    assert_eq!(row(3), ["", "tiny", "", "now", "2026-09-21 14:13", "512 B"]);
    assert_eq!(view.cell_text(4, FolderItemsColumn::Name), None);
    assert_eq!(view.row_path(1).as_deref(), Some("/work/a.tar.GZ"));
    assert_eq!(FolderItemsColumn::ALL.map(FolderItemsColumn::title), ["Item Icon Column", "Name", "Type", "Relative Date Column", "Exact Date Column", "Size"]);
}

fn parts(view: &BrowserViewModel) -> (&str, &str, &str) {
    let summary = view.summary();
    (&summary.count, &summary.selected, &summary.size)
}

#[test]
fn a_link_to_a_folder_shows_link_type_and_counts_as_a_folder() {
    let mut session = Session::new();
    let link = Entry::with_metadata(name("to-photos"), EntryKind::Symlink { points_to_directory: true }, EntryMetadata::new(Some(NOW - 60), Some(9)));
    session.show(BrowserSide::Left, path(&["work"]), vec![folder("Photos"), link]);
    let view = session.left.view();
    assert_eq!((view.cell_text(1, FolderItemsColumn::Type).unwrap(), view.cell_text(1, FolderItemsColumn::Size).unwrap()), ("[LNK]".to_owned(), String::new()));
    assert_eq!(parts(view), ("2 items", "", "0 B"));
}

#[test]
fn the_summary_counts_and_totals_the_selection_or_else_the_folder() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![folder("Photos"), file("a", None, Some(600)), file("b", None, Some(900)), file("c", None, None)]);
    assert_eq!(parts(session.left.view()), ("4 items", "", "—"));
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 1 });
    session.ui(BrowserSide::Left, UiEvent::ExtendToRow { row: 2 });
    assert_eq!(parts(session.left.view()), ("4 items", "2 selected", "1.5 KB / —"));
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    assert_eq!(parts(session.left.view()), ("4 items", "1 selected", "0 B / —"), "folders are excluded from totals");
    assert_eq!(parts(session.right.view()), ("", "", ""), "no summary before a listing");

    // A failed read keeps the last successful listing and its summary.
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path(&["missing"]) });
    let (tab, token) = match session.work.last() {
        Some(WorkRequest::ReadDirectory { tab, token, .. }) => (*tab, *token),
        other => panic!("{other:?}"),
    };
    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token, kind: dual_pane_domain::ListingErrorKind::ItemMissing });
    let view = session.left.view();
    assert_eq!((view.location_text(), parts(view), view.status_text()), ("/work", ("4 items", "1 selected", "0 B / —"), "“/missing” no longer exists."));
}

#[test]
fn a_command_click_can_leave_the_cursor_off_the_only_selected_item() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![file("a", None, None), file("b", None, None), file("c", None, None)]);
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    session.ui(BrowserSide::Left, UiEvent::ToggleRow { row: 2 });
    session.ui(BrowserSide::Left, UiEvent::ToggleRow { row: 2 });
    let view = session.left.view();
    assert_eq!((view.selected_rows(), view.cursor_row()), (vec![0], Some(2)));
    session.work.clear();
    session.ui(BrowserSide::Left, UiEvent::ActivateSelection);
    assert!(session.work.is_empty(), "a file cannot be entered, and the cursor row is not the target");
    let command = session.controller.command(BrowserSide::Left, UiEvent::ActivateSelection, session.left.view());
    assert!(matches!(command, Some(Command::OpenEntry { row: 0, ref name, .. }) if *name == EntryName::new("a").unwrap()));

    // With several Items selected, activation does nothing.
    session.ui(BrowserSide::Left, UiEvent::SelectAll);
    assert_eq!(session.controller.command(BrowserSide::Left, UiEvent::ActivateSelection, session.left.view()), None);
}

#[test]
fn movement_starts_from_the_cursor_even_without_a_selection_and_shift_extends_from_the_anchor() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), (0..5).map(|index| file(&format!("item-{index}"), None, None)).collect());
    session.ui(BrowserSide::Left, UiEvent::ExtendSelection(SelectionMovement::Next));
    assert!(session.left.view().selected_rows().is_empty(), "Shift-movement needs an anchor");
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 1 });
    session.ui(BrowserSide::Left, UiEvent::ToggleRow { row: 1 });
    assert!(session.left.view().selected_rows().is_empty());
    session.ui(BrowserSide::Left, UiEvent::MoveSelection(SelectionMovement::Next));
    assert_eq!(session.left.view().selected_rows(), vec![2], "Down moves on from the deselected cursor row");
    session.ui(BrowserSide::Left, UiEvent::ExtendSelection(SelectionMovement::Next));
    session.ui(BrowserSide::Left, UiEvent::ExtendSelection(SelectionMovement::Next));
    assert_eq!((session.left.view().selected_rows(), session.left.view().cursor_row()), (vec![2, 3, 4], Some(4)));
    session.ui(BrowserSide::Left, UiEvent::SecondaryRow { row: 3 });
    assert_eq!(session.left.view().selected_rows(), vec![2, 3, 4], "right-click on a selected Item keeps the selection");
    session.ui(BrowserSide::Left, UiEvent::SecondaryRow { row: 0 });
    assert_eq!(session.left.view().selected_rows(), vec![0]);
}

#[test]
fn tabs_show_folder_names_full_paths_and_the_root_volume_name() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["a", "docs"]), vec![]);
    session.ui(BrowserSide::Left, UiEvent::NewTab);
    session.deliver(BrowserSide::Left, vec![]);
    session.show(BrowserSide::Left, path(&["b", "docs"]), vec![]);
    session.ui(BrowserSide::Left, UiEvent::NewTab);
    session.deliver(BrowserSide::Left, vec![]);
    session.show(BrowserSide::Left, Location::root(), vec![]);
    let labels = |session: &Session| session.left.view().tabs().iter().map(|tab| (tab.label.clone(), tab.path.clone(), tab.active)).collect::<Vec<_>>();
    assert_eq!(labels(&session), [("docs".to_owned(), "/a/docs".to_owned(), false), ("docs".to_owned(), "/b/docs".to_owned(), false), ("/".to_owned(), "/".to_owned(), true)]);
    assert_eq!(session.left.view().folder_name(), "/");
    session.left.set_root_label("Macintosh HD");
    assert_eq!(labels(&session)[2].0, "Macintosh HD");
    assert_eq!(session.left.view().folder_name(), "Macintosh HD");
    assert!(session.right.view().tabs().iter().all(|tab| tab.label.is_empty()), "the Right Browser has its own tabs");

    let first = session.left.view().tabs()[0].id;
    let revision = session.left.view().tabs_revision();
    session.ui(BrowserSide::Left, UiEvent::ReorderTab { tab: first, position: 2 });
    assert_eq!(labels(&session).iter().map(|(_, path, _)| path.as_str()).collect::<Vec<_>>(), ["/b/docs", "/", "/a/docs"]);
    assert_ne!(session.left.view().tabs_revision(), revision);
    session.ui(BrowserSide::Left, UiEvent::CloseActiveTab);
    assert_eq!(labels(&session), [("docs".to_owned(), "/b/docs".to_owned(), false), ("docs".to_owned(), "/a/docs".to_owned(), true)], "closing the active tab activates its right neighbor");
}

#[test]
fn toolbar_state_and_sort_commands_follow_the_active_tab() {
    let mut session = Session::new();
    assert_eq!(session.controller.command(BrowserSide::Left, UiEvent::Sort(SortSpec::default()), session.left.view()), None, "nothing to sort before a listing");
    session.show(BrowserSide::Left, path(&["work"]), vec![]);
    session.show(BrowserSide::Left, path(&["work", "deeper"]), vec![]);
    let toolbar = session.left.view().toolbar();
    assert!(toolbar.can_go_back && !toolbar.can_go_forward && toolbar.can_go_up && toolbar.can_open_tab && toolbar.can_refresh);
    assert_eq!(toolbar.sort, Some(SortSpec::default()));
    let size = SortSpec::new(SortField::Size, SortDirection::Ascending);
    session.ui(BrowserSide::Left, UiEvent::Sort(size));
    assert_eq!(session.left.view().toolbar().sort, Some(size));
    assert!(session.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { sort, location, .. } if *sort == size && *location == path(&["work", "deeper"]))));
    session.ui(BrowserSide::Left, UiEvent::GoBack);
    assert!(session.left.view().toolbar().can_go_forward);
}

#[test]
fn scrolling_records_an_anchor_for_the_active_tab() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), (0..4).map(|index| file(&format!("item-{index}"), None, None)).collect());
    let command = session.controller.command(BrowserSide::Left, UiEvent::Scrolled { row: Some(2), offset: 5 }, session.left.view());
    let tab = session.left.view().active_tab().unwrap();
    assert_eq!(command, Some(Command::UpdateScrollHint { browser: BrowserSide::Left, tab, scroll: Some(ScrollAnchor::new(name("item-2"), 5)) }));
    session.ui(BrowserSide::Left, UiEvent::Scrolled { row: Some(2), offset: 5 });
    session.ui(BrowserSide::Left, UiEvent::NewTab);
    session.deliver(BrowserSide::Left, vec![]);
    session.ui(BrowserSide::Left, UiEvent::ActivateTab { tab });
    assert_eq!(session.left.view().scroll_hint_row(), Some((2, 5)));
}

#[test]
fn sidebar_groups_follow_their_positions_and_route_stable_ids() {
    let mut session = Session::new();
    let names = |session: &Session| session.sidebar.view().groups().iter().map(|group| (group.name.clone(), group.items.iter().map(|item| item.alias.clone()).collect::<Vec<_>>())).collect::<Vec<_>>();
    assert_eq!(names(&session), [("Home".to_owned(), vec![]), ("Work".to_owned(), vec!["Docs".to_owned(), "Notes".to_owned()])]);
    assert!(session.sidebar.view().favorites_ready());
    assert_eq!(session.favorites(FavoritesEvent::MoveGroup { id: 1, motion: GroupMotion::Up }), Some(Command::ReorderFavoriteGroup { id: 1, position: 0 }));
    assert_eq!(session.favorites(FavoritesEvent::MoveGroup { id: 2, motion: GroupMotion::Bottom }), Some(Command::ReorderFavoriteGroup { id: 2, position: 1 }));
    assert_eq!(session.favorites(FavoritesEvent::DropItem { id: 10, group_id: 1, slot: 2 }), Some(Command::MoveFavoriteItem { id: 10, group_id: 1, position: 1 }), "a slot below the Item's own row counts without it");
    assert_eq!(names(&session)[0].1, ["Notes", "Docs"]);
    assert_eq!(session.favorites(FavoritesEvent::DropItem { id: 10, group_id: 2, slot: 0 }), Some(Command::MoveFavoriteItem { id: 10, group_id: 2, position: 0 }));
    assert_eq!(names(&session)[1].1, ["Docs"]);
    session.work.clear();
    session.favorites(FavoritesEvent::OpenItem { id: 11 });
    assert!(matches!(session.work.as_slice(), [WorkRequest::ReadDirectory { browser: BrowserSide::Left, location, .. }] if *location == path(&["notes"])));
}

#[test]
fn adding_the_active_folder_uses_its_name_and_reports_a_duplicate_inline() {
    let mut session = Session::new();
    assert_eq!(session.favorites(FavoritesEvent::AddItem { group_id: 1 }), None, "nothing to add before a listing");
    session.show(BrowserSide::Right, path(&["projects", "Docs"]), vec![]);
    session.submit(Command::ActivateBrowser { browser: BrowserSide::Right });
    let added = session.favorites(FavoritesEvent::AddItem { group_id: 1 });
    assert_eq!(added, Some(Command::CreateFavoriteItem { group_id: 1, name: "Docs".into(), target: path(&["projects", "Docs"]) }));
    let rejection = session.sidebar.view().last_rejection();
    assert_eq!(rejection, Some((FavoriteEdit::CreateItem { group_id: 1 }, FavoriteRejection::DuplicateName)));
    let (edit, reason) = rejection.unwrap();
    assert_eq!(favorite_rejection_text(edit, reason), Some("Already in this group"));
    assert_eq!(session.workspace.notices().len(), 0, "an inline rejection adds no Notice");

    session.show(BrowserSide::Right, Location::root(), vec![]);
    session.right.set_root_label("Macintosh HD");
    assert_eq!(session.favorites(FavoritesEvent::AddItem { group_id: 2 }), Some(Command::CreateFavoriteItem { group_id: 2, name: "Macintosh HD".into(), target: Location::root() }));
    assert_eq!(session.sidebar.view().last_rejection(), None);
    assert_eq!(session.sidebar.view().groups()[0].items[0].path, "/");
}

#[test]
fn notices_are_worded_and_actionable_storage_errors_request_opening() {
    let mut session = Session::new();
    let opened = session.sidebar.view().open_requests();
    let Some(WorkRequest::SaveSettings { revision, .. }) = ({
        session.favorites(FavoritesEvent::CreateGroup { name: "New".into() });
        session.work.iter().rev().find(|work| matches!(work, WorkRequest::SaveSettings { .. })).cloned()
    }) else {
        panic!("a save")
    };
    session.submit(Event::SettingsSaveFailed { revision, failure: SettingsFailure::Unavailable });
    let notices = session.sidebar.view().notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].text, "Dual Pane couldn’t save settings. Your changes stay in effect until you quit.");
    assert!(notices[0].offers_reset);
    assert_eq!(session.sidebar.view().open_requests(), opened + 1);
    session.submit(Command::ResetSettings);
    assert!(!session.sidebar.view().favorites_ready());
    session.submit(Event::SettingsReset { backup: Some(path(&["Library", "settings.failed-1"])) });
    assert_eq!(session.sidebar.view().notices()[1].text, "Settings were reset. The previous settings were preserved at “/Library/settings.failed-1”.");
}

#[test]
fn effective_bindings_are_exposed_and_change_after_a_load() {
    use dual_pane_application::{ActionBinding, ActionId, Key, Shortcut};
    let mut session = Session { workspace: Workspace::new(), left: BrowserPresenter::new(BrowserSide::Left), right: BrowserPresenter::new(BrowserSide::Right), sidebar: WorkspacePresenter::new(), controller: InputController::new(), work: Vec::new() };
    session.submit(Command::ActivateBrowser { browser: BrowserSide::Left });
    let binding = |session: &Session, action| session.sidebar.view().bindings().iter().find(|binding| binding.action == action).and_then(|binding| binding.shortcut);
    assert_eq!(binding(&session, ActionId::CloseTab), dual_pane_application::default_shortcut(ActionId::CloseTab));
    let revision = session.sidebar.view().bindings_revision();
    session.submit(Command::ActivateBrowser { browser: BrowserSide::Right });
    assert_eq!(session.sidebar.view().bindings_revision(), revision, "an unrelated input does not rebind");
    assert_eq!(session.sidebar.view().active_browser(), BrowserSide::Right);
    let custom = Shortcut::new(true, true, false, false, Key::Character('T'));
    session.submit(Event::SettingsLoaded { snapshot: SettingsSnapshot { bindings: vec![ActionBinding { action: ActionId::NewTab, shortcut: Some(custom) }], ..SettingsSnapshot::default() } });
    assert_ne!(session.sidebar.view().bindings_revision(), revision, "a load rebinds");
    assert_eq!(binding(&session, ActionId::NewTab), Some(custom));
}
