# Product behavior

Status: Active

This document is the active source of truth for committed product behavior, safety safeguards, session recovery, and exclusions. It describes the product scope rather than release-by-release availability; the [README](../README.md) states what version 2.0.0 currently delivers, and the UX documents define the interface.

## Included product scope

- One workspace window with two side-by-side Browsers, independent current locations, and one active Browser. Notices and operation UI may use auxiliary windows; there is no second workspace window.
- Tabs in each Browser, including tab history and an active tab.
- One level of Favorite Groups containing Favorite Items in the Sidebar. People can create, rename, reorder, move, and delete them; their hierarchy and order remain in memory and across relaunches.
- Copy and move between Browsers; rename; create directory; move to Trash; and permanent deletion.
- Opening a regular file or a macOS package, such as an application bundle, with its default application.
- Restoring each Browser's open tabs, their order, active tab, locations, and active Browser when the application relaunches. Folder sort choices are restored from location-shared memory; text filtering is outside Phase 3 behavior and session state.
- Watching open tab locations and automatically refreshing Folder Items, including inactive tabs, while preserving matching selection and cursor state.

## Favorites

A fresh profile starts with one Favorite Group named Favorites containing, in order, Applications, Desktop, Documents, Screenshots, and Downloads. Applications targets `/Applications`; Desktop, Documents, and Downloads target the corresponding folders in the user's home directory. Screenshots targets `~/Documents/Screenshots` only if a background check finds that folder; otherwise, including when the check cannot complete, that item is omitted. Dual Pane does not create target folders when seeding Favorites. Once initialized, an intentionally empty Favorites collection stays empty across relaunches.

At launch, a Favorite Item whose target folder is missing or unavailable is automatically removed from the saved collection after a completed target probe. This includes temporary unavailability. Empty Favorite Groups remain. A database-load failure, failed validation worker, or cancelled probe does not count as a target-folder result and cannot remove an item.

Favorite Group names and Favorite Item Aliases reject empty or whitespace-only text. Group names are unique among groups, and Item Aliases are unique within their group, using exact text equality. An alias changes only an Item's displayed label, not its target. Deleting a Favorite Group removes its Favorite Items without a confirmation.

## File-operation safeguards

- Delete moves items to Trash without a confirmation dialog.
- Permanent deletion is a separate command and always requires confirmation.
- A file whose name already exists as a file at the destination offers **Skip**, **Replace**, and **Cancel** in its Operation Decision Card. An initially unchecked option applies the selected choice to all remaining conflicts in that operation only.
- A folder copied or moved onto an existing folder with the same name is merged into it without asking. Only the files inside that collide ask the conflict question above.
- A file onto an existing folder with the same name, or a folder onto an existing file, offers **Try Again**, **Skip**, and **Cancel** in its Operation Decision Card. Neither item is replaced.
- A recoverable error, such as permission denied, a privacy restriction, no space, or an item in use, offers **Try Again**, **Skip**, and **Cancel** in its Operation Decision Card.
- If a recursive source scan cannot read a folder, **Skip** omits that folder's unread subtree and continues with other targets. Any containing directory that still holds skipped content must remain during permanent deletion.
- When permanent deletion or a move removes a source folder after handling its contents, a folder that cannot be removed offers **Try Again**, **Skip**, and **Cancel**; Skip keeps it. Folders that still hold skipped or failed content are kept without asking.
- Try Again/Skip/Cancel decisions have no option to apply the choice to remaining items.
- A rename or New Folder name collision never overwrites the existing Item.
- Opening a symbolic link to a folder navigates into it; activating a link to a regular file opens its target. Activating a macOS package (a bundle such as an `.app`), or a link to one, launches or opens it with its default application; **Show Package Contents** navigates into it. Copy and move act on the link itself, and operations never follow links inside folders.
- The safety invariants for replacement, cross-volume moves, symlinks, cancellation, and error handling are defined in [architecture.md](architecture.md#44-data-safety-invariants).

## Session recovery

If a saved tab location cannot be restored, Dual Pane discards that tab and restores the rest of the session normally. If the saved active tab is discarded, the nearest surviving tab on its right becomes active, then the nearest on its left. If a Browser has no restorable tab, it receives one clean tab at the launch directory. Back/forward history is kept during the session but is not restored after relaunch.

Notices holds nonblocking messages for the current session, including discarded tabs, storage errors, and operation summaries. At startup it opens only when messages exist. The default-unchecked “Don’t show notices at startup” preference persists across relaunches and suppresses routine startup notices; actionable storage errors still open Notices. Notice history is not restored.

The [Favorites policy](#favorites) governs unavailable Favorite Items independently of tab recovery.

## Out of scope

- Built-in viewer or Quick Look UI.
- Archive browsing.
- Remote file systems, including SFTP and SMB.
