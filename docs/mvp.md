# MVP scope

Status: Active

This document is the single source of truth for the first usable Dual Pane release. It defines product scope and user-visible behavior; [architecture.md](architecture.md) defines the technical boundaries that implement it.

## Included

- One application window with two side-by-side panes, independent current locations, and one active pane. There is no multi-window mode.
- Tabs in each pane, including tab history and an active tab.
- Copy and move between panes; rename; create directory; move to Trash; and permanent deletion.
- Opening a regular file with its macOS default application.
- Restoring each pane's tabs, active tab, locations, sort/filter state, and active pane when the application relaunches.
- Watching open locations and automatically refreshing a changed listing while preserving matching selection and cursor state.

## Standard Layout

The **Standard Layout** has a **Sidebar**, **Sidebar Splitter**, **Left Browser**, **Browser Divider**, and **Right Browser**. The Sidebar contains a **Drives Group** of **Drive Item**s and **Favorites Groups** of **Favorite Group**s containing **Favorite Item**s. Each Left or Right Browser has its respective **Navigation Pane** above its **Folder Pane**. A Navigation Pane will contain **Browser Tabs** (not rendered in P2-M3) and has a read-only **Path Edit Control** showing that Browser’s absolute path. A Folder Pane has **Folder Pane Toolbar Row #1** (the concise current folder name), **Folder Pane Toolbar Row #2** (the **Up Button**), a **Folder Items List**, and a **Browser Status Bar** below the list. Both Browser layouts use zero outer margins and zero gaps between their child components for a flat, edge-to-edge appearance. The Sidebar Splitter and Browser Divider are three-pixel draggable strips using the inactive Folder Pane border color `#3A4048` at rest. Hovering either adds a gray border; pressing the primary mouse button to drag removes that border for the duration of the drag.

## Interaction

Mouse and keyboard are equally supported ways to invoke the same application commands. Two-pane browsing begins with both panes at the launch working directory; the left pane is active and its list has keyboard focus. The active pane is the target for pane-specific commands.

| Gesture | Result |
|---|---|
| Primary click in a row | Activate that pane and select the row. |
| Primary double-click in a row | Activate that pane, select the row, and activate the row. |
| Primary click in empty list space | Activate that pane and clear its selection. |
| Primary click on a pane scrollbar | Activate that pane; leave its selection unchanged. |
| Primary click on the Up control | Activate that pane and navigate to its parent. |
| Modified primary click | Behaves as the corresponding unmodified primary click. |
| Secondary or middle click | No-op. |
| Keyboard focus entering a pane's list | Activate that pane. |
| `Option+F` | Activate the other pane and move keyboard focus to its list, preserving both panes' selection and scroll position. |
| Plain Tab | Reserved for macOS Full Keyboard Access; no file-manager command. The Up control is not in keyboard focus traversal. |
| Exact Up, Down, Home, End, Page Up, or Page Down | Select a row in the active pane. At a boundary, movement is a no-op. With no selection, Home, Page Up, Up, and Down select the first row; End and Page Down select the last. Page Up and Page Down move by the number of fully visible rows, clamp at the boundary, and keep the selected row visible. With zero rows, all navigation is a no-op. |
| Exact Left | Navigate the active pane to its parent; at the root, no-op. This works even when the list has no rows. |
| Exact Right | Activate the selected row; without a selection or when the selected item cannot be entered, no-op. |
| Modified movement key | No-op. |
| Exact Return | Activate the selected row; without a selection, no-op. |
| Exact Command-Up | Navigate the active pane to its parent; at the root, no-op. |
| `Command+W` | Close the current app window. In Phase 2, this is the only window, so the app exits. |
| `Command+Q` | Quit the app. |
| Any other key gesture, including modified Return or Command-Up | No-op. |

Browsing gestures invoke only the application-level commands to activate a pane, select a row, clear selection, activate a row, or navigate to a parent; the no-op gestures invoke none. In a key window, the active Folder Pane has a one-pixel `#2F6D9A` border and the Up control retains its neutral border. Active selected rows use `#2F6D9A` with white text; inactive-pane selected rows use `#1E4668` with white text. In an inactive application window, neither Folder Pane has an accent border and both selected rows use `#1E4668` with white text.

Dragging items from one pane to the other copies them by default. Moving is an explicit operation.

### Responsiveness during work

Directory loading, automatic refreshes, session persistence, and file operations run without blocking ordinary interaction. While work is pending, a person can continue to use either pane, switch tabs, inspect or change selection, start independent safe work, and cancel the pending operation where cancellation is safe. The interface shows the current operation state and any required decision or error without making the whole application unusable.

A slow or unavailable location affects only the work or pane that depends on it. A newer navigation request takes precedence over an older listing, and a late result must not replace the newer view. Progress and automatic-refresh activity must not make typing, pointer interaction, dialogs, rendering, or accessibility feedback feel stalled. The implementation rules that make this possible are in [architecture.md](architecture.md#51-threading-and-responsiveness).

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

If a saved tab location cannot be restored, Dual Pane discards that tab and restores the rest of the session normally.

## Out of scope

- Built-in viewer or Quick Look UI.
- Archive browsing.
- Remote file systems, including SFTP and SMB.
- Package-specific browsing behavior, including application bundles.
