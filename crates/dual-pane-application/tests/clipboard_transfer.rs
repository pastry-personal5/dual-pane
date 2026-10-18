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
    let failed = workspace.handle(Event::ClipboardPublished { request: failed_request, transfer: failed_transfer, generation: ClipboardGeneration::new(20), succeeded: false }.into());
    assert!(matches!(failed.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::PublicationFailed, .. }]));

    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let previous = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(20), snapshot: ClipboardSnapshot::Private { transfer: first_transfer } }.into());
    assert!(matches!(previous.work.as_slice(), [WorkRequest::Operation(OperationEffect::Scan { .. })]));

    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(22), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    let replaced = request_paste(&mut workspace, BrowserSide::Right, right);
    let rejected = workspace.handle(Event::ClipboardRead { request: replaced, generation: ClipboardGeneration::new(22), snapshot: ClipboardSnapshot::Private { transfer: first_transfer } }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::UnknownPrivateTransfer, .. }]));
}

#[test]
fn unknown_private_token_falls_back_to_colocated_file_urls_but_private_only_rejects() {
    let mut workspace = Workspace::new();
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let source = path(&["finder", "item"]);
    let request = request_paste(&mut workspace, BrowserSide::Right, right);
    let fallback = workspace.handle(Event::ClipboardRead { request, generation: ClipboardGeneration::new(31), snapshot: ClipboardSnapshot::PrivateAndExternal { transfer: dual_pane_application::TransferId::new(999), sources: vec![source.clone()].into() } }.into());
    assert!(matches!(fallback.work.as_slice(), [WorkRequest::PreflightClipboard { sources, .. }] if sources.as_ref() == [source]));

    let request = request_paste(&mut workspace, BrowserSide::Right, right);
    let rejected = workspace.handle(Event::ClipboardRead { request, generation: ClipboardGeneration::new(31), snapshot: ClipboardSnapshot::Private { transfer: dual_pane_application::TransferId::new(999) } }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::UnknownPrivateTransfer, .. }]));
}

#[test]
fn pending_cut_projection_is_published_and_cleared_by_replacement() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, transfer, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    let published = workspace.handle(Event::ClipboardPublished { request, transfer, generation: ClipboardGeneration::new(40), succeeded: true }.into());
    assert!(published.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if pending_cut.len() == 1 && pending_cut[0].source == path(&["source", "item"]))));
    let replaced = workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(41), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    assert!(matches!(replaced.outputs.as_slice(), [Output::ClipboardPresentationChanged { pending_cut, .. }] if pending_cut.is_empty()));
}

#[test]
fn observing_our_own_write_before_the_lane_reports_it_keeps_the_publication() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(40), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    // The GUI thread can see the lane's write before the lane reports it.
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(41), offer: dual_pane_application::ClipboardOffer::Private }.into());
    publish(&mut workspace, request, cut, 41);

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let started = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(41), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert!(matches!(started.work.as_slice(), [WorkRequest::Operation(OperationEffect::Scan { .. })]));
}

#[test]
fn a_publication_older_than_an_observed_replacement_never_becomes_current() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(40), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(42), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    let late = workspace.handle(Event::ClipboardPublished { request, transfer: cut, generation: ClipboardGeneration::new(41), succeeded: true }.into());
    assert!(!late.outputs.iter().any(|output| matches!(output, Output::ClipboardPrepared { .. })));

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let rejected = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(42), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::UnknownPrivateTransfer, .. }]));
}

#[test]
fn a_late_failure_older_than_the_observed_generation_changes_nothing() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(40), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, copy, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Copy);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(42), offer: dual_pane_application::ClipboardOffer::Files }.into());
    let late = workspace.handle(Event::ClipboardPublished { request, transfer: copy, generation: ClipboardGeneration::new(41), succeeded: false }.into());
    assert!(late.outputs.iter().any(|output| matches!(output, Output::ClipboardRejected { reason: ClipboardRejection::PublicationFailed, .. })), "the failure is still reported");

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let finder_source = path(&["finder", "item"]);
    let started = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(42), snapshot: ClipboardSnapshot::ExternalFiles { sources: vec![finder_source].into() } }.into());
    assert!(matches!(started.work.as_slice(), [WorkRequest::PreflightClipboard { .. }]), "the observed generation did not move backwards");
}

#[test]
fn a_started_cut_keeps_its_cue_until_the_operation_reaches_a_terminal_result() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    publish(&mut workspace, request, cut, 8);

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let started = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert!(!started.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if pending_cut.is_empty())), "starting the Paste does not clear the cue");
    let WorkRequest::Operation(OperationEffect::Scan { id, generation, .. }) = started.work.into_iter().next().unwrap() else { panic!("move scan") };

    let finished = workspace.handle(Event::OperationExecutorUnavailable { id, generation }.into());
    assert!(finished.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if pending_cut.is_empty())), "terminal consumption clears the cue");
}

#[test]
fn replacing_the_clipboard_clears_a_running_cuts_cue() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    publish(&mut workspace, request, cut, 8);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let started = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    let WorkRequest::Operation(OperationEffect::Scan { id, generation, .. }) = started.work.into_iter().next().unwrap() else { panic!("move scan") };

    let replaced = workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(9), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
    assert!(replaced.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if pending_cut.is_empty())), "replacement clears the cue while the Cut still runs");

    let finished = workspace.handle(Event::OperationExecutorUnavailable { id, generation }.into());
    assert!(!finished.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if !pending_cut.is_empty())), "the move ending brings no cue back");
}

