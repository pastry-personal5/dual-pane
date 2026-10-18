use std::sync::Arc;

use dual_pane_application::{ClipboardAction, ClipboardGeneration, ClipboardRejection, ClipboardSnapshot, Command, Event, Input, OperationEffect, Output, PlannedItem, WorkRequest, Workspace, project_filter};
use dual_pane_domain::{BrowserSide, Entry, EntryKind, EntryName, Location, OperationKind, OperationRoot, TabId};

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}
fn path(parts: &[&str]) -> Location {
    parts.iter().fold(Location::root(), |at, part| at.join(&name(part)))
}

fn load(workspace: &mut Workspace, browser: BrowserSide, location: Location, entries: &[(&str, EntryKind)]) -> (TabId, u64) {
    let transition = workspace.handle(Command::Navigate { browser, location }.into());
    let WorkRequest::ReadDirectory { tab, token, .. } = transition.work.into_iter().find(|work| matches!(work, WorkRequest::ReadDirectory { .. })).unwrap() else { unreachable!() };
    let transition = workspace.handle(Event::FolderItemsLoaded { browser, tab, token, entries: entries.iter().map(|(text, kind)| Entry::new(name(text), *kind)).collect(), changes: None }.into());
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

fn capture(workspace: &mut Workspace, browser: BrowserSide, tab: TabId, revision: u64, action: ClipboardAction) -> (dual_pane_application::ClipboardRequestId, dual_pane_application::TransferId, Option<Arc<[OperationRoot]>>) {
    let transition = workspace.handle(Input::ObservedCommand { command: Command::CaptureClipboard { browser, tab, action }, browser, tab, visible_revision: revision });
    let WorkRequest::PublishClipboard { request, transfer, finder_roots, .. } = transition.work.into_iter().next().unwrap() else { panic!("publication") };
    (request, transfer, finder_roots)
}

fn publish(workspace: &mut Workspace, request: dual_pane_application::ClipboardRequestId, transfer: dual_pane_application::TransferId, generation: u64) {
    workspace.handle(Event::ClipboardPublished { request, transfer, generation: ClipboardGeneration::new(generation), succeeded: true }.into());
}

fn request_paste(workspace: &mut Workspace, browser: BrowserSide, tab: TabId) -> dual_pane_application::ClipboardRequestId {
    let transition = workspace.handle(Command::PasteClipboard { browser, tab }.into());
    let WorkRequest::ReadClipboard { request } = transition.work.into_iter().next().unwrap() else { panic!("clipboard read") };
    request
}

#[test]
fn capture_is_byte_exact_and_a_late_publication_cannot_replace_the_newer_cut() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("first", EntryKind::File), ("second", EntryKind::Directory)]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("first") }.into());
    let (old_request, old_transfer, old_roots) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Copy);
    assert_eq!(old_roots.unwrap()[0].source, path(&["source", "first"]));
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 1, name: name("second") }.into());
    let (new_request, new_transfer, new_roots) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    publish(&mut workspace, old_request, old_transfer, 1);
    publish(&mut workspace, new_request, new_transfer, 2);

    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let transition = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(2), snapshot: ClipboardSnapshot::Private { transfer: old_transfer } }.into());
    assert!(matches!(transition.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::UnknownPrivateTransfer, .. }]));
    assert!(new_roots.is_none(), "Cut exposes only its private reference");
}

