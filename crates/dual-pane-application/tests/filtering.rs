use std::sync::Arc;

use dual_pane_application::{BrowserSnapshot, Command, Event, FilterFailure, FilterRequest, FilterStatus, Input, Output, PreviewView, SessionSnapshot, SettingsSnapshot, TabSnapshot, Transition, VisibilityPolicy, WindowFrame, WindowLayout, WindowState, WorkRequest, Workspace, WorkspaceSnapshot, project_filter};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, OperationKind, RequestToken, ScrollAnchor, SortDirection, SortField, SortSpec, TabId};

fn name(bytes: impl AsRef<[u8]>) -> EntryName {
    EntryName::new(bytes.as_ref().to_vec()).unwrap()
}
fn folder(text: &str) -> Location {
    Location::root().join(&name(text))
}
fn entries(names: &[&[u8]]) -> Arc<[Entry]> {
    names.iter().map(|bytes| Entry::new(name(bytes), EntryKind::File)).collect()
}

fn read(transition: &Transition) -> (BrowserSide, TabId, RequestToken) {
    transition
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ReadDirectory { browser, tab, token, .. } => Some((*browser, *tab, *token)),
            _ => None,
        })
        .unwrap()
}

fn loaded(workspace: &mut Workspace, browser: BrowserSide, location: Location, names: &[&[u8]]) -> TabId {
    let transition = workspace.handle(Command::Navigate { browser, location }.into());
    let (_, tab, token) = read(&transition);
    workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries(names), changes: None }.into());
    tab
}

fn filter(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, text: &str) -> (FilterRequest, u64) {
    let transition = workspace.handle(Command::SetFilterText { browser, tab, text: text.into() }.into());
    assert!(!transition.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { .. })));
    let request = transition
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    let visible_revision = transition
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::VisibleItemsChanged { entries, filter_status: FilterStatus::Updating, visible_revision, .. } => {
                assert!(entries.is_empty());
                Some(*visible_revision)
            }
            _ => None,
        })
        .unwrap();
    (request, visible_revision)
}

fn finish(workspace: &mut Workspace, request: FilterRequest) -> Transition {
    let result = project_filter(&request);
    workspace.handle(Event::FilterProjected { request, result: Ok(result) }.into())
}

fn observed(browser: BrowserSide, tab: TabId, visible_revision: u64, command: Command) -> Input {
    Input::ObservedCommand { command, browser, tab, visible_revision }
}

#[test]
fn unicode_and_invalid_bytes_keep_exact_row_identities() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &["Café".as_bytes(), "cafe\u{301}".as_bytes(), "Straße".as_bytes(), b"bad\xff", b"bad\xfe"]);
    let (request, revision) = filter(&mut workspace, BrowserSide::Left, tab, "CAFE");
    let output = finish(&mut workspace, request);
    assert!(matches!(&output.outputs[0], Output::VisibleItemsChanged { entries, total_count: 5, filter_status: FilterStatus::Ready, .. } if entries.len() == 2));
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "strasse");
    assert_eq!(project_filter(&request).indices.as_ref(), &[2]);
    assert!(finish(&mut workspace, request).outputs.iter().any(|output| matches!(output, Output::VisibleItemsChanged { entries, .. } if entries.len() == 1)));
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "�");
    let result = project_filter(&request);
    assert_eq!(result.indices.as_ref(), &[3, 4]);
    assert_ne!(result.entries[0].name(), result.entries[1].name());
    let output = workspace.handle(Event::FilterProjected { request, result: Ok(result) }.into());
    let Output::VisibleItemsChanged { visible_revision, .. } = &output.outputs[0] else { panic!("visible output") };
    assert!(workspace.handle(observed(BrowserSide::Left, tab, revision, Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name(b"bad\xff") })).outputs.is_empty());
    workspace.handle(observed(BrowserSide::Left, tab, *visible_revision, Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name(b"bad\xff") }));
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name(b"bad\xff")]);
}

