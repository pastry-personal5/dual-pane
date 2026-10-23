use std::sync::Arc;

use dual_pane_application::{ActionId, ApplicationCandidate, ApplicationIdentity, ClipboardAction, ClipboardGeneration, ClipboardOffer, Command, ContentTypeDescription, ContextMenuGroup, ContextMenuKind, ContextMenuPreferenceRejection, DefaultApplicationRequest, Event, FileAssociationLookup, FileAssociationRecord, FileAssociationRequest, FileAssociationRequestId, FileAssociationRequestPurpose, FileIdentity, FolderItemsMenuAction, FolderItemsMenuSnapshot, FolderItemsMenuTarget, FolderItemsMenuToken, Key, NoticeKind, OpenWithFile, OpenWithRequest, Output, ResolvedFileAssociation, SettingsSnapshot, Shortcut, WorkRequest, Workspace};
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

fn setup_with_settings(entries: Vec<Entry>) -> (Workspace, TabId, u64, TabId) {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
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
    snapshot.sections.iter().flat_map(|section| section.actions.iter().filter_map(FolderItemsMenuAction::static_action)).collect()
}

#[test]
fn row_requests_apply_secondary_selection_and_project_only_available_configured_actions() {
    let entries = vec![Entry::new(name("first"), EntryKind::File), Entry::new(name("second"), EntryKind::File)];
    let (mut workspace, tab, revision, _) = setup(entries);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("first") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab, row: 1, name: name("second") }.into());

    let selected = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 1, name: name("second") }).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("first"), name("second")], "a selected row preserves the selection");
    assert_eq!(actions(&selected), vec![ActionId::ConfigureOpenWith, ActionId::CopyItems, ActionId::CutItems, ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::MoveToTrash]);

    let unselected = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first") }).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("first"), name("second")], "another selected row also preserves the selection");
    assert_eq!(actions(&unselected), actions(&selected));

    workspace.handle(Command::ClearSelection { browser: BrowserSide::Left, tab }.into());
    let sole = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 1, name: name("second") }).unwrap();
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("second")]);
    assert_eq!(actions(&sole), vec![ActionId::OpenItem, ActionId::ConfigureOpenWith, ActionId::CopyItems, ActionId::CutItems, ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::MoveToTrash]);
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

    assert!(workspace.context_menu_preferences().visible(ContextMenuKind::Normal, ActionId::ConfigureOpenWith));
    assert!(workspace.context_menu_preferences().visible(ContextMenuKind::Comprehensive, ActionId::ConfigureOpenWith));
    let setup_hidden = workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::ConfigureOpenWith, visible: false }.into());
    assert!(setup_hidden.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { snapshot, .. } if !snapshot.context_menus.visible(ContextMenuKind::Normal, ActionId::ConfigureOpenWith) && snapshot.context_menus.visible(ContextMenuKind::Comprehensive, ActionId::ConfigureOpenWith))));
    workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::ConfigureOpenWith, visible: true }.into());

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
    assert_eq!(actions(&snapshot), dual_pane_application::CONTEXT_MENU_ACTIONS.into_iter().filter(|action| *action != ActionId::ConfigureOpenWith).collect::<Vec<_>>());
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
    let shortcut = |snapshot: &FolderItemsMenuSnapshot| {
        snapshot
            .sections
            .iter()
            .flat_map(|section| &section.actions)
            .find_map(|row| match row {
                FolderItemsMenuAction::Static { action: ActionId::OpenItem, shortcut } => Some(*shortcut),
                _ => None,
            })
            .flatten()
    };
    assert_eq!(shortcut(&first), Some(custom));

    workspace.handle(Command::SetActionShortcuts { action: ActionId::OpenItem, shortcuts: vec![] }.into());
    assert_eq!(shortcut(&first), Some(custom), "an open snapshot is immutable");
    let later = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item") }).unwrap();
    assert_eq!(shortcut(&later), None);
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
    assert_eq!(actions(&snapshot), vec![ActionId::OpenItem, ActionId::ConfigureOpenWith, ActionId::CopyItems, ActionId::CutItems, ActionId::CopyToOtherBrowser, ActionId::MoveToOtherBrowser, ActionId::MoveToTrash]);
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

