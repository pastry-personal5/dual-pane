# Product behavior

Status: Active

This document is the active source of truth for committed product behavior, safety safeguards, session recovery, and exclusions. It describes the product scope rather than release-by-release availability; the [README](../README.md) states what version v3.0.0 currently delivers, and the UX documents define the interface.

## Included product scope

- One workspace window with two side-by-side Browsers, independent current locations, and one active Browser. Notices, operation UI, and the system Quick Look Panel may use auxiliary windows; there is no second workspace window.
- Tabs in each Browser, including tab history and an active tab.
- One level of Favorite Groups containing Favorite Items in the Sidebar. People can create, rename, reorder, move, and delete them; their hierarchy and order remain in memory and across relaunches.
- Copy and move between Browsers; rename; create directory; move to Trash; and permanent deletion.
- Opening a regular file or a macOS package, such as an application bundle, with its default application.
- Previewing a Folder Pane item with native macOS Quick Look.
- Restoring each Browser's open tabs, their order, active tab, locations, and active Browser, and the workspace window's frame, zoomed or full-screen state, and Sidebar Splitter and Browser Divider positions, when the application relaunches. Folder sort choices are restored from location-shared memory; text filtering is outside Phase 3 behavior and session state.
- Watching open tab locations and automatically refreshing Folder Items, including inactive tabs, while preserving matching selection and cursor state.
- Planned for Phase 4: tab-local name substring filtering of the current folder's Items, using full Unicode case folding and accent-insensitive matching on displayed names. Query spaces are literal; exact filename bytes still identify Items for actions. Switching tabs retains each tab's filter; any successful change to a different confirmed folder, including Back or Forward, clears it. Failed navigation and same-folder refresh keep it; filters reset on relaunch. Hidden Items leave selection and cursor state and cannot be newly targeted by file commands. While a filter update runs, the list clears temporarily without erasing visit state or allowing Item actions. The [P4-M2 owner decisions](phase-4/milestone-02-overview.md#owner-decisions) define the view behavior. Editable path navigation accepts absolute, `~/` home-relative, and paths relative to the submitting tab's confirmed folder. Per the [P4-M6 owner decisions](phase-4/milestone-06-overview.md#owner-decisions), a path to a file opens its folder with the file selected, a path to a package opens its contents, and typed letter case or Unicode normalization resolves to the folder's stored spelling. Clickable components of the confirmed absolute path and an expandable column view of available folder paths are also planned; their interaction details remain assigned to [P4-M6](phase-4/milestone-06-overview.md) and [P4-M8](phase-4/milestone-08-overview.md).
- Planned for Phase 4: file Copy in Dual Pane publishes a Finder-compatible clipboard representation for all selected Items, including multiple files, folders, packages, and symbolic links. Finder can paste them until the clipboard is replaced, including after Dual Pane quits or relaunches. Files copied in Finder can be pasted into Dual Pane as a Copy through its safeguarded file-operation workflow, leaving Finder's sources in place. Copy, Cut, and Paste also work within Dual Pane through the macOS clipboard. Dual Pane does not export Cut as a Finder move. Dual Pane's internal transfer intent becomes invalid when another app replaces the clipboard or Dual Pane relaunches; a valid Finder file Copy remains eligible for Paste after Dual Pane relaunches while the clipboard is unchanged. When a snapshot contains both, a valid current private transfer takes precedence; a stale or unknown private transfer may fall through to valid file URLs as external Copy, while Cut cannot fall through. A Copy can be pasted repeatedly, including into overlapping independent operations, until clipboard replacement. Paste requires a ready active tab with a confirmed, successfully loaded destination and no pending load or error. A same-folder Paste is rejected with an explanation. A Cut rejected before an operation starts remains pending; after a Cut Paste starts, it is reserved against another Paste and consumed at any terminal result, so retrying remaining sources requires a new Cut. Frozen byte-exact source paths are never replaced after rename; a missing or kind-changed root rejects, while supported same-kind content changed in place is handled as scanned. Finder sources receive an asynchronous no-follow preflight; any malformed, duplicate, unavailable, or unsupported root rejects the entire Paste before an operation starts. File drag and drop remains within Dual Pane; an accepted internal drop moves by default and Option copies. The [P4-M4 owner decisions](phase-4/milestone-04-overview.md#owner-decisions) define clipboard admission and lifetime; the [P4-M5 owner decisions](phase-4/milestone-05-overview.md#owner-decisions) define native fallback, menus, and feedback.
- Three global file and folder visibility settings, planned for P3-M15: macOS metadata and service items hidden by default, other leading-dot items shown by default, and items marked invisible by macOS hidden by default. The exact names, precedence, and controls are in the [P3-M15 owner decisions](archive/phases/phase-3/milestone-15-overview.md#owner-decisions). These settings affect Folder Items display, not the contents of file operations.

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

## Command-line launch

`dual-pane <folder>` shows that folder in the Left Browser after the saved session restores: if the Left Browser already has a tab on the folder, that tab becomes active; otherwise a new tab opens beside the others and becomes active, or, at the tab limit, the active tab navigates there. `dual-pane <file>` does the same for the file's folder and selects the file without launching it. Relative paths resolve against the shell's working directory and `~` against the home directory. A path that does not exist, a folder that cannot be read, an unknown option, or more than one path prints an error to standard error and exits with status 2 before any window opens; `--help` prints usage. Every launch starts its own instance; forwarding to a running instance is not supported. The target also applies when settings time out or the session is damaged or absent.

## Session recovery

If a saved tab location cannot be restored, Dual Pane discards that tab and restores the rest of the session normally. A location cannot be restored when the tab's first read at relaunch finds it missing, not a folder, or unreadable because of permissions, a privacy restriction, or another error. This includes temporary unavailability, such as a disconnected volume. A reader failure, a full read queue, or a cancelled read says nothing about the location and never discards a tab; that tab keeps its folder and error, and Refresh retries it. If the saved active tab is discarded, the nearest surviving tab on its right becomes active, then the nearest on its left. If a Browser has no restorable tab, or no saved session exists, it receives one clean Home tab at the user's home directory. Back/forward history is kept during the session but is not restored after relaunch.

A failure to classify a child item's macOS visibility is a reader failure, not evidence that the tab's folder is unrestorable. The tab stays open with a retryable listing error.

If readable saved session data, including the window layout, is damaged, each Browser opens one clean Home tab and Notices reports that the previous session could not be restored. Settings and Favorites stay usable and saveable, and the next session save replaces the damaged session. A storage read failure that prevents checking the session follows the settings-load failure behavior and does not overwrite the saved data.

If settings loading has not answered after ten seconds, a Waiting Window closes and the workspace opens at its default layout with one Home tab per Browser. Notices reports the timeout and that this launch's tabs and layout will not be saved. A later successful load supplies settings and Favorites but leaves the visible workspace alone; the prior saved session and layout remain available for the next launch. A later failure keeps the in-memory defaults and storage writes disabled. A deliberate successful Reset Settings starts a fresh saved session.

A restored tab whose folder does not answer, such as one on an unresponsive volume, keeps loading and is never discarded for being slow. Each discarded tab gets its own Notice. A saved window frame that no longer fits the connected displays moves fully onto the display it overlaps most, or the main display, and shrinks only if it must.

A successful Reset Settings also resets the workspace: every tab closes, each Browser receives one clean Home tab, the Left Browser becomes active, and the window leaves full screen or zoom and returns to its default size centered on the current display, with its splitters at their default positions. The confirmation says so beforehand, and a failed reset changes none of it.

Notices holds nonblocking messages for the current session, including discarded tabs, storage errors, and operation summaries. At startup it opens only when messages exist. The default-unchecked “Don’t show notices at startup” preference persists across relaunches and suppresses routine startup notices. With it checked, only startup notices that offer an action, such as Reset Settings or Try Again, still open Notices; discarded tabs, a damaged session, removed Favorite Items, and temporaries-sweep reports, including sweep failures, are recorded without opening it. Notices about the person's own actions, such as an opening failure, follow their usual rules. Notice history is not restored.

The [Favorites policy](#favorites) governs unavailable Favorite Items independently of tab recovery.

## Out of scope

- Built-in viewer.
- Archive browsing.
- Remote file systems, including SFTP and SMB.