#[test]
fn pending_failure_retry_and_clear_preserve_only_visible_selection() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &[b"alpha", b"beta"]);
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "a");
    let ready = finish(&mut workspace, request);
    let Output::VisibleItemsChanged { visible_revision, .. } = &ready.outputs[0] else { panic!("visible output") };
    workspace.handle(observed(BrowserSide::Left, tab, *visible_revision, Command::SelectAll { browser: BrowserSide::Left, tab }));
    assert_eq!(workspace.selection(BrowserSide::Left).entries().len(), 2);
    let (request, pending_revision) = filter(&mut workspace, BrowserSide::Left, tab, "alp");
    assert_eq!(workspace.selection(BrowserSide::Left).entries().len(), 2, "temporary blank rows keep visit state");
    assert!(workspace.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash).is_err());
    assert!(workspace.handle(observed(BrowserSide::Left, tab, pending_revision, Command::SelectAll { browser: BrowserSide::Left, tab })).outputs.is_empty());
    let failed = workspace.handle(Event::FilterProjected { request: request.clone(), result: Err(FilterFailure::WorkerUnavailable) }.into());
    assert!(matches!(&failed.outputs[0], Output::VisibleItemsChanged { entries, filter_status: FilterStatus::Failed, .. } if entries.is_empty()));
    let (retry, _) = filter(&mut workspace, BrowserSide::Left, tab, "alp");
    let ready = finish(&mut workspace, retry);
    assert!(matches!(&ready.outputs[0], Output::VisibleItemsChanged { entries, selection, .. } if entries.len() == 1 && selection.entries() == [name("alpha")]));
    let intent = workspace.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash).unwrap();
    assert_eq!(intent.targets().iter().map(|target| target.name.clone()).collect::<Vec<_>>(), vec![name("alpha")]);
    let clear = workspace.handle(Command::SetFilterText { browser: BrowserSide::Left, tab, text: String::new() }.into());
    assert!(matches!(&clear.outputs[0], Output::VisibleItemsChanged { entries, selection, filter_status: FilterStatus::Ready, .. } if entries.len() == 2 && selection.entries() == [name("alpha")]));
}

#[test]
fn stale_projection_and_same_named_row_gesture_cannot_replace_new_view() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &[b"alpha", b"beta"]);
    let (old, old_revision) = filter(&mut workspace, BrowserSide::Left, tab, "a");
    let (current, pending_revision) = filter(&mut workspace, BrowserSide::Left, tab, "al");
    assert!(finish(&mut workspace, old).outputs.is_empty());
    let ready = finish(&mut workspace, current);
    assert!(matches!(&ready.outputs[0], Output::VisibleItemsChanged { entries, .. } if entries.len() == 1));
    let Output::VisibleItemsChanged { visible_revision: current_revision, .. } = &ready.outputs[0] else { panic!("visible output") };
    let command = Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("alpha") };
    assert!(workspace.handle(observed(BrowserSide::Left, tab, old_revision, command.clone())).outputs.is_empty());
    assert!(workspace.handle(observed(BrowserSide::Left, tab, pending_revision, command.clone())).outputs.is_empty());
    assert!(workspace.handle(command.clone().into()).outputs.is_empty(), "unstamped filtered gesture is refused");
    assert!(matches!(workspace.handle(observed(BrowserSide::Left, tab, *current_revision, command)).outputs.as_slice(), [Output::SelectionChanged { .. }]));
}

#[test]
fn projection_completion_must_reference_the_authoritative_listing_and_mapping() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &[b"alpha", b"beta"]);
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "alp");

    let mut foreign_request = request.clone();
    foreign_request.entries = entries(&[b"alpha", b"beta"]);
    let foreign_result = project_filter(&foreign_request);
    assert!(workspace.handle(Event::FilterProjected { request: foreign_request, result: Ok(foreign_result) }.into()).outputs.is_empty());

    let mut invalid = project_filter(&request);
    invalid.indices = Arc::from([1]);
    let failed = workspace.handle(Event::FilterProjected { request, result: Ok(invalid) }.into());
    assert!(matches!(&failed.outputs[0], Output::VisibleItemsChanged { entries, filter_status: FilterStatus::Failed, .. } if entries.is_empty()));
}

