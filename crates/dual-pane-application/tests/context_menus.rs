use std::sync::Arc;

use dual_pane_application::{ActionId, ClipboardAction, ClipboardGeneration, ClipboardOffer, Command, ContextMenuGroup, ContextMenuKind, ContextMenuPreferenceRejection, Event, FolderItemsMenuSnapshot, FolderItemsMenuTarget, FolderItemsMenuToken, Key, Output, SettingsSnapshot, Shortcut, WorkRequest, Workspace};
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
    request_from(workspace, BrowserSide::Left, tab, revision, kind, target)
}

fn request_from(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, revision: u64, kind: ContextMenuKind, target: FolderItemsMenuTarget) -> Option<FolderItemsMenuSnapshot> {
    workspace.handle(Command::RequestFolderItemsMenu { browser, tab, visible_revision: revision, kind, target }.into()).outputs.into_iter().find_map(|output| match output {
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

#[test]
fn context_menu_setting_edits_reject_the_last_action_and_save_only_changes_and_resets() {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());

    let changed = workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::RenameItem, visible: true }.into());
    assert!(changed.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { snapshot, .. } if snapshot.context_menus.visible(ContextMenuKind::Normal, ActionId::RenameItem))));
    assert!(changed.outputs.is_empty());
    let unchanged = workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::RenameItem, visible: true }.into());
    assert_eq!(unchanged, Default::default());

    for action in dual_pane_application::CONTEXT_MENU_ACTIONS {
        if action != ActionId::RenameItem && workspace.context_menu_preferences().visible(ContextMenuKind::Normal, action) {
            workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action, visible: false }.into());
        }
    }
    let rejected = workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::RenameItem, visible: false }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ContextMenuPreferenceRejected { reason: ContextMenuPreferenceRejection::LastVisibleAction(ContextMenuKind::Normal) }]));
    assert!(rejected.work.is_empty());
    assert!(workspace.context_menu_preferences().visible(ContextMenuKind::Normal, ActionId::RenameItem));

    let reset = workspace.handle(Command::ResetContextMenu { kind: ContextMenuKind::Normal }.into());
    assert!(reset.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { snapshot, .. } if snapshot.context_menus == Default::default())));
    assert_eq!(workspace.context_menu_preferences(), &Default::default());

    workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::RenameItem, visible: true }.into());
    workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Comprehensive, action: ActionId::DeletePermanently, visible: false }.into());
    workspace.handle(Command::ResetContextMenu { kind: ContextMenuKind::Normal }.into());
    assert!(!workspace.context_menu_preferences().visible(ContextMenuKind::Comprehensive, ActionId::DeletePermanently), "a local reset does not change the other variant");
    let both = workspace.handle(Command::ResetContextMenus.into());
    assert!(both.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { snapshot, .. } if snapshot.context_menus == Default::default())));
    assert_eq!(workspace.context_menu_preferences(), &Default::default());
    assert_eq!(workspace.handle(Command::ResetContextMenus.into()), Default::default(), "resetting factory values is a no-op");
}

#[test]
fn comprehensive_projection_groups_package_actions_and_cached_paste_without_io() {
    let package = Entry::new(name("Tool.app"), EntryKind::Directory).with_package(true);
    let (mut workspace, tab, revision, _) = setup(vec![package]);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(7), offer: ClipboardOffer::Files }.into());

    let snapshot = request(&mut workspace, tab, revision, ContextMenuKind::Comprehensive, FolderItemsMenuTarget::Row { row: 0, name: name("Tool.app") }).unwrap();
    assert_eq!(snapshot.sections.iter().map(|section| section.group).collect::<Vec<_>>(), ContextMenuGroup::ALL);
    assert_eq!(actions(&snapshot), dual_pane_application::CONTEXT_MENU_ACTIONS);
    assert!(workspace.handle(Command::DismissFolderItemsMenu { token: snapshot.token }.into()).work.is_empty(), "projection and dismissal perform no clipboard read");

    let empty = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::EmptySpace).unwrap();
    assert_eq!(actions(&empty), vec![ActionId::PasteItems]);
    let invoked = workspace.handle(Command::InvokeFolderItemsMenu { token: empty.token, action: ActionId::PasteItems }.into());
    assert!(matches!(invoked.work.as_slice(), [WorkRequest::ReadClipboard { .. }]), "Paste validates through its ordinary fresh clipboard read only after invocation");
}

