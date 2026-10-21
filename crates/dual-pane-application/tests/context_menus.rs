use std::sync::Arc;

use dual_pane_application::{ActionId, ClipboardAction, Command, ContextMenuKind, Event, FolderItemsMenuSnapshot, FolderItemsMenuTarget, FolderItemsMenuToken, Output, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, RequestToken, TabId};

fn name(value: &str) -> EntryName {
    EntryName::new(value).unwrap()
}

fn location(value: &str) -> Location {
    Location::root().join(&name(value))
}

fn read(work: &[WorkRequest]) -> (TabId, RequestToken) {
    work.iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { tab, token, .. } => Some((*tab, *token)),
            _ => None,
        })
        .unwrap()
}

fn show(workspace: &mut Workspace, browser: BrowserSide, at: Location, entries: Vec<Entry>) -> (TabId, u64) {
    let (tab, token) = read(&workspace.handle(Command::Navigate { browser, location: at }.into()).work);
    let transition = workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: Arc::from(entries), changes: None }.into());
    let revision = transition
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::FolderItemsLoaded { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    (tab, revision)
}

fn setup(entries: Vec<Entry>) -> (Workspace, TabId, u64, TabId) {
    let mut workspace = Workspace::new();
    let (left, revision) = show(&mut workspace, BrowserSide::Left, location("from"), entries);
    let (right, _) = show(&mut workspace, BrowserSide::Right, location("to"), vec![]);
    (workspace, left, revision, right)
}

fn request(workspace: &mut Workspace, tab: TabId, revision: u64, kind: ContextMenuKind, target: FolderItemsMenuTarget) -> Option<FolderItemsMenuSnapshot> {
    workspace.handle(Command::RequestFolderItemsMenu { browser: BrowserSide::Left, tab, visible_revision: revision, kind, target }.into()).outputs.into_iter().find_map(|output| match output {
        Output::FolderItemsMenuReady { snapshot } => snapshot,
        _ => None,
    })
}

fn actions(snapshot: &FolderItemsMenuSnapshot) -> Vec<ActionId> {
    snapshot.sections.iter().flat_map(|section| section.actions.iter().map(|action| action.action)).collect()
}

#[test]
fn row_requests_apply_secondary_selection_and_project_only_available_configured_actions() {
    let entries = vec![Entry::new(name("first"), EntryKind::File), Entry::new(name("second"), EntryKind::File)];
    let (mut workspace, tab, revision, _) = setup(entries);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("first") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab, row: 1, name: name("second") }.into());

    let selected = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 1, name: name("second") }).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("first"), name("second")], "a selected row preserves the selection");
    assert_eq!(actions(&selected), vec![ActionId::CopyItems, ActionId::CutItems, ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::MoveToTrash]);

    let unselected = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("first"), name("second")], "another selected row also preserves the selection");
    assert_eq!(actions(&unselected), actions(&selected));

    workspace.handle(Command::ClearSelection { browser: BrowserSide::Left, tab }.into());
    let sole = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 1, name: name("second") }).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("second")]);
    assert_eq!(actions(&sole), vec![ActionId::OpenItem, ActionId::CopyItems, ActionId::CutItems, ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::MoveToTrash]);
}

#[test]
fn empty_space_preserves_selection_and_suppresses_every_source_action() {
    let (mut workspace, tab, revision, _) = setup(vec![Entry::new(name("item"), EntryKind::File)]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("item") }.into());
    assert!(request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::EmptySpace).is_none(), "Normal has no configured available destination action with an unsupported clipboard");
    let comprehensive = request(&mut workspace, tab, revision, ContextMenuKind::Comprehensive, FolderItemsMenuTarget::EmptySpace).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("item")]);
    assert_eq!(actions(&comprehensive), vec![ActionId::NewFolder]);
}

#[test]
fn unknown_tokens_do_not_retire_the_live_menu_and_valid_tokens_are_one_use() {
    let (mut workspace, tab, revision, _) = setup(vec![Entry::new(name("item"), EntryKind::File)]);
    let snapshot = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    let unknown = FolderItemsMenuToken(snapshot.token.0.wrapping_add(100));
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: unknown, action: ActionId::CopyItems }.into()), Default::default());

    let invoked = workspace.handle(Command::InvokeFolderItemsMenu { token: snapshot.token, action: ActionId::CopyItems }.into());
    assert!(matches!(invoked.work.as_slice(), [WorkRequest::PublishClipboard { action: ClipboardAction::Copy, .. }]));
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: snapshot.token, action: ActionId::CopyItems }.into()), Default::default());
}

#[test]
fn browser_listing_and_destination_changes_stale_a_snapshot_before_dispatch() {
    let (mut workspace, tab, revision, right) = setup(vec![Entry::new(name("item"), EntryKind::File)]);
    let copy = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: copy.token, action: ActionId::CopyItems }.into()), Default::default(), "a menu cannot act after another Browser became active");

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Left }.into());
    let create = request(&mut workspace, tab, revision, ContextMenuKind::Comprehensive, FolderItemsMenuTarget::EmptySpace).unwrap();
    let refresh = workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into());
    let (_, refreshed) = read(&refresh.work);
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: create.token, action: ActionId::NewFolder }.into()), Default::default(), "a refresh retires destination actions without opening an editor or reporting a current-command rejection");

    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token: refreshed, entries: Arc::from(vec![Entry::new(name("item"), EntryKind::File)]), changes: None }.into());
    let revision = loaded
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::FolderItemsLoaded { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    let transfer = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    workspace.handle(Command::Refresh { browser: BrowserSide::Right, tab: right }.into());
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: transfer.token, action: ActionId::CopyToOtherBrowser }.into()), Default::default(), "a destination refresh cannot turn a stale snapshot into an Operation rejection or job");
}
