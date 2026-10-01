# P3-M4 architecture

Status: Done

P3-M4 is a documentation milestone. It translates committed product policy and earlier Phase 3 decisions into a reviewed interaction contract for the state, UI, operation, persistence, and watcher milestones.

## Sources and decision method

[Product behavior](../product-behavior.md) owns product scope, file-operation safeguards, Favorites policy, and session recovery. [UX GUI](../ux-gui.md) owns gestures, focus, control behavior, accessibility, and visible states. [UX terms](../ux-terms.md) owns component names; [UX information architecture](../ux-information-architecture.md) owns containment. Update the latter two only if a decided interaction requires new vocabulary or structure. The milestone overview lists questions and acceptance evidence; it does not become another behavior source.

The [owner interview decisions](changelog.md#2026-10-01--p3-m4-workspace-interview-decisions) are recorded in the canonical sources before feature implementation. The current 2.0.0 gesture table remains separate from planned Phase 3 behavior. P3-M2's completed filter storage evidence is historical; the current product scope removes text filtering from Phase 3 behavior and restored state. Existing code and persisted action IDs remain unchanged in this documentation milestone; later implementations must reconcile compatibility without reusing or silently changing those IDs.

Use one scenario table as review evidence, with columns for starting state, gesture or external event, affected Browser/tab, state change, visible result, and implementing milestone. Include sequences where a newer request wins over a slow earlier result and where an operation is waiting for a person's decision. This table may live in the active UX document if it is itself normative; otherwise it is acceptance evidence in the P3-M4 overview and links to the normative rules.

## Contract to later milestones

The agreed interactions should describe commands, events, state, and output at the application boundary without fixing Rust type names, Qt signals, worker topology, or native calls. An input controller translates gestures; application state decides validity and ordering; a presenter renders choices and status. The runtime reports facts and performs requested work, and it never waits for a dialog.

| Consumer | Required handoff |
|---|---|
| P3-M5 | Tab and history transitions; Browser/tab activation; Favorites edit rules; selection and cursor rules; location-shared sort; request-token and refresh-retention behavior. |
| P3-M6 | Browser Tabs Strip, Sidebar, sort and summary controls; focus order and accessibility; command enablement; loading and error states. Add planned action IDs without reusing existing persisted IDs, and reconcile `CloseWindow`/`CloseTab` bindings with the new `Command+W` focus behavior. |
| P3-M7 | Selection-to-intent mapping, frozen source/destination context, operation availability, decision and cancellation lifecycle, and the exact permitted responses for each policy case. |
| P3-M8 | Operation command entry points, independent Operation Panels and Operation Decision Cards, open-file behavior, Notices summaries, terminal feedback, and affected-listing refresh. |
| P3-M9 | Startup and fallback sequence, valid-tab restoration, active Browser/tab choice, Notices startup preference and session-only history, and visible settings/session errors. Keep earlier stored filter fields readable during migration without restoring a Phase 3 filter. |
| P3-M10 | Watch invalidation and refresh feedback, visible/hidden tab behavior, precedence over stale reads, and retained selection/cursor after a completed listing. |

## Boundaries to preserve

- Each Browser has at least one tab and exactly one active tab. Closing the last tab creates a clean Home tab. Tab history is in memory during a session and is absent from the saved workspace. The saved session records tab order, active tabs, current locations, and the active Browser; text filtering is absent from the Phase 3 restoration contract.
- Folder-sort memory is one byte-exact-location choice shared across Browsers and tabs, with natural Name ascending as the default and the P3-M2 100-location LRU.
- Folder Pane summary values describe the last successfully visible Folder Items; totals are non-recursive sums of known direct visible sizes and use `—` when incomplete. Pending or failed navigation does not replace the displayed summary with a different location's data.
- Source and destination observations can change while a file operation runs. The application owns the accepted intent and pending decision; the executor revalidates before mutation. Conflict choices, directory merge, replacement, Trash, permanent-delete confirmation, and cancellation follow the existing product and architecture rules.
- Asynchronous results affect state only when their tokens still match the relevant tab, location, sort request, or operation. Reads, target probes, persistence, watches, and file operations stay off the GUI thread. Inactive tabs refresh in the background; an active refresh keeps the last successful Folder Items with a status cue. The current UI remains usable while these run.

## Verification and sequence

1. Trace the interview decisions to [product behavior](../product-behavior.md), [GUI rules](../ux-gui.md#planned-phase-3-gui-decisions), and [planned containment](../ux-information-architecture.md#planned-phase-3-containment).
2. Review the [scenario trace](milestone-04-overview.md#scenario-trace) against P3-M5 through P3-M10 and leave genuinely unresolved interaction details open rather than attributing assumptions to the owner.
3. Run the repository gate, record evidence beside completed checklist items, and mark the milestone Done only when every checklist item is satisfied.
