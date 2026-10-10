//! View models and gesture routing for the tabbed desktop interface, driven
//! through a real `Workspace` without Qt.

use std::sync::Arc;

use dual_pane_adapters::{BrowserPresenter, BrowserViewModel, FavoritesEvent, FolderItemsColumn, GroupMotion, InputController, SelectionMovement, UiEvent, WorkspacePresenter, favorite_rejection_text};
use dual_pane_application::{Command, Event, FavoriteEdit, FavoriteGroupRecord, FavoriteItemRecord, FavoriteRejection, FavoritesRecords, Input, MonitoringStatus, SettingsFailure, SettingsSnapshot, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryMetadata, EntryName, ListingErrorKind, Location, ScrollAnchor, SortDirection, SortField, SortSpec};
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

#[test]
fn monitoring_feedback_yields_to_loading_and_listing_errors() {
    let mut session = Session::new();
    let folder = path(&["watched"]);
    session.show(BrowserSide::Left, folder.clone(), vec![file("a", None, None)]);
    let generation = session
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::WatchLocation { location, generation } if *location == folder => Some(*generation),
            _ => None,
        })
        .unwrap();
    session.submit(Event::WatcherStatusChanged { location: folder.clone(), generation, status: MonitoringStatus::Periodic });
    assert_eq!(session.left.view().status_text(), "Checking for changes periodically");

    session.submit(Event::WatchedLocationInvalidated { location: folder.clone(), generation });
    assert_eq!(session.left.view().status_text(), "Loading…");
    let (tab, token) = session
        .work
        .iter()
        .rev()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser: BrowserSide::Left, tab, token, location, .. } if *location == folder => Some((*tab, *token)),
            _ => None,
        })
        .unwrap();
    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token, kind: ListingErrorKind::ItemMissing });
    session.submit(Event::WatcherStatusChanged { location: folder.clone(), generation, status: MonitoringStatus::Unavailable });
    assert_eq!(session.left.view().status_text(), "“/watched” no longer exists.");

    session.submit(Event::WatchedLocationInvalidated { location: folder, generation });
    session.deliver(BrowserSide::Left, vec![file("a", None, None)]);
    assert_eq!(session.left.view().status_text(), "Automatic refresh unavailable — Command+R to Refresh");
}

#[test]
fn native_preview_paths_preserve_name_bytes_and_directory_kind() {
    let mut session = Session::new();
    let location = Location::from_components([name("work"), EntryName::new(b"raw\xFF".to_vec()).unwrap()]);
    let image_name = EntryName::new(b"image\xFE.png".to_vec()).unwrap();
    let image = Entry::new(image_name, EntryKind::File);
    session.show(BrowserSide::Left, location, vec![folder("Photos"), image]);

    let view = session.left.view();
    assert_eq!(view.row_native_path(0), Some((b"/work/raw\xFF/Photos".to_vec(), true)));
    assert_eq!(view.row_native_path(1), Some((b"/work/raw\xFF/image\xFE.png".to_vec(), false)));
    assert_eq!(view.row_native_path(2), None);
    assert_ne!(view.row_path(1).unwrap().as_bytes(), b"/work/raw\xFF/image\xFE.png");
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
    assert_eq!(parts(view), ("2 items", "0 selected", "0 B"));
}

#[test]
fn the_summary_counts_and_totals_the_selection_or_else_the_folder() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![folder("Photos"), file("a", None, Some(600)), file("b", None, Some(900)), file("c", None, None)]);
    assert_eq!(parts(session.left.view()), ("4 items", "0 selected", "—"));
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 1 });
    session.ui(BrowserSide::Left, UiEvent::ExtendToRow { row: 2 });
    assert_eq!(parts(session.left.view()), ("4 items", "2 selected", "1.5 KB / —"));
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 0 });
    assert_eq!(parts(session.left.view()), ("4 items", "1 selected", "0 B / —"), "folders are excluded from totals");
    assert_eq!(parts(session.right.view()), ("", "", ""), "no summary before a listing");

    // A failed read keeps the last successful listing and its summary.
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path(&["missing"]) });
    let (tab, token) = match session.work.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) {
        Some(WorkRequest::ReadDirectory { tab, token, .. }) => (*tab, *token),
        other => panic!("{other:?}"),
    };
    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token, kind: dual_pane_domain::ListingErrorKind::ItemMissing });
    let view = session.left.view();
    assert_eq!((view.location_text(), parts(view), view.status_text()), ("/work", ("4 items", "1 selected", "0 B / —"), "“/missing” no longer exists."));
}

#[test]
fn summary_inflects_one_item_and_keeps_zero_selection_visible() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["one"]), vec![file("only", None, Some(12))]);
    assert_eq!(parts(session.left.view()), ("1 item", "0 selected", "12 B"));

    session.show(BrowserSide::Right, path(&["empty"]), Vec::new());
    assert_eq!(parts(session.right.view()), ("0 items", "0 selected", "0 B"));
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
    assert!(session.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { browser: BrowserSide::Left, location, .. } if *location == path(&["notes"]))));
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