#[test]
fn observed_wrapper_rejects_commands_without_a_visible_target() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &[b"alpha"]);
    let wrapped = observed(BrowserSide::Left, tab, 1, Command::Refresh { browser: BrowserSide::Left, tab });
    assert!(workspace.handle(wrapped).work.is_empty());
}

#[test]
fn refresh_keeps_filter_navigation_clears_it_and_failed_navigation_keeps_it() {
    let mut workspace = Workspace::new();
    let folder_a = folder("a");
    let folder_b = folder("b");
    let tab = loaded(&mut workspace, BrowserSide::Left, folder_a.clone(), &[b"alpha", b"beta"]);
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "alp");
    finish(&mut workspace, request);
    let refresh = workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into());
    let (_, _, token) = read(&refresh);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"alpha", b"alpine", b"beta"]), changes: None }.into());
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::VisibleItemsChanged { filter_status: FilterStatus::Updating, entries, .. } if entries.is_empty())));
    let request = loaded
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert!(matches!(&finish(&mut workspace, request).outputs[0], Output::VisibleItemsChanged { entries, total_count: 3, .. } if entries.len() == 2));
    let navigation = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder_b.clone() }.into());
    let (_, _, failed_token) = read(&navigation);
    workspace.handle(Event::FolderItemsFailed { browser: BrowserSide::Left, tab, token: failed_token, kind: dual_pane_domain::ListingErrorKind::ItemMissing }.into());
    assert_eq!(workspace.entries(BrowserSide::Left).len(), 3);
    let retry = workspace.handle(Command::Navigate { browser: BrowserSide::Left, location: folder_b }.into());
    let (_, _, token) = read(&retry);
    let arrived = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"other"]), changes: None }.into());
    assert!(matches!(&arrived.outputs[0], Output::FolderItemsLoaded { entries, .. } if entries.len() == 1));
    let back = workspace.handle(Command::GoBack { browser: BrowserSide::Left, tab }.into());
    let (_, _, token) = read(&back);
    let returned = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"alpha", b"beta"]), changes: None }.into());
    assert!(matches!(&returned.outputs[0], Output::FolderItemsLoaded { entries, .. } if entries.len() == 2));
    let forward = workspace.handle(Command::GoForward { browser: BrowserSide::Left, tab }.into());
    let (_, _, token) = read(&forward);
    let forward_loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"other"]), changes: None }.into());
    assert!(matches!(&forward_loaded.outputs[0], Output::FolderItemsLoaded { entries, .. } if entries.len() == 1));
}

#[test]
fn hidden_scroll_uses_nearest_surviving_source_row_without_selecting_it() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("a"), &[b"a", b"b", b"c", b"d", b"e"]);
    let revision = match workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into()).work.iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })) {
        Some(_) => 1,
        None => panic!("read"),
    };
    workspace.handle(observed(BrowserSide::Left, tab, revision, Command::UpdateScrollHint { browser: BrowserSide::Left, tab, row: Some(2), scroll: Some(ScrollAnchor::new(name("c"), 11)) }));
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "e");
    let result = finish(&mut workspace, request);
    assert!(matches!(&result.outputs[0], Output::VisibleItemsChanged { scroll_hint: Some(scroll), selection, .. } if scroll.entry() == &name("e") && scroll.offset() == 0 && selection.entries().is_empty()));
}

