use std::sync::Arc;

use dual_pane_application::{Command, DragId, DropAction, DropRejection, DropTarget, Event, Input, OperationEffect, Output, ResolvedDropDestination, WorkRequest, Workspace};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, OperationKind, OperationRoot, TabId};

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}
fn path(parts: &[&str]) -> Location {
    parts.iter().fold(Location::root(), |at, part| at.join(&name(part)))
}

fn load(workspace: &mut Workspace, browser: BrowserSide, location: Location, entries: Vec<Entry>) -> (TabId, u64) {
    let transition = workspace.handle(Command::Navigate { browser, location }.into());
    let WorkRequest::ReadDirectory { tab, token, .. } = transition.work.into_iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })).unwrap() else { unreachable!() };
    let loaded = workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries.into(), changes: None }.into());
    let revision = loaded
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::FolderItemsLoaded { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    (tab, revision)
}

fn begin(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, revision: u64) -> DragId {
    let transition = workspace.handle(Command::BeginDrag { browser, tab, visible_revision: revision }.into());
    transition
        .outputs
        .into_iter()
        .find_map(|output| match output {
            Output::DragStarted { drag, .. } => Some(drag),
            _ => None,
        })
        .unwrap()
}

#[test]
fn selection_is_frozen_in_listing_order_and_a_drop_token_is_one_use() {
    let mut workspace = Workspace::new();
    let source_entries = vec![Entry::new(name("a"), EntryKind::File), Entry::new(name("b"), EntryKind::Directory), Entry::new(name("c"), EntryKind::File)];
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), source_entries);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Input::ObservedCommand { command: Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 2, name: name("c") }, browser: BrowserSide::Left, tab: left, visible_revision: revision });
    workspace.handle(Input::ObservedCommand { command: Command::ToggleEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("a") }, browser: BrowserSide::Left, tab: left, visible_revision: revision });
    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Right, target: DropTarget::BrowserFolder { browser: BrowserSide::Right, tab: right }, action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::UnknownDrag) }]));
    workspace.handle(Input::ObservedCommand { command: Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 1, name: name("b") }, browser: BrowserSide::Left, tab: left, visible_revision: revision });
    let target = DropTarget::BrowserFolder { browser: BrowserSide::Right, tab: right };
    let preview = workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into());
    assert!(matches!(preview.outputs.as_slice(), [Output::DropPreviewed { preview: Ok(preview) }] if preview.count == 2));
    let accepted = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into());
    let WorkRequest::PreflightDrop { preflight } = accepted.work.into_iter().next().unwrap() else { panic!("drop preflight") };
    assert_eq!(preflight.roots.iter().map(|root| root.source.clone()).collect::<Vec<_>>(), [path(&["source", "a"]), path(&["source", "c"])]);
    let replay = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target, action: DropAction::Move }.into());
    assert!(matches!(replay.outputs.as_slice(), [Output::DropRejected { reason: DropRejection::UnknownDrag, .. }]));
}

#[test]
fn unsupported_source_can_be_carried_but_rejects_on_release() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("special"), EntryKind::Other)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("special") }.into());
    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let target = DropTarget::BrowserFolder { browser: BrowserSide::Right, tab: right };
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::UnsupportedSource) }]));
    let refused = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target, action: DropAction::Move }.into());
    assert!(refused.work.is_empty());
    assert!(matches!(refused.outputs.as_slice(), [Output::DropRejected { browser: BrowserSide::Right, reason: DropRejection::UnsupportedSource, .. }]));
}

#[test]
fn row_and_tab_targets_use_exact_identity_and_stale_views_reject() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("item"), EntryKind::File)]);
    let target_entries = vec![Entry::new(name("folder"), EntryKind::Directory), Entry::new(name("package"), EntryKind::Directory).with_package(true), Entry::new(name("file"), EntryKind::File)];
    let (right, target_revision) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), target_entries);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let folder = DropTarget::FolderRow { browser: BrowserSide::Right, tab: right, visible_revision: target_revision, name: name("folder") };
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: folder, action: DropAction::Copy }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Ok(_) }]));
    let package = DropTarget::FolderRow { browser: BrowserSide::Right, tab: right, visible_revision: target_revision, name: name("package") };
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: package, action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::UnsupportedTarget) }]));
    let stale = DropTarget::FolderRow { browser: BrowserSide::Right, tab: right, visible_revision: target_revision.wrapping_add(1), name: name("folder") };
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: stale, action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::StaleTarget) }]));
}

