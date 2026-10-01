# UX GUI

Status: Active

This document defines the currently rendered interface’s visual, accessibility, input, state, and responsiveness behavior. Product scope is in [product-behavior.md](product-behavior.md); deferred components are described only by [ux-terms.md](ux-terms.md).

## Visual layout and accessibility

Both Browser layouts have zero outer margins and zero gaps between child components for a flat, edge-to-edge appearance. Folder Pane borders and splitters use `#3A4048`; a key-window active Folder Pane uses a one-pixel `#2F6D9A` border. Active selected rows use `#2F6D9A` with white text; inactive-Browser selected rows use `#1E4668` with white text. When the application window is inactive, neither Folder Pane has an accent border and both selections use `#1E4668` with white text.

The Sidebar Splitter and Browser Divider are three-pixel draggable strips. Hovering adds a gray border; primary-button dragging removes that border until release. The current accessible names are Sidebar Splitter, Browser Divider, Path Edit Control, Up Button, Folder Items List, and Browser Status Bar. Controls retain these names as the window resizes; both Browsers share available space through their splitters without overlapping or suppressing ordinary interaction.

## Input and active-Browser state

Mouse and keyboard invoke the same application commands. Both Browsers open at the launch working directory; the Left Browser is active and its Folder Items List has keyboard focus. The active Browser is the target for Browser-specific commands.

| Gesture | Result |
|---|---|
| Primary click in a row | Activate that Browser and select the row. |
| Primary double-click in a row | Activate that Browser, select the row, and activate the row. |
| Primary click in empty Folder Items List space | Activate that Browser and clear selection. |
| Primary click on a Folder Items List scrollbar | Activate that Browser; leave selection unchanged. |
| Primary click on the Up Button | Activate that Browser and navigate to its parent. |
| Modified primary click | Behaves as the corresponding unmodified primary click. |
| Secondary or middle click | No-op. |
| Keyboard focus entering a Folder Items List | Activate that Browser. |
| `Option+F` | Activate the other Browser and move keyboard focus to its Folder Items List, preserving both selections and scroll positions. |
| Plain Tab | Reserved for macOS Full Keyboard Access; no file-manager command. The Up Button is not in keyboard focus traversal. |
| Exact Up, Down, Home, End, Page Up, or Page Down | Select according to the active Browser’s Folder Items List. Boundary movement is a no-op. With no selection, Home, Page Up, Up, and Down select the first row; End and Page Down select the last. Page Up and Page Down move by the number of fully visible rows, clamp at the boundary, and keep the selected row visible. With zero rows, all navigation is a no-op. |
| Exact Left | Navigate the active Browser to its parent; root is a no-op, including when the Folder Items List has zero rows. |
| Exact Right | Activate the selected row; without a selection or when the item cannot be entered, this is a no-op. |
| Exact Return | Activate the selected row, or no-op without selection. |
| Exact Command-Up | Navigate the active Browser to its parent; root is a no-op. |
| `Command+W` / `Command+Q` | Close the only app window / quit the app. |
| Modified movement key, modified Return or Command-Up, or any other gesture | No-op. |

## Responsiveness during work

The current implementation runs directory loading on separate Browser workers and keeps the last successful Folder Items visible while a read is pending or fails. A slow or unavailable location affects only the Browser that requested it; a newer navigation request takes precedence over an older result.

Automatic refresh, session persistence, and file operations are committed product scope but are not delivered in version 2.0.0. When implemented, they must not stall typing, pointer interaction, dialogs, rendering, or accessibility feedback; the implementation constraints are in [architecture.md](architecture.md#51-threading-and-responsiveness).