#[test]
fn copy_repeats_with_frozen_destinations_while_cut_rejection_retries_and_terminal_consumes() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (publication, transfer, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Copy);
    publish(&mut workspace, publication, transfer, 7);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    for _ in 0..2 {
        let paste = request_paste(&mut workspace, BrowserSide::Right, right);
        let transition = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(7), snapshot: ClipboardSnapshot::Private { transfer } }.into());
        let WorkRequest::Operation(OperationEffect::Scan { intent, .. }) = transition.work.into_iter().next().unwrap() else { panic!("copy scan") };
        assert_eq!(intent.kind(), &OperationKind::Copy);
        assert_eq!(intent.destination(), Some(&path(&["destination"])));
    }

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Left }.into());
    let (publication, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    publish(&mut workspace, publication, cut, 8);
    let same = request_paste(&mut workspace, BrowserSide::Left, left);
    let rejected = workspace.handle(Event::ClipboardRead { request: same, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::SameSourceAndDestination, .. }]));
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let retry = request_paste(&mut workspace, BrowserSide::Right, right);
    let started = workspace.handle(Event::ClipboardRead { request: retry, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    let WorkRequest::Operation(OperationEffect::Scan { id, generation, .. }) = started.work.into_iter().next().unwrap() else { panic!("move scan") };
    let overlapping = request_paste(&mut workspace, BrowserSide::Right, right);
    let reserved = workspace.handle(Event::ClipboardRead { request: overlapping, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert!(matches!(reserved.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::TransferReserved, .. }]));
    workspace.handle(Event::OperationExecutorUnavailable { id, generation }.into());
    let again = request_paste(&mut workspace, BrowserSide::Right, right);
    let consumed = workspace.handle(Event::ClipboardRead { request: again, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert!(matches!(consumed.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::UnknownPrivateTransfer, .. }]));
}

#[test]
fn failed_publication_preserves_the_previous_clipboard_and_replacement_invalidates_it() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("first", EntryKind::File), ("second", EntryKind::File)]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("first") }.into());
    let (first_request, first_transfer, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Copy);
    publish(&mut workspace, first_request, first_transfer, 20);

    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 1, name: name("second") }.into());
    let (failed_request, failed_transfer, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Copy);
    let failed = workspace.handle(Event::ClipboardPublished { request: failed_request, transfer: failed_transfer, generation: ClipboardGeneration::new(21), succeeded: false }.into());
    assert!(matches!(failed.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::PublicationFailed, .. }]));

    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let previous = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(20), snapshot: ClipboardSnapshot::Private { transfer: first_transfer } }.into());
    assert!(matches!(previous.work.as_slice(), [WorkRequest::Operation(OperationEffect::Scan { .. })]));

    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(22) }.into());
    let replaced = request_paste(&mut workspace, BrowserSide::Right, right);
    let rejected = workspace.handle(Event::ClipboardRead { request: replaced, generation: ClipboardGeneration::new(22), snapshot: ClipboardSnapshot::Private { transfer: first_transfer } }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::UnknownPrivateTransfer, .. }]));
}

#[test]
fn finder_preflight_is_tagged_all_or_nothing_and_supports_mixed_parents() {
    let mut workspace = Workspace::new();
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let request = request_paste(&mut workspace, BrowserSide::Right, right);
    let sources: Arc<[Location]> = vec![path(&["one", "a"]), path(&["two", "b"])].into();
    let read = workspace.handle(Event::ClipboardRead { request, generation: ClipboardGeneration::new(3), snapshot: ClipboardSnapshot::ExternalFiles { sources: Arc::clone(&sources) } }.into());
    assert!(matches!(read.work.as_slice(), [WorkRequest::PreflightClipboard { request: found, generation, sources: found_sources }] if *found == request && *generation == ClipboardGeneration::new(3) && found_sources == &sources));
    let roots: Arc<[OperationRoot]> = vec![OperationRoot { source: sources[0].clone(), kind: EntryKind::File }, OperationRoot { source: sources[1].clone(), kind: EntryKind::Symlink { points_to_directory: false } }].into();
    let started = workspace.handle(Event::ClipboardPreflighted { request, generation: ClipboardGeneration::new(3), result: Ok(Arc::clone(&roots)) }.into());
    let WorkRequest::Operation(OperationEffect::Scan { id, generation, intent, .. }) = started.work.into_iter().next().unwrap() else { panic!("scan") };
    assert_eq!(intent.roots(), &*roots);
    assert_eq!(intent.destination_for(&sources[1]), Some(path(&["destination", "b"])));
    let plan: Arc<[PlannedItem]> = roots.iter().map(|root| PlannedItem { source: root.source.clone(), destination: intent.destination_for(&root.source), kind: root.kind }).collect();
    let accepted = workspace.handle(Event::OperationScanned { id, generation, plan: Ok(plan) }.into());
    assert!(matches!(accepted.work.as_slice(), [WorkRequest::Operation(OperationEffect::Execute { .. })]));
}

