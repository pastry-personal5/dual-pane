# P4-M2 architecture

Status: Done

Keep filtering in Qt-free Browser Tab state and expose a visible row projection to the adapter. The [overview checklist](milestone-02-overview.md#completion-checklist) and its [owner decisions](milestone-02-overview.md#owner-decisions) define acceptance. This plan covers the application and adapter contract; P4-M3 connects it to a native filter field and model.

## Matching contract

Apply the same deterministic transformation to query text and each displayed name: Unicode case folding (including expansions such as `ß`/`ss`), canonical decomposition, and removal of accents and diacritics. Then use substring matching. The owner has approved matching the lossy displayed form of an invalid-UTF-8 name; its original `EntryName` bytes remain the only identity used for commands and operations. Tests must cover canonically equivalent spellings, accented/plain pairs, case-fold expansions, and two distinct byte names that render identically. Only empty text matches every Item; leading and trailing spaces are literal query characters.

Keep the transformation Qt-free and independent of the process locale, using the Unicode default non-Turkic fold. The existing `unicode-normalization` crate supplies decomposition and combining-mark classification but not full case folding. The owner approved adding [MIT-licensed `caseless` 0.2.2](https://github.com/unicode-rs/rust-caseless) during M2 implementation; it supplies full folding and already depends on `unicode-normalization`. Do not use a display string as a filesystem path or equate two Items because their transformed names match.

## State and output contract

Add a typed `SetFilterText { browser, tab, text }` command in `crates/dual-pane-application/src/input.rs`. Store the query, projection generation, and visible mapping with `TabState` in `workspace.rs`, not in `VisitState`, folder sort memory, or `TabSnapshot`. A new tab and a restored tab start with empty text. A successful read that confirms a different location clears the query as part of committing that location, including Back and Forward; a failed or superseded read does not. Switching tabs, refreshing the same folder, sorting, and watcher updates retain the query. A text edit starts no directory read.

The sorted `Arc<[Entry]>` in `FolderItems` remains the authoritative snapshot. Derive either an identity view for empty text or a visible-to-source index mapping for nonempty text. Keep the mapping tied to the exact tab ID, listing token, visibility generation, and filter generation that produced it. Publish immutable snapshot and mapping together with visible and total counts, visible metadata-size aggregates, and an explicit no-match flag. An accepted directory result with a still-active query must never briefly expose its unfiltered rows. A stricter visibility setting retains its existing immediate-clear behavior.

`crates/dual-pane-application/src/output.rs` and `crates/dual-pane-adapters/src/browser_presenter.rs` need a distinct **visible-row revision** that advances whenever a row mapping can change: filter, accepted listing/sort/visibility result, or tab switch. It is distinct from the existing `folder_items_revision`, which currently changes on listing replacement. The adapter's `entry(row)`, selection rows, cursor row, scroll hint, preview target, and summary read through one coherent visible mapping. A filter edit or filtered refresh can use a full model reset initially; do not apply worker-produced deltas for the unfiltered snapshot as if they were filtered row deltas. Preserve the current sort order without a second sort.

## Safe gestures and reconciliation

Extend the controller/application command contract so every row or selection-dependent gesture carries the visible-row revision observed by the widget. Row gestures also carry the exact `EntryName`. The application validates tab, revision, visible row, and exact name before selecting, opening, showing package contents, or updating a scroll hint. Revision validation also covers Select All, activation of the selection, inline Rename, and file-command capture; a queued command from an earlier view cannot act on a newer selection. This prevents a queued event from acting on a different row after rapid edits or tab changes, including a row whose number is still valid. P4-M3 must carry the revision across the native event boundary. On an accepted file command, the application captures only the current selected **visible** Items; an already-started operation keeps its frozen targets.

When a filter or refreshed projection hides Items, prune them from the visit's selection and cursor, clear a hidden range anchor, and retarget scroll to the nearest surviving row in the prior sorted order (prefer the following row on a tie). Preserve the pixel offset only if the old top Item survives; do not select the fallback row. Clearing the filter does not restore pruned selection. Range gestures traverse visible order, and Select All selects only visible Items. An Item newly created or renamed while a filter is active is auto-selected only if it matches the current projection. Selection-dependent Preview Pane and file-command availability must update with the reconciled state.

For filtered summaries, provide enough Qt-free aggregate data for the presenter to render visible matches out of total Items admitted by the current visibility settings, visible selected count, and selected visible size over visible total size. With no selection, show visible total size as the existing summary does. Unknown sizes keep the existing unavailable-size convention. A no-match listing has total Items but zero visible rows; an actually empty folder has zero total Items. P4-M3 owns the exact wording and accessibility announcement.

## Work and ordering

Matching every name after each keystroke can exceed the [GUI event-loop budget](../architecture.md#51-threading-and-responsiveness). Define a purpose-specific projection work request and result in `work_request.rs` and `input.rs` for immutable listing snapshots; calculate the visible mapping and aggregates on a worker. Coalesce superseded requests per tab, keep admission bounded, and apply a result only if its tab, listing, visibility, and filter generations still match. While a projection runs, publish an empty visible list with an updating state and block Item and selection-dependent actions. Retain the authoritative snapshot and visit state until the matching result arrives; the temporary blank list is not a selection-reconciliation event. A failed or cancelled projection keeps the list blank with retryable feedback, never silently exposes unfiltered rows or permits stale-row actions. M2 delivers and tests the matcher, reducer transitions, bounded desktop scheduler, and adapter projection; M3 connects the native filter field and carries observed revisions across that UI boundary.

## Verification scenarios

Qt-free tests use synthetic `Entry` snapshots and temporary-directory paths only where a path is needed. Cover:

- Empty text, literal leading/trailing whitespace, no matches, and a truly empty folder; case, composed/decomposed text, diacritics, full case folding, and invalid bytes with identical displayed names.
- Two tabs showing the same folder and both Browsers: edit, switch, close, and restore tabs without sharing or persisting a filter.
- Filter, clear, range, Select All, scroll, cursor, Preview Pane target, and summary changes; hidden Items never enter a new operation intent, while an operation already started retains its frozen targets.
- Pending, failed, cancelled, and retried projection: the list stays blank with the right state, Item commands cannot start, and temporary blank rows do not clear visit selection or cursor.
- Sort, watcher and operation refresh, visibility tightening, and a new-folder or rename result under a filter; preserve or prune state according to the resulting visible rows.
- Successful navigation, Back/Forward, same-folder refresh, failed navigation, and a superseded navigation result. A different-folder filter reset happens only on a confirmed successful change.
- Rapid edits, delayed projection results, stale row and selection-dependent events (including the same row number and rendered name), and a tab switch or close while work is in flight. No obsolete result may replace a newer view or retarget an action.