#[test]
fn open_with_choices_are_independent_flat_rows_and_one_use() {
    let (mut workspace, tab, revision, _) = setup_with_settings(vec![Entry::new(name("item.txt"), EntryKind::File)]);
    let source = location("from").join(&name("item.txt"));
    let application = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
    let record = FileAssociationRecord { content_type: ContentTypeDescription { identifier: "public.plain-text".into(), localized_kind: "Plain Text".into(), preferred_extension: Some("txt".into()) }, candidates: vec![ApplicationCandidate { identity: application.clone(), display_name: "Editor".into(), location_hint: "/Applications".into() }], default_application: None };
    workspace.handle(Event::FileAssociationsResolved { request: FileAssociationRequest { id: FileAssociationRequestId(7), files: vec![source.clone()], content_types: vec![], purpose: FileAssociationRequestPurpose::Refresh }, outcome: FileAssociationLookup::Resolved { files: vec![ResolvedFileAssociation { source: source.clone(), target: source, source_identity: FileIdentity { device: 1, inode: 2 }, target_identity: FileIdentity { device: 1, inode: 2 }, record: record.clone() }], records: vec![record] } }.into());
    workspace.handle(Command::SetFileAssociationSelected { content_type: "public.plain-text".into(), kind: ContextMenuKind::Normal, application: application.clone(), selected: true }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("item.txt") }.into());

    let normal = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item.txt") }).unwrap();
    let dynamic = normal
        .sections
        .iter()
        .flat_map(|section| &section.actions)
        .find_map(|entry| match entry {
            FolderItemsMenuAction::OpenWith(action) => Some(action.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(dynamic.label, "Open with Editor");
    let comprehensive = request(&mut workspace, tab, revision, ContextMenuKind::Comprehensive, FolderItemsMenuTarget::Row { row: 0, name: name("item.txt") }).unwrap();
    assert!(!comprehensive.sections.iter().flat_map(|section| &section.actions).any(|entry| matches!(entry, FolderItemsMenuAction::OpenWith(_))), "Normal selection does not leak into Comprehensive");
    assert!(actions(&comprehensive).contains(&ActionId::ConfigureOpenWith));
    assert_eq!(workspace.handle(Command::InvokeOpenWith { menu: normal.token, token: dynamic.token }.into()), Default::default(), "a newer menu retires the old dynamic row");

    let normal = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item.txt") }).unwrap();
    let dynamic = normal
        .sections
        .iter()
        .flat_map(|section| &section.actions)
        .find_map(|entry| match entry {
            FolderItemsMenuAction::OpenWith(action) => Some(action.clone()),
            _ => None,
        })
        .unwrap();
    let invoked = workspace.handle(Command::InvokeOpenWith { menu: normal.token, token: dynamic.token }.into());
    assert!(matches!(invoked.work.as_slice(), [WorkRequest::OpenWithApplication { request }] if request.application == application && request.content_type == "public.plain-text" && request.files.iter().all(|file| file.source == file.target)));
    assert_eq!(workspace.handle(Command::InvokeOpenWith { menu: normal.token, token: dynamic.token }.into()), Default::default(), "a dynamic row is one-use");
}

#[test]
fn an_unavailable_saved_application_can_be_unselected() {
    let (mut workspace, _, _, _) = setup_with_settings(vec![Entry::new(name("item.txt"), EntryKind::File)]);
    let application = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
    let content_type = ContentTypeDescription { identifier: "public.plain-text".into(), localized_kind: "Plain Text".into(), preferred_extension: Some("txt".into()) };
    let available = FileAssociationRecord { content_type: content_type.clone(), candidates: vec![ApplicationCandidate { identity: application.clone(), display_name: "Editor".into(), location_hint: "/Applications".into() }], default_application: None };
    workspace.handle(Event::FileAssociationsResolved { request: FileAssociationRequest { id: FileAssociationRequestId(9), files: vec![], content_types: vec![content_type.identifier.clone()], purpose: FileAssociationRequestPurpose::Refresh }, outcome: FileAssociationLookup::Resolved { files: vec![], records: vec![available] } }.into());
    workspace.handle(Command::SetFileAssociationSelected { content_type: content_type.identifier.clone(), kind: ContextMenuKind::Normal, application: application.clone(), selected: true }.into());
    let unavailable = FileAssociationRecord { content_type: content_type.clone(), candidates: vec![], default_application: None };
    workspace.handle(Event::FileAssociationsResolved { request: FileAssociationRequest { id: FileAssociationRequestId(10), files: vec![], content_types: vec![content_type.identifier.clone()], purpose: FileAssociationRequestPurpose::Refresh }, outcome: FileAssociationLookup::Resolved { files: vec![], records: vec![unavailable] } }.into());
    let row = workspace.file_association_types().into_iter().find(|row| row.content_type.identifier == content_type.identifier).unwrap();
    assert!(row.candidates.iter().any(|candidate| candidate.identity == application && !candidate.available && candidate.normal));

    let changed = workspace.handle(Command::SetFileAssociationSelected { content_type: content_type.identifier.clone(), kind: ContextMenuKind::Normal, application: application.clone(), selected: false }.into());
    assert!(changed.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { .. })));
    let row = workspace.file_association_types().into_iter().find(|row| row.content_type.identifier == content_type.identifier).unwrap();
    assert!(!row.candidates.iter().any(|candidate| candidate.identity == application));
}