#[test]
fn canonical_preflight_revalidates_and_hands_the_frozen_transfer_to_operations() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("item"), EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let target = DropTarget::BrowserTab { browser: BrowserSide::Right, tab: right };
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Ok(preview) }] if preview.action == DropAction::Move));
    let accepted = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target, action: DropAction::Copy }.into());
    let WorkRequest::PreflightDrop { preflight } = accepted.work.into_iter().next().unwrap() else { panic!("drop preflight") };
    let roots: Arc<[OperationRoot]> = vec![OperationRoot { source: path(&["physical-source", "item"]), kind: EntryKind::File }].into();
    let started = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Ok(ResolvedDropDestination { canonical: path(&["physical-destination"]), canonical_roots: roots }) }.into());
    let WorkRequest::Operation(OperationEffect::Scan { intent, .. }) = started.work.into_iter().next().unwrap() else { panic!("operation scan") };
    assert_eq!(intent.kind(), &OperationKind::Copy);
    assert_eq!(intent.roots(), &[OperationRoot { source: path(&["source", "item"]), kind: EntryKind::File }]);
    assert_eq!(intent.destination(), Some(&path(&["physical-destination"])));
    assert!(started.outputs.iter().any(|output| matches!(output, Output::DropStarted { action: DropAction::Copy, .. })));

    let late = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Err(DropRejection::DestinationUnavailable) }.into());
    assert!(late.outputs.is_empty() && late.work.is_empty());
}

#[test]
fn cancellation_and_tab_lifetime_invalidate_the_exact_session() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("item"), EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let target = DropTarget::BrowserTab { browser: BrowserSide::Right, tab: right };

    let cancelled = begin(&mut workspace, BrowserSide::Left, left, revision);
    workspace.handle(Command::CancelDrag { drag: cancelled }.into());
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag: cancelled, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::UnknownDrag) }]));

    let deactivated = begin(&mut workspace, BrowserSide::Left, left, revision);
    workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag: deactivated, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::StaleSource) }]));

    let activated = workspace.handle(Command::ActivateTab { browser: BrowserSide::Left, tab: left }.into());
    let activated_revision = activated
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::TabViewChanged { browser: BrowserSide::Left, tab, visible_revision, .. } if *tab == left => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    let closed = begin(&mut workspace, BrowserSide::Left, left, activated_revision);
    workspace.handle(Command::CloseTab { browser: BrowserSide::Left, tab: left }.into());
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag: closed, source_browser: BrowserSide::Left, target, action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::StaleSource) }]));

    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("item"), EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let target = DropTarget::BrowserTab { browser: BrowserSide::Right, tab: right };
    workspace.handle(Command::NewTab { browser: BrowserSide::Right }.into());
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Ok(_) }]));
    workspace.handle(Command::CloseTab { browser: BrowserSide::Right, tab: right }.into());
    assert!(matches!(workspace.handle(Command::PreviewDrop { drag, source_browser: BrowserSide::Left, target, action: DropAction::Move }.into()).outputs.as_slice(), [Output::DropPreviewed { preview: Err(DropRejection::StaleTarget) }]));
}

#[test]
fn overlapping_destination_checks_complete_independently() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("item"), EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let target = DropTarget::BrowserFolder { browser: BrowserSide::Right, tab: right };

    let first_drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let first = workspace.handle(Command::AcceptDrop { drag: first_drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Copy }.into());
    let WorkRequest::PreflightDrop { preflight: first } = first.work.into_iter().next().unwrap() else { panic!("first preflight") };
    let second_drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let second = workspace.handle(Command::AcceptDrop { drag: second_drag, source_browser: BrowserSide::Left, target, action: DropAction::Copy }.into());
    let WorkRequest::PreflightDrop { preflight: second } = second.work.into_iter().next().unwrap() else { panic!("second preflight") };
    assert_ne!(first.request, second.request);

    for preflight in [second, first] {
        let completed = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Ok(ResolvedDropDestination { canonical: path(&["physical-destination"]), canonical_roots: preflight.roots }) }.into());
        assert!(completed.work.iter().any(|work| matches!(work, WorkRequest::Operation(OperationEffect::Scan { .. }))));
        assert!(completed.outputs.iter().any(|output| matches!(output, Output::DropStarted { action: DropAction::Copy, .. })));
    }
}