#[test]
fn snapshots_capture_first_shortcuts_and_later_settings_changes_only_affect_later_menus() {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    let (tab, revision) = show(&mut workspace, BrowserSide::Left, location("from"), vec![Entry::new(name("item"), EntryKind::File)]);
    show(&mut workspace, BrowserSide::Right, location("to"), vec![]);
    let custom = Shortcut::new(true, true, false, false, Key::Character('O'));
    workspace.handle(Command::SetActionShortcuts { action: ActionId::OpenItem, shortcuts: vec![custom] }.into());
    let first = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    assert_eq!(first.sections.iter().flat_map(|section| &section.actions).find(|row| row.action == ActionId::OpenItem).and_then(|row| row.shortcut), Some(custom));

    workspace.handle(Command::SetActionShortcuts { action: ActionId::OpenItem, shortcuts: vec![] }.into());
    assert_eq!(first.sections.iter().flat_map(|section| &section.actions).find(|row| row.action == ActionId::OpenItem).and_then(|row| row.shortcut), Some(custom), "an open snapshot is immutable");
    let later = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    assert_eq!(later.sections.iter().flat_map(|section| &section.actions).find(|row| row.action == ActionId::OpenItem).and_then(|row| row.shortcut), None);
}

#[test]
fn selection_changes_and_newer_requests_retire_observed_actions_without_retargeting() {
    let entries = vec![Entry::new(name("first"), EntryKind::File), Entry::new(name("second"), EntryKind::File)];
    let (mut workspace, tab, revision, _) = setup(entries);
    let first = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 1, name: name("second") }.into());
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: first.token, action: ActionId::OpenItem }.into()), Default::default(), "selection changes cannot retarget Open Item");

    let older = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    let newer = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 1, name: name("second") }).unwrap();
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: older.token, action: ActionId::CopyItems }.into()), Default::default());
    assert!(matches!(workspace.handle(Command::InvokeFolderItemsMenu { token: newer.token, action: ActionId::CopyItems }.into()).work.as_slice(), [WorkRequest::PublishClipboard { .. }]));
}

#[test]
fn requests_reject_stale_rows_and_work_identically_from_the_right_browser() {
    let entries = vec![Entry::new(name("item"), EntryKind::Symlink { points_to_directory: false })];
    let (mut workspace, left, left_revision, right) = setup(entries.clone());
    let refresh = workspace.handle(Command::Refresh { browser: BrowserSide::Right, tab: right }.into());
    let right_token = read(&refresh.work).1;
    let right_loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Right, tab: right, token: right_token, entries: entries.into(), changes: None }.into());
    let right_revision = right_loaded
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::FolderItemsLoaded { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();

    assert!(request(&mut workspace, left, left_revision.wrapping_add(1), ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).is_none());
    assert!(request(&mut workspace, left, left_revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("other") }).is_none());
    let snapshot = request_from(&mut workspace, BrowserSide::Right, right, right_revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    assert_eq!(actions(&snapshot), vec![ActionId::OpenItem, ActionId::CopyItems, ActionId::CutItems, ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::MoveToTrash]);
}

#[test]
fn non_emitted_dismissed_and_tab_stale_actions_do_nothing_while_destination_actions_ignore_selection() {
    let entries = vec![Entry::new(name("first"), EntryKind::File), Entry::new(name("second"), EntryKind::File)];
    let (mut workspace, tab, revision, _) = setup(entries);
    let normal = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: normal.token, action: ActionId::DeletePermanently }.into()), Default::default(), "a configured but non-emitted action cannot be smuggled through the token");

    let dismissed = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    workspace.handle(Command::DismissFolderItemsMenu { token: dismissed.token }.into());
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: dismissed.token, action: ActionId::OpenItem }.into()), Default::default());

    let stale_tab = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    assert_eq!(workspace.handle(Command::InvokeFolderItemsMenu { token: stale_tab.token, action: ActionId::OpenItem }.into()), Default::default());

    let activated = workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab }.into());
    let current_revision = activated
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::TabViewChanged { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    let destination = request(&mut workspace, tab, current_revision, ContextMenuKind::Comprehensive, FolderItemsMenuTarget::EmptySpace).unwrap();
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 1, name: name("second") }.into());
    let invoked = workspace.handle(Command::InvokeFolderItemsMenu { token: destination.token, action: ActionId::NewFolder }.into());
    assert!(invoked.outputs.iter().any(|output| matches!(output, Output::NameEditorOpened { .. })), "irrelevant selection changes do not stale a destination-only action");
}