#[test]
fn asynchronous_association_effect_results_reach_feedback_and_refresh() {
    let (mut workspace, _, _, _) = setup_with_settings(vec![]);
    let source = location("from").join(&name("item.txt"));
    let application = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
    let open_request = OpenWithRequest { files: vec![OpenWithFile { source: source.clone(), target: source, source_identity: FileIdentity { device: 1, inode: 2 }, target_identity: FileIdentity { device: 1, inode: 2 } }], content_type: "public.plain-text".into(), application: application.clone() };

    workspace.handle(Event::OpenWithFinished { request: open_request, result: Err("The application refused the file.".into()) }.into());
    assert!(matches!(workspace.notices().last().map(|notice| &notice.kind), Some(NoticeKind::FileAssociationFailed { action, reason }) if action == "Open With" && reason == "The application refused the file."));

    let default_request = DefaultApplicationRequest { content_type: "public.plain-text".into(), application };
    let completed = workspace.handle(Event::DefaultApplicationFinished { request: default_request, result: Ok(()) }.into());
    assert!(matches!(completed.work.as_slice(), [WorkRequest::ResolveFileAssociations { request }] if request.content_types == ["public.plain-text"] && matches!(request.purpose, FileAssociationRequestPurpose::Refresh)));
}

#[test]
fn configure_open_with_uses_the_captured_selection_and_each_visibility_setting() {
    let (mut workspace, tab, revision, _) = setup_with_settings(vec![Entry::new(name("item.txt"), EntryKind::File)]);
    let normal = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item.txt") }).unwrap();
    let opened = workspace.handle(Command::InvokeFolderItemsMenu { token: normal.token, action: ActionId::ConfigureOpenWith }.into());
    assert!(matches!(opened.work.as_slice(), [WorkRequest::ResolveFileAssociations { request }] if request.files == [location("from").join(&name("item.txt"))] && matches!(request.purpose, FileAssociationRequestPurpose::FocusSettings)));

    workspace.handle(Command::SetContextMenuVisibility { kind: ContextMenuKind::Normal, action: ActionId::ConfigureOpenWith, visible: false }.into());
    let normal = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("item.txt") }).unwrap();
    assert!(!actions(&normal).contains(&ActionId::ConfigureOpenWith));
    let comprehensive = request(&mut workspace, tab, revision, ContextMenuKind::Comprehensive, FolderItemsMenuTarget::Row { row: 0, name: name("item.txt") }).unwrap();
    assert!(actions(&comprehensive).contains(&ActionId::ConfigureOpenWith));
}

