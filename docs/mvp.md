# MVP scope

Status: Active

This document is the single source of truth for the first usable Dual Pane release. It defines product scope and user-visible behavior; [architecture.md](architecture.md) defines the technical boundaries that implement it.

## Included

- Two side-by-side panes with independent current locations and one active pane.
- Tabs in each pane, including tab history and an active tab.
- Copy and move between panes; rename; create directory; move to Trash; and permanent deletion.
- Opening a regular file with its macOS default application.
- Restoring each pane's tabs, active tab, locations, sort/filter state, and active pane when the application relaunches.
- Watching open locations and automatically refreshing a changed listing while preserving matching selection and cursor state.

## Interaction

Mouse and keyboard are equally supported ways to invoke the same application commands. Concrete key bindings remain to be designed and must not be copied from another file manager by default.

Dragging items from one pane to the other copies them by default. Moving is an explicit operation.

### Responsiveness during work

Directory loading, automatic refreshes, session persistence, and file operations run without blocking ordinary interaction. While work is pending, a person can continue to use either pane, switch tabs, inspect or change selection, start independent safe work, and cancel the pending operation where cancellation is safe. The interface shows the current operation state and any required decision or error without making the whole application unusable.

A slow or unavailable location affects only the work or pane that depends on it. A newer navigation request takes precedence over an older listing, and a late result must not replace the newer view. Progress and automatic-refresh activity must not make typing, pointer interaction, dialogs, rendering, or accessibility feedback feel stalled. The implementation rules that make this possible are in [architecture.md](architecture.md#51-threading-and-responsiveness).

## File-operation safeguards

- Delete moves items to Trash without a confirmation dialog.
- Permanent deletion is a separate command and always requires confirmation.
- A destination conflict offers **Skip**, **Replace**, and **Cancel**. The dialog includes an initially unchecked option to apply the selected choice to all remaining conflicts in that operation.
- The safety invariants for replacement, cross-volume moves, symlinks, cancellation, and error handling are defined in [architecture.md](architecture.md).

## Session recovery

If a saved tab location cannot be restored, Dual Pane discards that tab and restores the rest of the session normally.

## Out of scope

- Built-in viewer or Quick Look UI.
- Archive browsing.
- Remote file systems, including SFTP and SMB.
- Package-specific browsing behavior, including application bundles.
