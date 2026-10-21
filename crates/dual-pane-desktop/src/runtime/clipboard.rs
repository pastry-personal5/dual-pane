//! The clipboard capture lane.

use super::*;

pub(super) enum ClipboardJob {
    Publish { request: ClipboardRequestId, transfer: dual_pane_application::TransferId, action: ClipboardAction, roots: Option<Arc<[OperationRoot]>> },
    Read { request: ClipboardRequestId },
}

pub(super) fn run_clipboard(jobs: Receiver<ClipboardJob>, delivery: Delivery) {
    for job in jobs {
        let event = match job {
            ClipboardJob::Publish { request, transfer, action, roots } => {
                let outcome = catch_unwind(AssertUnwindSafe(|| clipboard_native::publish(transfer, action, roots.as_deref())));
                let (generation, succeeded) = outcome.unwrap_or_else(|_| (clipboard_native::generation_and_offer().0, false));
                Event::ClipboardPublished { request, transfer, generation, succeeded }
            }
            ClipboardJob::Read { request } => {
                let (generation, snapshot) = catch_unwind(AssertUnwindSafe(clipboard_native::read)).unwrap_or_else(|_| (clipboard_native::generation_and_offer().0, dual_pane_application::ClipboardSnapshot::Unsupported));
                Event::ClipboardRead { request, generation, snapshot }
            }
        };
        deliver_event(&delivery, event);
    }
}

pub(super) fn preflight_clipboard(sources: &[Location]) -> Result<Arc<[OperationRoot]>, ClipboardRejection> {
    let mut seen = std::collections::HashSet::new();
    if sources.is_empty() || sources.iter().any(|source| source.is_root() || !seen.insert(source)) {
        return Err(ClipboardRejection::MalformedExternalContent);
    }
    sources
        .iter()
        .map(|source| {
            let metadata = std::fs::symlink_metadata(path_from_location(source)).map_err(|_| ClipboardRejection::SourceUnavailable)?;
            let kind = match native_kind(metadata.file_type()) {
                NativeKind::Directory => EntryKind::Directory,
                NativeKind::File => EntryKind::File,
                NativeKind::Symlink => EntryKind::Symlink { points_to_directory: false },
                NativeKind::Other => return Err(ClipboardRejection::UnsupportedContent),
            };
            Ok(OperationRoot { source: source.clone(), kind })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::from)
}