#[test]
fn tabs_and_browsers_keep_independent_transient_queries() {
    let mut workspace = Workspace::new();
    let left = loaded(&mut workspace, BrowserSide::Left, folder("shared"), &[b"alpha", b"beta"]);
    let right = loaded(&mut workspace, BrowserSide::Right, folder("shared"), &[b"alpha", b"beta"]);
    let (left_request, _) = filter(&mut workspace, BrowserSide::Left, left, "alp");
    let (right_request, _) = filter(&mut workspace, BrowserSide::Right, right, "bet");
    finish(&mut workspace, left_request);
    finish(&mut workspace, right_request);
    let new_tab = workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    let (_, second, token) = read(&new_tab);
    let second_loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab: second, token, entries: entries(&[b"alpha", b"beta"]), changes: None }.into());
    assert!(matches!(&second_loaded.outputs[0], Output::FolderItemsLoaded { entries, .. } if entries.len() == 2));
    let first_view = workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab: left }.into());
    assert!(first_view.outputs.iter().any(|output| matches!(output, Output::TabViewChanged { filter_text, entries, .. } if filter_text == "alp" && entries.len() == 1)));
    let right_view = workspace.handle(Command::ActivateTab { browser: BrowserSide::Right, tab: right }.into());
    assert!(right_view.outputs.is_empty(), "the right Browser was already on its tab");
    let left_snapshot = WorkspaceSnapshot { left: BrowserSnapshot { tabs: vec![TabSnapshot { location: folder("shared") }, TabSnapshot { location: folder("shared") }], active_tab: Some(0) }, right: BrowserSnapshot { tabs: vec![TabSnapshot { location: folder("shared") }], active_tab: Some(0) }, active_browser: BrowserSide::Left, layout: WindowLayout { frame: WindowFrame { x: 1, y: 2, width: 800, height: 600 }, state: WindowState::Normal, sidebar_splitter: 200, browser_splitter: 400, left_preview_height: 0, right_preview_height: 0 } };
    let mut restored = Workspace::new();
    let result = restored.handle(Event::SettingsLoadedWithSession { snapshot: SettingsSnapshot::default(), session: SessionSnapshot::Saved(left_snapshot) }.into());
    assert!(result.work.iter().any(|work| matches!(work, WorkRequest::ReadDirectory { .. })));
    assert!(restored.tabs(BrowserSide::Left).count() >= 1);
    assert!(restored.tabs(BrowserSide::Right).count() >= 1);
    for work in result.work {
        if let WorkRequest::ReadDirectory { browser, tab, token, .. } = work {
            let loaded = restored.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries(&[b"alpha", b"beta"]), changes: None }.into());
            assert!(loaded.outputs.iter().any(|output| matches!(output, Output::FolderItemsLoaded { entries, .. } if entries.len() == 2)));
        }
    }
}

#[test]
fn closed_tab_and_replaced_listing_reject_delayed_projection() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("same"), &[b"alpha", b"beta"]);
    let (old, _) = filter(&mut workspace, BrowserSide::Left, tab, "alp");
    let refresh = workspace.handle(Command::Refresh { browser: BrowserSide::Left, tab }.into());
    let (_, _, token) = read(&refresh);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"alpha", b"alpine"]), changes: None }.into());
    let current = loaded
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert!(finish(&mut workspace, old).outputs.is_empty());
    assert!(matches!(&finish(&mut workspace, current).outputs[0], Output::VisibleItemsChanged { entries, .. } if entries.len() == 2));
    let (closing, _) = filter(&mut workspace, BrowserSide::Left, tab, "alpha");
    workspace.handle(Command::CloseTab { browser: BrowserSide::Left, tab }.into());
    assert!(finish(&mut workspace, closing).outputs.is_empty());
}

#[test]
fn range_anchor_and_select_all_use_only_surviving_visible_rows() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("items"), &[b"alpha", b"beta", b"gamma"]);
    workspace.handle(observed(BrowserSide::Left, tab, 1, Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("alpha") }));
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "bet");
    let ready = finish(&mut workspace, request);
    let Output::VisibleItemsChanged { visible_revision, selection, .. } = &ready.outputs[0] else { panic!("visible output") };
    assert!(selection.entries().is_empty());
    assert!(workspace.handle(observed(BrowserSide::Left, tab, *visible_revision, Command::SelectRange { browser: BrowserSide::Left, tab, row: 0, name: name("beta") })).outputs.is_empty(), "the hidden anchor was cleared");
    workspace.handle(observed(BrowserSide::Left, tab, *visible_revision, Command::SelectAll { browser: BrowserSide::Left, tab }));
    assert_eq!(workspace.selection(BrowserSide::Left).entries(), &[name("beta")]);
}

