# Product behavior

Status: Active

This document is the active source of truth for committed product behavior, safety safeguards, session recovery, and exclusions. It describes the product scope rather than release-by-release availability; the [README](../README.md) states what version 2.0.0 currently delivers, and the UX documents define the interface.

## Included product scope

- One application window with two side-by-side Browsers, independent current locations, and one active Browser. There is no multi-window mode.
- Tabs in each Browser, including tab history and an active tab.
- One level of Favorite Groups containing Favorite Items in the Sidebar. People can create, rename, reorder, move, and delete them; their hierarchy and order remain in memory and across relaunches.
- Copy and move between Browsers; rename; create directory; move to Trash; and permanent deletion.
- Opening a regular file with its macOS default application.
- Restoring each Browser's open tabs, their order, active tab, locations, sort/filter state, and active Browser when the application relaunches.
- Watching open locations and automatically refreshing Folder Items while preserving matching selection and cursor state.

## Favorites

A fresh profile starts with one Favorite Group named Favorites containing, in order, Applications, Desktop, Documents, Screenshots, and Downloads. Applications targets `/Applications`; Desktop, Documents, and Downloads target the corresponding folders in the user's home directory. Screenshots targets `~/Documents/Screenshots` only if that folder exists; otherwise that item is omitted. Dual Pane does not create target folders when seeding Favorites. Once initialized, an intentionally empty Favorites collection stays empty across relaunches.

At launch, a Favorite Item whose target folder is missing or unavailable is automatically removed from the saved collection after a completed target probe. This includes temporary unavailability. Empty Favorite Groups remain. A database-load failure, failed validation worker, or cancelled probe does not count as a target-folder result and cannot remove an item.

## File-operation safeguards

- Delete moves items to Trash without a confirmation dialog.
- Permanent deletion is a separate command and always requires confirmation.
- A file whose name already exists as a file at the destination offers **Skip**, **Replace**, and **Cancel**. The dialog includes an initially unchecked option to apply the selected choice to all remaining conflicts in that operation.
- A folder copied or moved onto an existing folder with the same name is merged into it without asking. Only the files inside that collide ask the conflict question above.
- A file onto an existing folder with the same name, or a folder onto an existing file, offers **Try Again**, **Skip**, and **Cancel**. Neither item is replaced.
- A recoverable error, such as permission denied, a privacy restriction, no space, or an item in use, offers **Try Again**, **Skip**, and **Cancel**.
- Try Again/Skip/Cancel dialogs have no option to apply the choice to remaining items.
- Opening a symbolic link to a folder navigates into it. Copy and move copy the link itself, and operations never follow links inside folders.
- The safety invariants for replacement, cross-volume moves, symlinks, cancellation, and error handling are defined in [architecture.md](architecture.md).

## Session recovery

If a saved tab location cannot be restored, Dual Pane discards that tab and restores the rest of the session normally. Back/forward history is kept during the session but is not restored after relaunch.

The [Favorites policy](#favorites) governs unavailable Favorite Items independently of tab recovery.

## Out of scope

- Built-in viewer or Quick Look UI.
- Archive browsing.
- Remote file systems, including SFTP and SMB.
- Package-specific browsing behavior, including application bundles.
