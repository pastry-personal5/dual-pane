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
}

impl ProbeJob {
    /// The event for a job that could not run; a failure proves nothing
    /// about its location.
    pub(super) fn fail(self) -> Event {
        match self {
            Self::Probe { event, .. } => event(FavoriteProbeOutcome::Failed),
            Self::Resolve { event, .. } => event(ResolvedTarget::Unavailable),
            Self::ClipboardPreflight { request, generation, .. } => Event::ClipboardPreflighted { request, generation, result: Err(ClipboardRejection::SourceUnavailable) },
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
        };
        deliver_event(&delivery, event);
    }
}