#[test]
fn destination_check_reports_which_side_became_stale() {
    for stale_source in [true, false] {
        let mut workspace = Workspace::new();
        let source_entries = vec![Entry::new(name("item"), EntryKind::File)];
        let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), source_entries.clone());
        let target_entries = vec![Entry::new(name("folder"), EntryKind::Directory)];
        let (right, target_revision) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), target_entries.clone());
        workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
        let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
        let target = DropTarget::FolderRow { browser: BrowserSide::Right, tab: right, visible_revision: target_revision, name: name("folder") };
        let accepted = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target, action: DropAction::Move }.into());
        let WorkRequest::PreflightDrop { preflight } = accepted.work.into_iter().next().unwrap() else { panic!("preflight") };

        let browser = if stale_source { BrowserSide::Left } else { BrowserSide::Right };
        let tab = if stale_source { left } else { right };
        let refreshed = workspace.handle(Command::Refresh { browser, tab }.into());
        let WorkRequest::ReadDirectory { token, .. } = refreshed.work.into_iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })).unwrap() else { unreachable!() };
        let entries = if stale_source { source_entries.clone() } else { target_entries };
        workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries.into(), changes: None }.into());

        let completed = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Ok(ResolvedDropDestination { canonical: path(&["physical-destination"]), canonical_roots: preflight.roots }) }.into());
        let expected = if stale_source { DropRejection::StaleSource } else { DropRejection::StaleTarget };
        assert!(matches!(completed.outputs.as_slice(), [Output::DropRejected { reason, .. }] if *reason == expected));
        assert!(completed.work.is_empty());
    }
}

#[test]
fn malformed_or_post_quit_destination_results_cannot_start_work() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), vec![Entry::new(name("item"), EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), vec![]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let target = DropTarget::BrowserFolder { browser: BrowserSide::Right, tab: right };

    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let accepted = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target: target.clone(), action: DropAction::Move }.into());
    let WorkRequest::PreflightDrop { preflight } = accepted.work.into_iter().next().unwrap() else { panic!("preflight") };
    let malformed = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Ok(ResolvedDropDestination { canonical: path(&["physical-destination"]), canonical_roots: Arc::from([]) }) }.into());
    assert!(matches!(malformed.outputs.as_slice(), [Output::DropRejected { reason: DropRejection::StaleSource, .. }]));
    assert!(malformed.work.is_empty());

    let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
    let accepted = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target, action: DropAction::Move }.into());
    let WorkRequest::PreflightDrop { preflight } = accepted.work.into_iter().next().unwrap() else { panic!("preflight") };
    assert!(matches!(workspace.handle(Command::Quit { confirmed: false }.into()).outputs.as_slice(), [Output::QuitAccepted]));
    let late = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Ok(ResolvedDropDestination { canonical: path(&["physical-destination"]), canonical_roots: preflight.roots }) }.into());
    assert!(late.outputs.is_empty() && late.work.is_empty());
}

#[test]
fn physical_aliases_reject_same_folder_and_descendant_destinations() {
    for (kind, canonical_source, canonical_destination, expected) in [(EntryKind::File, path(&["physical-source", "item"]), path(&["physical-source"]), DropRejection::SameSourceAndDestination), (EntryKind::Directory, path(&["physical-source", "item"]), path(&["physical-source", "item", "inside"]), DropRejection::DestinationWithinSource)] {
        let mut workspace = Workspace::new();
        let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source-alias"]), vec![Entry::new(name("item"), kind)]);
        let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination-alias"]), vec![]);
        workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
        let drag = begin(&mut workspace, BrowserSide::Left, left, revision);
        let accepted = workspace.handle(Command::AcceptDrop { drag, source_browser: BrowserSide::Left, target: DropTarget::BrowserFolder { browser: BrowserSide::Right, tab: right }, action: DropAction::Move }.into());
        let WorkRequest::PreflightDrop { preflight } = accepted.work.into_iter().next().unwrap() else { panic!("preflight") };
        let roots = vec![OperationRoot { source: canonical_source, kind }].into();
        let completed = workspace.handle(Event::DropPreflighted { request: preflight.request, result: Ok(ResolvedDropDestination { canonical: canonical_destination, canonical_roots: roots }) }.into());
        assert!(matches!(completed.outputs.as_slice(), [Output::DropRejected { reason, .. }] if *reason == expected));
        assert!(completed.work.is_empty());
    }
}