#[test]
fn stricter_visibility_clears_filtered_rows_before_the_replacement_read() {
    let mut workspace = Workspace::new();
    workspace.handle(Event::SettingsLoaded { snapshot: SettingsSnapshot::default() }.into());
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("items"), &[b".hidden", b"keep"]);
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, ".");
    finish(&mut workspace, request);
    let stricter = VisibilityPolicy { show_dotfiles: false, ..VisibilityPolicy::default() };
    let change = workspace.handle(Command::SetVisibilityPolicy { visibility: stricter }.into());
    assert!(change.outputs.iter().any(|output| matches!(output, Output::TabViewChanged { entries, filter_text, .. } if entries.is_empty() && filter_text == ".")));
    let (_, _, token) = read(&change);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"keep"]), changes: None }.into());
    assert!(loaded.outputs.iter().any(|output| matches!(output, Output::VisibleItemsChanged { entries, filter_status: FilterStatus::Updating, .. } if entries.is_empty())));
    let request = loaded
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert!(matches!(&finish(&mut workspace, request).outputs[0], Output::VisibleItemsChanged { entries, total_count: 1, .. } if entries.is_empty()));
    assert!(workspace.operation_availability(BrowserSide::Left, tab, OperationKind::MoveToTrash).is_err());
}

#[test]
fn sort_and_watcher_refresh_reproject_in_the_new_sorted_order() {
    let mut workspace = Workspace::new();
    let location = folder("items");
    let tab = loaded(&mut workspace, BrowserSide::Left, location.clone(), &[b"alpha", b"beta"]);
    let (request, _) = filter(&mut workspace, BrowserSide::Left, tab, "a");
    finish(&mut workspace, request);
    let sort = SortSpec::new(SortField::Name, SortDirection::Descending);
    let transition = workspace.handle(Command::SetSort { browser: BrowserSide::Left, tab, location: location.clone(), sort }.into());
    let (_, _, token) = read(&transition);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"beta", b"alpha"]), changes: None }.into());
    let request = loaded
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.text, "a");
    assert!(matches!(&finish(&mut workspace, request).outputs[0], Output::VisibleItemsChanged { entries, .. } if entries[0].name() == &name("beta")));
    let watch = workspace.handle(Event::LocationInvalidated { location }.into());
    let (_, _, token) = read(&watch);
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser: BrowserSide::Left, tab, token, entries: entries(&[b"gamma", b"beta", b"alpha"]), changes: None }.into());
    let request = loaded
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert!(matches!(&finish(&mut workspace, request).outputs[0], Output::VisibleItemsChanged { entries, filter_text, .. } if entries.len() == 3 && filter_text == "a"));
}

#[test]
fn preview_target_clears_when_its_selected_item_is_hidden() {
    let mut workspace = Workspace::new();
    let tab = loaded(&mut workspace, BrowserSide::Left, folder("items"), &[b"alpha", b"beta"]);
    workspace.handle(Command::SetPreviewTarget { browser: BrowserSide::Left, tab, width: 200, height: 200 }.into());
    let selected = workspace.handle(observed(BrowserSide::Left, tab, 1, Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("alpha") }));
    assert!(selected.work.iter().any(|work| matches!(work, WorkRequest::ReadPreview { .. })));
    let pending = workspace.handle(Command::SetFilterText { browser: BrowserSide::Left, tab, text: "bet".into() }.into());
    assert!(pending.outputs.iter().any(|output| matches!(output, Output::PreviewChanged { browser: BrowserSide::Left, view: PreviewView::NoSelection })));
    let request = pending
        .work
        .iter()
        .find_map(|work| match work {
            WorkRequest::ProjectFilter { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    finish(&mut workspace, request);
    assert!(workspace.selection(BrowserSide::Left).entries().is_empty());
}
