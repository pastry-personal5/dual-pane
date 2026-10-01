# UX GUI

Status: Active

This document defines the current interface and records owner-approved Phase 3 GUI behavior separately under a planned heading. Product scope is in [product-behavior.md](product-behavior.md); component vocabulary is in [ux-terms.md](ux-terms.md).

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

## Planned Phase 3 GUI decisions

Implementation status: Planned. These owner decisions describe future behavior; the sections above continue to describe the current interface. P3-M3/P3-M4 incorporate them into the broader interaction design, and P3-M5/P3-M6 deliver the sorting model and GUI.

### Folder Items fields and display

- The columns appear from left to right as Name, Type, Relative Date Column, Exact Date Column, and Size. The two date columns show the same modification timestamp in different forms.
- The Relative Date Column uses compact elapsed units such as `17 h`, `2 d`, `11 M`, and `2 y`. Its text updates when the folder reloads; no periodic timer updates it.
- The Exact Date Column uses a local timestamp such as `2026-09-30 20:21`. Size uses decimal units such as `112.5 KB`. Unavailable values show `—`.

### Sort controls and remembered choice

- Name, Type, Date, and Size each have separate ascending and descending buttons; there is no direction-toggle cycle. Both Date displays share one Date button pair and sort by the same modification timestamp.
- The eight direct buttons sit beside the Up Button in Folder Pane Toolbar Row #2. They are very small bitmap carets (`^` for ascending and its inverse for descending), with descriptive tooltips and accessible names.
- A field's button pair is shown only while its field is visible. The Date pair remains while either date column is visible.
- Folders, including links to folders, stay ahead of non-folders for every sort. A folder without a remembered choice starts at natural Name ascending. A chosen sort is shared by location across Browsers and tabs and restored from persistent storage; the [P3-M2 plan](phase-3/milestone-02-architecture.md#folder-sort-memory) owns its retention and storage rules.

### Narrow Browser behavior

- As a Browser narrows, hide columns in this order: Size, Exact Date Column, Relative Date Column, Type. Name remains visible while the Browser is usable. Hide each field's sort controls with it.
- Below a 280-pixel Browser width, cover that Browser with a semi-transparent layer and center large, legible `Expand` text both horizontally and vertically. The text is an instruction, not a button; the person drags the Browser Divider to expand the Browser.

### Future shortcut editor

- A later shortcut editor lists customizable actions backed by stable UpperCamelCase action IDs and lets the person replace a default keyboard shortcut. For example, `NewFolder` starts with `Command+Shift+N` and can be assigned another valid shortcut once the action exists.
- The editor shows each action's effective shortcut, reports conflicts, and offers Restore Default. The [P3-M2 plan](phase-3/milestone-02-architecture.md#actions-and-shortcut-settings) owns action identity, validation, and persistence.

### Sidebar Favorites

- The Sidebar supports one level of Favorite Groups, each containing ordered Favorite Items. People can create, rename, reorder, move, and delete groups and items. The hierarchy and order persist across relaunches.
- At launch, a Favorite Item whose target folder is missing or unavailable is automatically removed. The [product behavior](product-behavior.md#favorites) defines this recovery rule; the Sidebar interaction design for editing is part of P3-M3/P3-M4.