#[test]
fn late_discovery_from_a_closed_folder_cannot_restore_its_type() {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    let folder = location("from");
    let (tab, token) = read(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder.clone() }.into()).work);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![Entry::new(name("item.txt"), EntryKind::File)]), changes: None }.into());
    let request = loaded
        .work
        .into_iter()
        .find_map(|work| match work {
            WorkRequest::ResolveFileAssociations { request } if matches!(request.purpose, FileAssociationRequestPurpose::Observe) => Some(request),
            _ => None,
        })
        .unwrap();

    let closed = workspace.handle(Command::CloseTab { browser: BrowserSide::Left, tab }.into());
    assert!(closed.outputs.iter().any(|output| matches!(output, Output::FileAssociationsChanged)));

    let source = folder.join(&name("item.txt"));
    let record = FileAssociationRecord { content_type: ContentTypeDescription { identifier: "public.plain-text".into(), localized_kind: "Plain Text".into(), preferred_extension: Some("txt".into()) }, candidates: vec![], default_application: None };
    workspace.handle(Event::FileAssociationsResolved { request, outcome: FileAssociationLookup::Resolved { files: vec![ResolvedFileAssociation { source: source.clone(), target: source, source_identity: FileIdentity { device: 1, inode: 1 }, target_identity: FileIdentity { device: 1, inode: 1 }, record: record.clone() }], records: vec![record] } }.into());
    assert!(workspace.file_association_types().is_empty(), "a result for a closed folder must be ignored rather than repopulating its type cache");
}

#[test]
fn late_discovery_cannot_restore_a_type_after_an_empty_listing_replaces_its_folder() {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    let folder = location("from");
    let (tab, token) = read(&workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder.clone() }.into()).work);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: Arc::from(vec![Entry::new(name("item.txt"), EntryKind::File)]), changes: None }.into());
    let request = loaded
        .work
        .into_iter()
        .find_map(|work| match work {
            WorkRequest::ResolveFileAssociations { request } if matches!(request.purpose, FileAssociationRequestPurpose::Observe) => Some(request),
            _ => None,
        })
        .unwrap();
    let (_, refresh) = read(&workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into()).work);
    workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token: refresh, entries: Arc::from(Vec::<Entry>::new()), changes: None }.into());

    let source = folder.join(&name("item.txt"));
    let record = FileAssociationRecord { content_type: ContentTypeDescription { identifier: "public.plain-text".into(), localized_kind: "Plain Text".into(), preferred_extension: Some("txt".into()) }, candidates: vec![], default_application: None };
    workspace.handle(Event::FileAssociationsResolved { request, outcome: FileAssociationLookup::Resolved { files: vec![ResolvedFileAssociation { source: source.clone(), target: source, source_identity: FileIdentity { device: 1, inode: 1 }, target_identity: FileIdentity { device: 1, inode: 1 }, record: record.clone() }], records: vec![record] } }.into());
    assert!(workspace.file_association_types().is_empty(), "an empty replacement listing must retire its earlier discovery request");
}

#[test]
fn saved_choices_remain_manageable_when_macos_no_longer_knows_the_type() {
    let application = ApplicationIdentity { bundle_identifier: "com.example.RemovedEditor".into(), bundle_url: "file:///Applications/RemovedEditor.app".into() };
    let mut snapshot = SettingsSnapshot::default();
    snapshot.file_associations.set_selected("com.example.removed-type", ContextMenuKind::Normal, application.clone(), true).unwrap();
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot }.into());

    let types = workspace.file_association_types();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].content_type.identifier, "com.example.removed-type");
    assert_eq!(types[0].content_type.localized_kind, "Unavailable file type");
    assert!(types[0].pinned);
    assert!(matches!(types[0].candidates.as_slice(), [candidate] if candidate.identity == application && candidate.normal && !candidate.available));
}

#[test]
fn a_single_moved_application_copy_keeps_its_saved_menu_choice() {
    let old = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Old/Editor.app".into() };
    let moved = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
    let mut snapshot = SettingsSnapshot::default();
    snapshot.file_associations.set_selected("public.plain-text", ContextMenuKind::Normal, old, true).unwrap();
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot }.into());
    let record = FileAssociationRecord { content_type: ContentTypeDescription { identifier: "public.plain-text".into(), localized_kind: "Plain Text".into(), preferred_extension: Some("txt".into()) }, candidates: vec![ApplicationCandidate { identity: moved.clone(), display_name: "Editor".into(), location_hint: "/Applications".into() }], default_application: None };

    let changed = workspace.handle(Event::FileAssociationsResolved { request: FileAssociationRequest { id: FileAssociationRequestId(23), files: vec![], content_types: vec!["public.plain-text".into()], purpose: FileAssociationRequestPurpose::Refresh }, outcome: FileAssociationLookup::Resolved { files: vec![], records: vec![record] } }.into());
    assert!(changed.work.iter().any(|work| matches!(work, WorkRequest::SaveSettings { snapshot, .. } if snapshot.file_associations.get("public.plain-text").is_some_and(|preferences| preferences.selected(ContextMenuKind::Normal, &moved)))));
    let row = workspace.file_association_types().pop().unwrap();
    assert!(matches!(row.candidates.as_slice(), [candidate] if candidate.identity == moved && candidate.available && candidate.normal));
}