#[test]
fn a_refused_file_command_shows_its_reason_until_its_own_timer_expires() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![file("a", None, None)]);
    session.show(BrowserSide::Right, path(&["other"]), vec![]);
    session.ui(BrowserSide::Left, UiEvent::FileCommand(dual_pane_domain::OperationCommand::MoveToTrash));
    let view = session.left.view();
    assert_eq!(view.status_text(), "Select one or more items first.");
    let first = view.status_token();
    session.ui(BrowserSide::Left, UiEvent::FileCommand(dual_pane_domain::OperationCommand::Rename));
    let second = session.left.view().status_token();
    assert_ne!(first, second);
    session.left.expire_status(first);
    assert_eq!(session.left.view().status_text(), "Select one or more items first.", "an older timer cannot clear a newer reason");
    session.left.expire_status(second);
    assert_eq!(session.left.view().status_text(), "/work", "the path returns");
    assert_eq!(session.right.view().status_text(), "/other", "only the source Browser shows the reason");
}

#[test]
fn rename_and_new_folder_open_their_editors_only_when_available() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![file("a", None, None), file("b", None, None)]);
    session.ui(BrowserSide::Left, UiEvent::SelectRow { row: 1 });
    session.ui(BrowserSide::Left, UiEvent::FileCommand(dual_pane_domain::OperationCommand::Rename));
    let editor = session.left.view().editor().cloned().expect("the Rename Item Editor opens");
    assert_eq!((editor.row, editor.text.as_str()), (Some(1), "b"));
    session.ui(BrowserSide::Left, UiEvent::FileCommand(dual_pane_domain::OperationCommand::NewFolder));
    let editor = session.left.view().editor().cloned().unwrap();
    assert_eq!((editor.command, editor.row, editor.text.as_str()), (dual_pane_domain::OperationCommand::NewFolder, None, ""));
    let revision = session.left.view().editor_revision();
    session.ui(BrowserSide::Left, UiEvent::SelectAll);
    session.ui(BrowserSide::Left, UiEvent::FileCommand(dual_pane_domain::OperationCommand::Rename));
    assert_eq!(session.left.view().editor_revision(), revision, "a refused Rename opens nothing");
    assert_eq!(session.left.view().status_text(), "Select exactly one item to rename.");
    let command = session.controller.name_command(BrowserSide::Left, dual_pane_domain::OperationCommand::NewFolder, name("made"), session.left.view());
    assert!(matches!(command, Some(Command::StartOperation { kind: dual_pane_domain::OperationKind::NewFolder { .. }, .. })));
}

#[test]
fn a_missing_folder_keeps_its_rows_under_the_overlay_until_a_refresh_succeeds() {
    let mut session = Session::new();
    session.show(BrowserSide::Left, path(&["work"]), vec![file("a", None, None)]);
    session.ui(BrowserSide::Left, UiEvent::Refresh);
    let (tab, token) = match session.work.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) {
        Some(WorkRequest::ReadDirectory { tab, token, .. }) => (*tab, *token),
        _ => panic!("a refresh"),
    };
    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token, kind: dual_pane_domain::ListingErrorKind::ItemMissing });
    let view = session.left.view();
    assert!(view.missing_folder());
    assert_eq!((view.row_count(), view.location_text()), (1, "/work"), "the last rows and path stay");
    session.submit(Command::Navigate { browser: BrowserSide::Left, location: path(&["elsewhere"]) });
    let (tab, token) = match session.work.iter().rev().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) {
        Some(WorkRequest::ReadDirectory { tab, token, .. }) => (*tab, *token),
        _ => panic!("a read"),
    };
    session.submit(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token, kind: dual_pane_domain::ListingErrorKind::ItemMissing });
    assert!(session.left.view().missing_folder(), "a failed navigation elsewhere leaves the overlay as it was");
    session.ui(BrowserSide::Left, UiEvent::Refresh);
    session.deliver(BrowserSide::Left, vec![file("a", None, None)]);
    assert!(!session.left.view().missing_folder());
}

#[test]
fn a_package_row_can_show_its_contents() {
    let mut session = Session::new();
    let package = Entry::new(name("Tool.app"), EntryKind::Directory).with_package(true);
    session.show(BrowserSide::Left, path(&["apps"]), vec![package, folder("plain")]);
    assert!(session.left.view().is_package(0) && !session.left.view().is_package(1));
    session.ui(BrowserSide::Left, UiEvent::ShowPackageContents { row: 0 });
    assert!(session.work.iter().rev().any(|work| matches!(work, WorkRequest::ReadDirectory { location, .. } if *location == path(&["apps", "Tool.app"]))));
}
