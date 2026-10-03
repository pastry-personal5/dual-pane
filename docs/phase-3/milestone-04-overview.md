# P3-M4: Decide workspace interactions

Status: Done

This documentation milestone records the owner's Phase 3 interaction decisions for tabs, Favorites, file operations, session restoration, sort controls, Notices, and automatic refresh. Its output is the agreed behavior in the active product and UX documents, with edge cases traced to P3-M5 through P3-M10.

## Scope and fixed inputs

In scope: user gestures and command availability; which Browser and tab an action affects; visible loading, decision, error, and completion states; keyboard focus and accessibility; and the interaction consequences of asynchronous work. Cover pointer and keyboard access for new controls without assuming every command needs a shortcut. The owner kept tab reordering, Favorite Item movement, and Operation Panel docking as drag-only actions and the Favorite Item Context Menu as pointer-only; other new controls need keyboard access. Retain the existing [input model](../ux-gui.md#input-and-active-browser-state) unless an approved decision explicitly changes it.

Use the existing [product safeguards](../product-behavior.md#file-operation-safeguards), [session recovery](../product-behavior.md#session-recovery), [P3-M2 persistence contract](milestone-02-architecture.md), [P3-M3 layout](milestone-03-architecture.md), and [architecture boundaries](../architecture.md#3-boundary-ownership) as fixed inputs. The owner-approved field order, direct sort buttons, responsive hiding, folder-sort memory, Favorite hierarchy, and Folder Pane summary rules already constrain the new design. New or changed product and interaction choices require an owner decision before they become canonical; record those decisions in the [phase changelog](changelog.md).

Out of scope: implementing or testing application, adapter, or desktop behavior; changing P3-M2's completed storage evidence or persisted action IDs; the shortcut editor; and selecting native APIs, worker capacity, watcher technology, sort collation, or session-save timing. Text filtering is removed from Phase 3 behavior and restored session state. Later implementation owns compatibility with existing storage records and action IDs.

## Decision sources and consumers

Each rule lives in one active source: [product behavior](../product-behavior.md) for scope, safeguards, Favorites identity, and recovery; [GUI rules](../ux-gui.md#planned-phase-3-gui-decisions) for gestures, controls, panels, Notices, and refresh feedback; and [information architecture](../ux-information-architecture.md#planned-phase-3-containment) for containment. [UX terms](../ux-terms.md) supplies exact names. The [architecture handoff](milestone-04-architecture.md#contract-to-later-milestones) assigns P3-M5 through P3-M10 their consumers.

| Decision area | Canonical rule | Consumers |
|---|---|---|
| Tab creation, closure, labels, ordering, history, view state, and keyboard exit | [Browser Tabs Strip and navigation](../ux-gui.md#browser-tabs-strip-and-navigation) | P3-M5, P3-M6, P3-M9 |
| Favorite target, uniqueness, alias meaning, deletion, and launch pruning | [Favorites](../product-behavior.md#favorites) | P3-M5, P3-M6, P3-M9 |
| Favorite controls, editing, drag movement, and activation | [Sidebar Favorites](../ux-gui.md#sidebar-favorites) | P3-M5, P3-M6 |
| Multi-selection, command availability, frozen operation targets, and link activation | [Selection and file commands](../ux-gui.md#selection-and-file-commands) | P3-M5, P3-M6, P3-M7, P3-M8 |
| Conflict, collision, merge, error, and destructive-operation safeguards | [File-operation safeguards](../product-behavior.md#file-operation-safeguards) | P3-M7, P3-M8 |
| Concurrent Operation Panels and Decision Cards, cancellation, terminal feedback, and quick-action Notices | [Operation Panels and decisions](../ux-gui.md#operation-panels-and-decisions) | P3-M7, P3-M8 |
| Missing saved tabs, fallback tab, Notices startup policy, and session-only history | [Session recovery](../product-behavior.md#session-recovery) | P3-M9 |
| Notices controls and focus | [Notices and restoration feedback](../ux-gui.md#notices-and-restoration-feedback) | P3-M6, P3-M8, P3-M9 |
| Sort memory, controls, and no Phase 3 text filtering | [Included product scope](../product-behavior.md#included-product-scope), [sort controls](../ux-gui.md#sort-controls-and-remembered-choice) | P3-M5, P3-M6, P3-M9 |
| Inactive-tab refresh, watched-location loss, operation-triggered updates, and active last-successful feedback | [Automatic refresh](../ux-gui.md#automatic-refresh) | P3-M5, P3-M6, P3-M8, P3-M10 |

## Scenario trace

| Starting state | Gesture or event | Affected Browser/tab | State and visible result | Consumer |
|---|---|---|---|---|
| Left Browser active; focus enters any Right Browser control | Keyboard focus moves into its Browser Tabs Strip, Folder Pane control, or Folder Items List | Right Browser and its active tab | Right Browser becomes active before a Browser-specific command; selection and scroll remain unchanged | P3-M5, P3-M6 |
| Only one tab remains | `Command+W` on the workspace | Active Browser | Close that tab and replace it with a clean Home tab | P3-M5, P3-M6 |
| Several tabs; middle tab active | Close Tab Button on that tab | Its Browser | Remove it; nearest right tab becomes active, otherwise left | P3-M5, P3-M6 |
| Tab navigated Back | Navigate into another folder | That tab | Discard Forward branch and show the new folder | P3-M5, P3-M6 |
| Favorite Group contains Items | Delete from Favorite Group Menu | Sidebar collection | Group and Items are removed without confirmation | P3-M5, P3-M6 |
| Favorite Item belongs to a group | Drag to another group or position | Sidebar collection | Move or reorder the Item; its target remains the same | P3-M5, P3-M6 |
| Tab has no range anchor, even if `Command+A` selected Items | Shift-click or Shift-arrow | That Browser Tab | No selection or cursor change; a later plain click or unmodified movement selecting a row establishes the anchor | P3-M5, P3-M6 |
| Several Folder Items selected | Command-click, Shift-click, or right-click | Active tab | Toggle, extend, retain, or replace selection according to the planned gesture | P3-M5, P3-M6 |
| Range selected; cursor has moved | Plain arrow key | Active tab | Collapse to one selected Item; the fixed range anchor no longer extends the selection | P3-M5, P3-M6 |
| Both active tabs show the same folder | Request Copy or Move | Source and other Browser | Both commands are disabled; no job starts | P3-M7, P3-M8 |
| Other Browser folder is pending or unavailable | Request Copy or Move | Source and other Browser | Both commands are disabled until that destination is confirmed available | P3-M7, P3-M8 |
| Copy or Move has started | Switch tab or selection | Captured source and other Browser destination | Job keeps the starting selection and destination | P3-M7, P3-M8 |
| Permanent Delete confirmation is open | Switch tab or selection | Captured source tab and frozen targets | Confirmation still shows the original target count; Cancel remains default | P3-M7, P3-M8 |
| Rename or New Folder name collides | Commit the inline editor | Source folder | Editor stays open with an error; existing Item is not overwritten | P3-M7, P3-M8 |
| Two jobs encounter collisions | Two conflict results | Each job | Separate Operation Decision Cards; conflict-only apply-to-all starts unchecked | P3-M7, P3-M8 |
| Job is running in a panel | Close source tab, cancel job, or encounter a partial result | Job and workspace | Tab closure does not cancel; cancellation preserves completed work; partial result remains for review | P3-M7, P3-M8 |
| Running Operation Panel has focus | `Command+W` | Focused panel | No action; job and panel stay open | P3-M8 |
| Completed Operation Panel has focus | `Command+W` | Focused panel | Close that panel | P3-M8 |
| Saved active tab is missing | Restore session | Its Browser | Discard it and activate nearest surviving right tab, then left | P3-M9 |
| No saved tabs survive | Restore session | Affected Browser | Create one clean Home tab ([changed in P3-M9 planning](changelog.md#2026-10-03--p3-m9-planning)); report discarded tabs in Notices | P3-M9 |
| Routine notices are suppressed | Launch with routine and actionable storage notices | Notices auxiliary window | Suppress routine startup opening; actionable storage error opens Notices | P3-M9 |
| Notices has focus | `Command+W` | Notices auxiliary window | Close Notices; Main Toolbar Notices Button can reopen it | P3-M6, P3-M9 |
| Inactive tab's folder changes | Watch invalidation | That tab | Refresh in background; when active, retain last successful Folder Items with a status cue during refresh | P3-M5, P3-M6, P3-M10 |
| Watched folder disappears | Watch invalidation | That tab | Keep last successful Items and path with an error; retry on `Command+R` or a later event | P3-M5, P3-M6, P3-M10 |
| An operation changes a displayed folder | Operation completion | Every tab showing that folder | Refresh active and inactive tabs in both Browsers; drop missing selections and clear a missing cursor | P3-M5, P3-M8, P3-M10 |

The owner approved a roughly three-second successful-panel timeout, whitespace-only name rejection, and `Command+W` closing a completed Operation Panel. The owner also chose drag-only reordering and docking despite the otherwise keyboard-accessible new controls.

## Completion checklist

- [x] Record owner decisions for all new or changed interaction rules in the phase changelog, then put each rule in exactly one active product or UX source. Keep current 2.0.0 behavior and planned Phase 3 behavior visibly distinct. Evidence: [decision log](changelog.md#2026-10-01--p3-m4-workspace-interview-decisions) and [decision-source map](#decision-sources-and-consumers).
- [x] Browser Tabs Strip, history, activation, and last-tab behavior are specified, including the planned `Command+W` cases and repeated tab labels; plain Tab retains its current reservation. Evidence: [Browser Tabs Strip and navigation](../ux-gui.md#browser-tabs-strip-and-navigation) and [focus-entry scenario](#scenario-trace).
- [x] Sidebar Favorite creation, editing, ordering, activation, exact-text uniqueness, and deletion are specified without altering the one-level hierarchy or launch-removal policy. Evidence: [Favorite policy](../product-behavior.md#favorites) and [Sidebar controls](../ux-gui.md#sidebar-favorites).
- [x] Selection and cursor gestures, multi-selection scope, command targeting, and the effect of changing tabs or selections during an operation are specified. Evidence: [selection and file commands](../ux-gui.md#selection-and-file-commands) and [no-anchor scenario](#scenario-trace).
- [x] Every committed file operation and default-app opening has its decided entry point, target and availability rule, and success/failure presentation; long operations have progress and cancellation behavior, and required decisions preserve every [file-operation safeguard](../product-behavior.md#file-operation-safeguards). Evidence: [selection and file commands](../ux-gui.md#selection-and-file-commands), [Operation Panels and decisions](../ux-gui.md#operation-panels-and-decisions), and [scenario trace](#scenario-trace).
- [x] Startup restoration, discarded-tab handling, fallback tab creation, active tab choice, Notices and storage-error feedback, and the separate Favorites validation path are specified without restoring tab history. Evidence: [session recovery](../product-behavior.md#session-recovery), [Favorites probing](../product-behavior.md#favorites), and [Notices controls](../ux-gui.md#notices-and-restoration-feedback).
- [x] Sort behavior retains location-shared memory, direct accessible controls, narrow-Browser behavior, and last-successful summary treatment; Phase 3 text filtering is removed from behavior and restored state. Evidence: [scope](../product-behavior.md#included-product-scope), [sort controls](../ux-gui.md#sort-controls-and-remembered-choice), [narrow behavior](../ux-gui.md#narrow-browser-behavior), and [P3-M3 summary](milestone-03-architecture.md#deferred-summary-and-controls).
- [x] Refresh behavior specifies visible and hidden tabs, stale-result precedence, failure feedback, watched-location loss, operation-triggered updates, and safe selection/cursor retention. Evidence: [automatic refresh](../ux-gui.md#automatic-refresh) and [scenario trace](#scenario-trace).
- [x] A scenario table traces the interview decisions through startup, navigation, tab switching, operation decisions, cancellation, refresh, and failure and names the P3-M5–P3-M10 consumer. Evidence: [scenario trace](#scenario-trace) and [consumer map](#decision-sources-and-consumers).
- [x] Cross-links and terminology are reviewed; the documentation contains no conflict with existing product safeguards, P3-M2 storage, P3-M3 layout, or the current input model. Evidence: 170 nonarchived-doc local links with zero issues, stale-claim search, `git diff --check`, and passing `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` on 2026-10-01.