#[test]
fn replacement_cancels_a_pending_read_and_duplicate_external_roots_never_preflight() {
    let mut workspace = Workspace::new();
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let stale = request_paste(&mut workspace, BrowserSide::Right, right);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(5) }.into());
    assert_eq!(workspace.handle(Event::ClipboardRead { request: stale, generation: ClipboardGeneration::new(4), snapshot: ClipboardSnapshot::Unsupported }.into()), Default::default());
    let duplicate = request_paste(&mut workspace, BrowserSide::Right, right);
    let source = path(&["source", "same"]);
    let transition = workspace.handle(Event::ClipboardRead { request: duplicate, generation: ClipboardGeneration::new(5), snapshot: ClipboardSnapshot::ExternalFiles { sources: vec![source.clone(), source].into() } }.into());
    assert!(transition.work.is_empty());
    assert!(matches!(transition.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::DuplicateSource, .. }]));
}

#[test]
fn filtered_capture_uses_only_the_observed_ready_projection() {
    let mut workspace = Workspace::new();
    let (tab, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("alpha", EntryKind::File), ("beta", EntryKind::File)]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("alpha") }.into());
    let requested = workspace.handle(Command::SetFilterText { browser: BrowserSide::Left, tab, text: "bet".into() }.into());
    let WorkRequest::ProjectFilter { request } = requested.work.into_iter().next().unwrap() else { panic!("filter") };
    let result = project_filter(&request);
    let ready = workspace.handle(Event::FilterProjected { request, result: Ok(result) }.into());
    let visible_revision = ready
        .outputs
        .iter()
        .find_map(|output| match output {
            Output::VisibleItemsChanged { visible_revision, .. } => Some(*visible_revision),
            _ => None,
        })
        .unwrap();
    let hidden = workspace.handle(Input::ObservedCommand { command: Command::CaptureClipboard { browser: BrowserSide::Left, tab, action: ClipboardAction::Copy }, browser: BrowserSide::Left, tab, visible_revision });
    assert!(matches!(hidden.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::NoSelection, .. }]));
    workspace.handle(Input::ObservedCommand { command: Command::SelectEntry { browser: BrowserSide::Left, tab, row: 0, name: name("beta") }, browser: BrowserSide::Left, tab, visible_revision });
    let (_, _, roots) = capture(&mut workspace, BrowserSide::Left, tab, visible_revision, ClipboardAction::Copy);
    assert_eq!(roots.unwrap()[0].source, path(&["source", "beta"]));
    let stale = workspace.handle(Input::ObservedCommand { command: Command::CaptureClipboard { browser: BrowserSide::Left, tab, action: ClipboardAction::Copy }, browser: BrowserSide::Left, tab, visible_revision: revision });
    assert!(matches!(stale.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::StaleObservation, .. }]));
}

#[test]
fn an_inactive_tab_cannot_capture_or_supply_a_paste_destination() {
    let mut workspace = Workspace::new();
    let (old, revision) = load(&mut workspace, BrowserSide::Left, path(&["old"]), &[("item", EntryKind::File)]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: old, row: 0, name: name("item") }.into());
    workspace.handle(Command::NewTab { browser: BrowserSide::Left }.into());
    assert_eq!(workspace.clipboard_capture_availability(BrowserSide::Left, old), Err(ClipboardRejection::StaleTab));
    assert_eq!(workspace.paste_availability(BrowserSide::Left, old), Err(ClipboardRejection::StaleTab));
    let stale = workspace.handle(Input::ObservedCommand { command: Command::CaptureClipboard { browser: BrowserSide::Left, tab: old, action: ClipboardAction::Copy }, browser: BrowserSide::Left, tab: old, visible_revision: revision });
    assert!(matches!(stale.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::StaleObservation, .. }]));
}
