# P3-M5 architecture

Status: Done

P3-M5 replaces the current Browser-wide, single-selection listing state with a Qt-free tabbed Workspace. The application remains a pure reducer; native directory reads and settings writes remain outside it. The [overview checklist](milestone-05-overview.md#completion-checklist) defines acceptance.

## Ownership and affected modules

| Ring | Planned change |
|---|---|
| Domain | Add Browser and Tab invariants, stable tab identity, per-history-entry view state, multi-selection/cursor/range anchor, one-level Favorites rules, and sort ordering policy. Extend Folder Item values with the optional direct metadata needed for sort and summary. Keep exact `EntryName` and logical `Location` identity. |
| Application | Evolve `Workspace` into the sole owner of two Browsers, settings-backed sort memory, Favorites, pending reads, and retained listings. Add typed tab, selection, Favorites, sort, and refresh commands; tab-addressed read results and immutable outputs. Reuse P3-M2 records rather than creating another persistence format. |
| Adapters | Translate UI-neutral gestures and application outputs for the revised boundary. Preserve rendering-only caches; no adapter decides whether an edit, selection, or result is valid. |
| Desktop | Extend the existing directory worker to gather required non-recursive metadata, sort off the GUI thread, and route request/results by tab and token. Make only the compatibility changes needed for the current one-visible-tab-per-Browser shell; P3-M6 builds the new widgets and focus behavior. |

## Tabs, history, and view state

Each Browser owns an ordered, nonempty sequence of Tabs and exactly one active tab ID. IDs are not list indexes, so reordering and closure cannot retarget a pending result. A new tab starts at the active tab's confirmed current folder with clean selection, cursor, anchor, and history; it becomes active. Closing a nonfinal tab activates the nearest right tab, then left. Closing the final tab replaces it with a clean Home tab at the user's home location, supplied by startup context without domain I/O. Tab activation changes the active Browser only when the command or focus-entry input targets that Browser.

Each Tab keeps a current confirmed history position and per-position view state. Navigation, Back, Forward, Up, and Favorite activation request a read for the target; commit the history transition and shown location only when that read succeeds. A failed or cancelled read keeps the confirmed position and last successful Folder Items. Back and Forward reread the destination and restore only view-state entries still present; a successful new navigation after Back truncates Forward. History is never included in the P3-M9 session snapshot. A separate requested target and loading/error state prevent a pending read from pretending the old listing belongs to the target location.

Selection is a set of exact Folder Item identities, not display rows. Cursor and fixed range anchor are separate, allowing a Command-click to leave the cursor on a deselected row and Command+A to leave a missing anchor missing. Gesture commands carry a row plus the exact observed identity or a validated movement intent; the reducer checks it against the tab's current listing before changing state. An explicit `UpdateScrollHint`-style input stores an entry anchor and relative position, never a Qt widget handle. P3-M6 translates that hint to actual scroll coordinates.

## Favorites and settings

The domain hierarchy holds ordered groups and their ordered items. Application commands enforce exact-text uniqueness among group names and within each group's aliases, reject empty or whitespace-only names, and default a newly created Item alias to the active folder's current name. They preserve stable IDs and byte-exact targets, and reject stale or invalid edit targets without saving. Moving an item changes only group membership and order. Deleting a group explicitly removes its children in one edit. Empty Favorites remain initialized, so they are not reseeded. Target probes are external facts keyed by the item's stable ID and observed target: a completed unavailable result may remove only the still-matching item under the product policy, whereas a failed or cancelled probe cannot. P3-M6 supplies the launch probes and Sidebar events.

The Workspace incorporates the existing `SettingsState` as its single in-memory source for folder-sort and Favorites choices. Accepted Favorites edits request a prompt ordered save through the P3-M2 settings service; sort choices use its coalesced save path. A failed save leaves the effective in-memory edit and produces a recoverable error instead of claiming durability. Stored Favorites IDs and order survive round trips through the existing records. Persisted action IDs stay unchanged; the old filter field remains a P3-M9 compatibility concern and is absent from this tab model.

## Reads, sorting, and refresh

Every directory read carries Browser side, stable tab ID, request token, logical location, effective `SortSpec`, and the previous immutable Folder Items needed for a worker-computed row delta. The worker reads direct entries and relevant metadata, uses a link's own metadata, sorts folders and folder links first, and places unknown metadata last. Name ascending retains the existing natural order. Type, Date, and Size order by their selected field and direction, then break ties with natural Name ascending in either direction. The completed result carries its request identity and listing changes; the reducer applies it only if the tab still exists and its pending token, location, and sort agree. A new navigation, sort, or refresh supersedes that tab's earlier request. Closing a tab cancels its outstanding read.

The current runtime reserves one pending slot per Browser, which cannot service inactive-tab refresh independently. Replace that assumption with bounded read admission keyed by stable tab ID. Its capacity and worker implementation remain runtime choices, but an accepted read reserves a terminal delivery, rejected reads produce a typed retryable saturation result, and the GUI thread never waits for queue space. Cancellation, worker panic containment, coalesced wake-up, and terminal delivery retain the [desktop execution guarantees](../architecture.md#52-planned-desktop-execution-model).

Changing a location's sort updates the P3-M2 LRU memory and issues fresh reads for every open tab at that byte-exact location, including tabs in the other Browser. Explicit manual refresh or a later affected-location event uses the same path and refreshes inactive tabs too. The active tab retains its last successful listing and summary while waiting. A successful same-location result intersects selected identities with new entries, clears a missing cursor or anchor, and preserves the scroll hint where its entry still exists. A different-location success restores the destination history position's matching state. Failures keep the last successful view and path, attach error feedback to the attempted read, and permit a later refresh. Result batches include row deltas consistent with the new immutable listing; P3-M6 applies them to the Qt model.

Directory metadata collection, sorting, and diffing stay on workers. The reducer performs only bounded state transitions and identity reconciliation on the GUI owner. If a metadata field is unavailable, it remains explicitly unknown; the application does not launch a recursive size scan or block to fill it. This follows the [threading contract](../architecture.md#51-threading-and-responsiveness) and keeps P3-M10 watcher technology outside this milestone.

## Sequence and verification

1. Add domain tab, selection, Favorites, and sort invariants with focused Qt-free tests.
2. Migrate the application input/output/work boundary and settings ownership; exercise tab, history, edit, and stale-result sequences directly through `Workspace::handle`.
3. Extend worker metadata, sort, and row-delta delivery, then adapt the existing presenter and desktop session without enabling new controls.
4. Verify shared-location sort and refresh across both Browsers, failure retention, runtime cancellation, and current-window browsing with focused tests before the full repository gate.

No new dependency is assumed. If implementation needs one, obtain the approval required by [AGENTS.md](../../AGENTS.md#dependencies-and-licensing) before changing the dependency graph.