#[test]
fn multi_file_open_with_requires_one_type_and_an_app_common_to_every_file() {
    let (mut workspace, tab, revision, _) = setup_with_settings(vec![Entry::new(name("first.txt"), EntryKind::File), Entry::new(name("second.txt"), EntryKind::File)]);
    let common = ApplicationIdentity { bundle_identifier: "com.example.Common".into(), bundle_url: "file:///Applications/Common.app".into() };
    let alternate = ApplicationIdentity { bundle_identifier: "com.example.Alternate".into(), bundle_url: "file:///Applications/Alternate.app".into() };
    let first_only = ApplicationIdentity { bundle_identifier: "com.example.First".into(), bundle_url: "file:///Applications/First.app".into() };
    let candidate = |identity: ApplicationIdentity, name: &str| ApplicationCandidate { identity, display_name: name.into(), location_hint: "/Applications".into() };
    let content_type = ContentTypeDescription { identifier: "public.plain-text".into(), localized_kind: "Plain Text".into(), preferred_extension: Some("txt".into()) };
    let first_record = FileAssociationRecord { content_type: content_type.clone(), candidates: vec![candidate(alternate.clone(), "Alternate"), candidate(first_only.clone(), "First"), candidate(common.clone(), "Common")], default_application: None };
    let second_record = FileAssociationRecord { content_type: content_type.clone(), candidates: vec![candidate(alternate.clone(), "Alternate"), candidate(common.clone(), "Common")], default_application: None };
    let ranked_record = FileAssociationRecord { content_type, candidates: vec![candidate(common.clone(), "Common"), candidate(alternate.clone(), "Alternate"), candidate(first_only.clone(), "First")], default_application: None };
    let first = location("from").join(&name("first.txt"));
    let second = location("from").join(&name("second.txt"));
    workspace.handle(Event::FileAssociationsResolved { request: FileAssociationRequest { id: FileAssociationRequestId(8), files: vec![first.clone(), second.clone()], content_types: vec![], purpose: FileAssociationRequestPurpose::Refresh }, outcome: FileAssociationLookup::Resolved { files: vec![ResolvedFileAssociation { source: first.clone(), target: first, source_identity: FileIdentity { device: 1, inode: 1 }, target_identity: FileIdentity { device: 1, inode: 1 }, record: first_record }, ResolvedFileAssociation { source: second.clone(), target: second, source_identity: FileIdentity { device: 1, inode: 2 }, target_identity: FileIdentity { device: 1, inode: 2 }, record: second_record }], records: vec![ranked_record] } }.into());
    for application in [common.clone(), alternate.clone(), first_only] {
        workspace.handle(Command::SetFileAssociationSelected { content_type: "public.plain-text".into(), kind: ContextMenuKind::Normal, application, selected: true }.into());
    }
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("first.txt") }.into());
    workspace.handle(Command::ToggleEntry { browser: BrowserSide::Left, tab, row: 1, name: name("second.txt") }.into());
    let menu = request(&mut workspace, tab, revision, ContextMenuKind::Normal, FolderItemsMenuTarget::Row { row: 0, name: name("first.txt") }).unwrap();
    let dynamic = menu
        .sections
        .iter()
        .flat_map(|section| &section.actions)
        .filter_map(|entry| match entry {
            FolderItemsMenuAction::OpenWith(action) => Some(action),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dynamic.len(), 2);
    assert_eq!(dynamic[0].application, common);
    assert_eq!(dynamic[1].application, alternate);
}