#[test]
fn a_newer_copy_clears_a_running_cuts_cue() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    publish(&mut workspace, request, cut, 8);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Left }.into());
    let (request, copy, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Copy);
    let prepared = workspace.handle(Event::ClipboardPublished { request, transfer: copy, generation: ClipboardGeneration::new(9), succeeded: true }.into());
    assert!(prepared.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if pending_cut.is_empty())), "a Copy replaces the running Cut's cue");
}

#[test]
fn an_unavailable_clipboard_worker_reports_busy_not_unsupported() {
    let mut workspace = Workspace::new();
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let rejected = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(1), snapshot: ClipboardSnapshot::Unavailable }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::ClipboardBusy, .. }]));
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
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(5), offer: dual_pane_application::ClipboardOffer::Unsupported }.into());
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

#[test]
fn a_read_that_finds_an_unreported_replacement_adopts_it_so_a_retry_succeeds() {
    let mut workspace = Workspace::new();
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(1), offer: dual_pane_application::ClipboardOffer::Files }.into());

    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let sources: Arc<[Location]> = Arc::from(vec![path(&["elsewhere", "file"])]);
    let rejected = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(2), snapshot: ClipboardSnapshot::ExternalFiles { sources: Arc::clone(&sources) } }.into());
    assert!(rejected.outputs.iter().any(|output| matches!(output, Output::ClipboardRejected { reason: ClipboardRejection::ClipboardReplaced, .. })));

    let retry = request_paste(&mut workspace, BrowserSide::Right, right);
    let started = workspace.handle(Event::ClipboardRead { request: retry, generation: ClipboardGeneration::new(2), snapshot: ClipboardSnapshot::ExternalFiles { sources } }.into());
    assert!(matches!(started.work.as_slice(), [WorkRequest::PreflightClipboard { .. }]), "the retry is no longer rejected as replaced");
}

#[test]
fn a_newer_cut_alone_holds_the_cue_while_an_earlier_cut_runs() {
    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("one", EntryKind::File), ("two", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("one") }.into());
    let (request, first, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    publish(&mut workspace, request, first, 8);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(8), snapshot: ClipboardSnapshot::Private { transfer: first } }.into());

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Left }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 1, name: name("two") }.into());
    let (request, second, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    let prepared = workspace.handle(Event::ClipboardPublished { request, transfer: second, generation: ClipboardGeneration::new(9), succeeded: true }.into());
    assert!(prepared.outputs.iter().any(|output| matches!(output, Output::ClipboardPresentationChanged { pending_cut, .. } if pending_cut.len() == 1 && pending_cut[0].source == path(&["source", "two"]))), "only the newer Cut is pending");
}

#[test]
fn a_private_only_offer_enables_paste_only_for_this_launchs_unreserved_current_transfer() {
    use dual_pane_application::ClipboardOffer;

    let mut workspace = Workspace::new();
    let (left, revision) = load(&mut workspace, BrowserSide::Left, path(&["source"]), &[("item", EntryKind::File)]);
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(3), offer: ClipboardOffer::Private }.into());
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Err(ClipboardRejection::UnknownPrivateTransfer), "a token left by an earlier launch cannot paste");
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(4), offer: ClipboardOffer::PrivateAndFiles }.into());
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Ok(()), "colocated file URLs may still paste as a Copy");

    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Left }.into());
    workspace.handle(Command::SelectEntry { browser: BrowserSide::Left, tab: left, row: 0, name: name("item") }.into());
    let (request, cut, _) = capture(&mut workspace, BrowserSide::Left, left, revision, ClipboardAction::Cut);
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(5), offer: ClipboardOffer::Private }.into());
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Ok(()), "an unreported publication may be the observed content");
    publish(&mut workspace, request, cut, 5);
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Ok(()));

    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let started = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(5), snapshot: ClipboardSnapshot::Private { transfer: cut } }.into());
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Err(ClipboardRejection::TransferReserved));
    let WorkRequest::Operation(OperationEffect::Scan { id, generation, .. }) = started.work.into_iter().next().unwrap() else { panic!("move scan") };
    workspace.handle(Event::OperationExecutorUnavailable { id, generation }.into());
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Err(ClipboardRejection::UnknownPrivateTransfer), "a consumed Cut cannot paste again");
}

#[test]
fn an_unavailable_read_reports_busy_even_with_a_stale_generation() {
    let mut workspace = Workspace::new();
    let (right, _) = load(&mut workspace, BrowserSide::Right, path(&["destination"]), &[]);
    workspace.handle(Command::ActivateBrowser { browser: BrowserSide::Right }.into());
    workspace.handle(Event::ClipboardChanged { generation: ClipboardGeneration::new(6), offer: dual_pane_application::ClipboardOffer::Files }.into());
    let paste = request_paste(&mut workspace, BrowserSide::Right, right);
    let rejected = workspace.handle(Event::ClipboardRead { request: paste, generation: ClipboardGeneration::new(7), snapshot: ClipboardSnapshot::Unavailable }.into());
    assert!(matches!(rejected.outputs.as_slice(), [Output::ClipboardRejected { reason: ClipboardRejection::ClipboardBusy, .. }]));
    assert_eq!(workspace.paste_availability(BrowserSide::Right, right), Ok(()), "an untaken read changes no observed state");
}
