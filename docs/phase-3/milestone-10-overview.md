# P3-M10: Watch open locations

Status: Planned

This milestone keeps open Browser Tabs current when their folders change outside Dual Pane. It watches active and inactive tabs, coalesces notifications into bounded refresh work, and keeps matching selection, cursor, and scroll state. The [architecture plan](milestone-10-architecture.md) defines the application and native boundaries; this checklist defines completion.

## Scope and fixed inputs

Use the [Phase 3 exit criteria](phase-3.md#exit-criteria), [automatic-refresh interaction](../ux-gui.md#automatic-refresh), [watcher and threading boundaries](../architecture.md#34-frameworks-and-drivers--replaceable-mechanics), [responsive execution rules](../architecture.md#51-threading-and-responsiveness), and [remote-volume watch policy](../architecture.md#53-planned-mounted-remote-volume-model). P3-M5 already models selection-preserving reloads, and P3-M8 already refreshes folders affected by its own operations. P3-M11 must be Done before P3-M10 implementation starts, as the phase plan requires.

In scope: shared watches for the logical locations shown or loading in any open tab; local native invalidations; missing, renamed, inaccessible, disconnected, and later restored folders; watcher lifecycle across navigation, tab closure, session restoration, and Reset Settings; bounded fallback checks where native notifications cannot be used; foreground return reconciliation; active-tab Status Bar feedback when monitoring degrades; and tests and native verification.

Out of scope: changing the manual `Command+R` entry point; adding a refresh button or preference; changing saved session content; following a renamed folder to a new path; recursive Folder Items display; a new dependency beyond the approved dynamic CoreServices framework link; and implementing unrelated file-operation capability policy from the broader planned remote-volume model.

## Decisions recorded 2026-10-04

| Topic | Decision |
|---|---|
| Read storms | An invalidation during an in-flight read marks the tab dirty. That read finishes; one follow-up read covers all intervening invalidations. A person's newer navigation, sort, or manual refresh retains precedence. |
| App activity | Native watches remain registered while the app is hidden or unfocused. Fallback checks and watch-triggered listing work pause; returning to the app reconciles every open location. |
| Inactive fallback tabs | While the app is active, tabs without usable native notifications receive sparse background checks rather than waiting for activation. Visible tabs check more often. |
| Monitoring failure | A readable listing remains usable. Its active Browser shows a Status Bar cue when native monitoring falls back to periodic checks or automatic checking is unavailable; a failed listing keeps its existing, higher-priority error feedback. |
| Native integration | The owner approved dynamically linking Apple's CoreServices framework for FSEvents. No third-party crate or build tool is added. Byte-exact paths that the native stream cannot represent use the fallback path. |

## Completion checklist

- [ ] Qt-free application state derives one desired watch per distinct byte-exact location shown or loading in an open tab, including inactive tabs in both Browsers. Navigation, closure, restoration, discard, final-tab replacement, and Reset Settings start and stop watches without leaks; stale watch callbacks cannot affect a later subscription.
- [ ] `LocationInvalidated` coalesces per pending tab instead of cancelling its read. Completion starts at most one follow-up read for changes observed during that read; newer navigation, sort, or manual refresh wins. Repeated events cannot prevent a listing from completing, and restoration reads retain their existing discard rule.
- [ ] A dedicated, bounded desktop watcher service registers local FSEvents without blocking the GUI thread, delivers byte-exact location invalidations through the existing serialized input boundary, and handles root changes, dropped events, service failure, and shutdown. Watch setup cannot leave a gap between the initial listing and monitoring.
- [ ] Locations without usable native notifications receive worker-based checks with jitter and bounded backoff. Visible tabs check more often than inactive tabs; no fallback checks or watch-triggered reads run while the app is hidden or unfocused. Returning to the app causes a full reconciliation. A failed or missing folder is retried without navigating to a parent.
- [ ] The active Browser Status Bar distinguishes native monitoring, periodic checking, and unavailable automatic checking without replacing a listing error or changing `Command+R`. A successful refresh retains matching selection, cursor, and scroll state; an unsuccessful one retains the last successful rows and path.
- [ ] Deterministic application and runtime tests cover shared locations, initial-watch races, navigation and closure, session restoration, invalidation storms, stale results, missing and returned folders, fallback scheduling, foreground return, and Status Bar precedence. File-system fixtures use temporary directories only.
- [ ] A native macOS check covers external create, rename, delete, and metadata changes in active and inactive tabs; folder loss and return; foreground return; degraded monitoring feedback; and selection and cursor retention. Record the result here.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` plus `make check` pass. Update the directly affected design and user-facing docs in the implementation change, record evidence here, then mark P3-M10 Done in the phase plan.

## Acceptance evidence

Planning completed 2026-10-04. The documentation-only change passed `git diff --check`, `make check`, and the direct Cargo format, Clippy, and test commands. All implementation and milestone-verification items remain unchecked.
