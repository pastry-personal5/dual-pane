//! The location probe lane.

use super::*;

/// Short work for the probe lane and the application event that reports it.
pub(super) enum ProbeJob {
    Probe {
        location: Location,
        event: Box<dyn FnOnce(FavoriteProbeOutcome) -> Event + Send>,
    },
    /// Resolves a link for activation.
    Resolve {
        location: Location,
        event: Box<dyn FnOnce(ResolvedTarget) -> Event + Send>,
    },
    ClipboardPreflight {
        request: dual_pane_application::ClipboardRequestId,
        generation: dual_pane_application::ClipboardGeneration,
        sources: Arc<[Location]>,
    },
    DropPreflight {
        preflight: dual_pane_application::DropPreflight,
    },
}

impl ProbeJob {
    /// The event for a job that could not run; a failure proves nothing
    /// about its location.
    pub(super) fn fail(self) -> Event {
        match self {
            Self::Probe { event, .. } => event(FavoriteProbeOutcome::Failed),
            Self::Resolve { event, .. } => event(ResolvedTarget::Unavailable),
            Self::ClipboardPreflight { request, generation, .. } => Event::ClipboardPreflighted { request, generation, result: Err(ClipboardRejection::SourceUnavailable) },
            Self::DropPreflight { preflight } => Event::DropPreflighted { request: preflight.request, result: Err(dual_pane_application::DropRejection::DestinationUnavailable) },
        }
    }
}

/// Runs probes in order. A panicking probe reports a failure and the lane
/// continues, because a probe keeps no state between calls.
pub(super) fn run_probes(jobs: Receiver<ProbeJob>, probe: LocationProbe, delivery: Delivery) {
    for job in jobs {
        let event = match job {
            ProbeJob::Probe { location, event } => event(catch_unwind(AssertUnwindSafe(|| probe(&location))).unwrap_or(FavoriteProbeOutcome::Failed)),
            ProbeJob::Resolve { location, event } => event(catch_unwind(AssertUnwindSafe(|| resolve_item(&location))).unwrap_or(ResolvedTarget::Unavailable)),
            ProbeJob::ClipboardPreflight { request, generation, sources } => {
                let result = catch_unwind(AssertUnwindSafe(|| preflight_clipboard(&sources))).unwrap_or(Err(ClipboardRejection::SourceUnavailable));
                Event::ClipboardPreflighted { request, generation, result }
            }
            ProbeJob::DropPreflight { preflight } => {
                let request = preflight.request;
                let result = catch_unwind(AssertUnwindSafe(|| preflight_drop(&preflight))).unwrap_or(Err(dual_pane_application::DropRejection::DestinationUnavailable));
                Event::DropPreflighted { request, result }
            }
        };
        deliver_event(&delivery, event);
    }
}

pub(super) fn preflight_drop(preflight: &dual_pane_application::DropPreflight) -> Result<dual_pane_application::ResolvedDropDestination, dual_pane_application::DropRejection> {
    use dual_pane_application::{DropRejection, ResolvedDropDestination};
    use dual_pane_domain::{EntryKind, OperationRoot};
    use std::fs;

    let candidate_path = path_from_location(&preflight.candidate);
    let canonical_path = fs::canonicalize(&candidate_path).map_err(|_| DropRejection::DestinationUnavailable)?;
    let metadata = fs::metadata(&canonical_path).map_err(|_| DropRejection::DestinationUnavailable)?;
    if !metadata.is_dir() {
        return Err(DropRejection::UnsupportedTarget);
    }
    if crate::native_shell::is_bundle(&canonical_path) {
        return Err(DropRejection::UnsupportedTarget);
    }
    let canonical = crate::native_location::location_from_path(&canonical_path).ok_or(DropRejection::DestinationUnavailable)?;
    let canonical_roots = preflight
        .roots
        .iter()
        .map(|root| {
            let path = path_from_location(&root.source);
            let canonical_path = if matches!(root.kind, EntryKind::Symlink { .. }) {
                let parent = path.parent().and_then(|parent| fs::canonicalize(parent).ok()).ok_or(DropRejection::StaleSource)?;
                parent.join(path.file_name().ok_or(DropRejection::StaleSource)?)
            } else {
                fs::canonicalize(path).map_err(|_| DropRejection::StaleSource)?
            };
            let source = crate::native_location::location_from_path(&canonical_path).ok_or(DropRejection::StaleSource)?;
            Ok(OperationRoot { source, kind: root.kind })
        })
        .collect::<Result<Vec<_>, DropRejection>>()?;
    Ok(ResolvedDropDestination { canonical, canonical_roots: canonical_roots.into() })
}

#[cfg(test)]
mod drop_tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use dual_pane_application::{DropPreflight, DropRejection, DropRequestId};
    use dual_pane_domain::{EntryKind, OperationRoot};
    use tempfile::tempdir;

    use super::*;
    use crate::native_location::location_from_path;

    #[test]
    fn drop_preflight_canonicalizes_aliases_and_refuses_files_and_packages() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        fs::create_dir(root.join("source")).unwrap();
        fs::write(root.join("source/item"), b"").unwrap();
        fs::create_dir(root.join("destination")).unwrap();
        symlink("destination", root.join("destination-link")).unwrap();
        let roots = vec![OperationRoot { source: location_from_path(&root.join("source/item")).unwrap(), kind: EntryKind::File }].into();
        let resolved = preflight_drop(&DropPreflight { request: DropRequestId::new(1), candidate: location_from_path(&root.join("destination-link")).unwrap(), roots: Arc::clone(&roots) }).unwrap();
        assert_eq!(resolved.canonical, location_from_path(&root.join("destination")).unwrap());
        assert_eq!(resolved.canonical_roots.as_ref(), roots.as_ref());

        fs::write(root.join("file"), b"").unwrap();
        assert_eq!(preflight_drop(&DropPreflight { request: DropRequestId::new(2), candidate: location_from_path(&root.join("file")).unwrap(), roots: Arc::clone(&roots) }), Err(DropRejection::UnsupportedTarget));

        fs::create_dir_all(root.join("Tool.app/Contents/MacOS")).unwrap();
        fs::write(root.join("Tool.app/Contents/Info.plist"), br#"<?xml version="1.0"?><plist><dict><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#).unwrap();
        symlink("Tool.app", root.join("tool-link")).unwrap();
        for (request, candidate) in [(3, "Tool.app"), (4, "tool-link")] {
            assert_eq!(preflight_drop(&DropPreflight { request: DropRequestId::new(request), candidate: location_from_path(&root.join(candidate)).unwrap(), roots: Arc::clone(&roots) }), Err(DropRejection::UnsupportedTarget));
        }
    }
}
