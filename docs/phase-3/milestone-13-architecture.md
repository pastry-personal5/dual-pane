# P3-M13 architecture: Settings Window skeleton

Status: Done

P3-M13 adds a read-only Qt-free settings-interaction availability fact to the existing application, adapter, bridge, and desktop path. It creates no editable configuration state and does not alter the settings database.

## Entry, lifecycle, and availability

The application initializes settings-interaction availability as false. A successful settings load makes it true. A load failure, timeout, reset/reload, or any non-stale write failure from the shared settings database, including a session write, makes it false. A later ordinary successful write does not recover it; only a successful Reset Settings followed by successful replacement load does. `WorkspaceChrome`, `WorkspacePresenter`, and `WorkspaceBridge` project this boolean without exposing storage or database details to Qt.

Settings and session saves have separate revision sequences. Their failure events keep those sequences distinct, and Reset Settings records a stale-result floor for each so an old failure cannot disable a newly loaded Settings Window while a current session failure still can.

The Main Toolbar owns its existing Settings button. The desktop adds a fixed `Command+,` shortcut that opens the same dialog; it is a standard presentation command, not a persisted or customizable `ActionId`, and P3-M14 reserves it from editable macOS bindings. The dialog is a single parented Qt window-modal dialog opened asynchronously, so it blocks workspace input without a nested event loop or GUI-thread wait. A second presentation request raises and activates the existing instance. Its shortcut has workspace-window scope and therefore cannot fire while the modal dialog owns focus.

The desktop applies availability to both entry points. If it becomes false while the dialog is visible, its lifecycle owner rejects the dialog before disabling its entry points; Notices continues to expose the established failure and Reset Settings recovery flow.

The dialog owns a left category list and a stacked content area. It starts with General Settings and Keyboard Shortcuts Settings placeholders only. It has accessible labels and standard focus order, opens with category-list focus, and rejects on Escape or an in-window Close control at the upper right. There is no footer. It has a 1400 by 900 logical-pixel minimum; the category sidebar reuses the main window’s dark surface, blue selection, neutral hover, and divider while the content pane uses the Browser surface. P3-M14 replaces placeholders with live controls.

## Verification

Cover availability transitions in application tests and its projection in adapter and bridge tests. `scripts/check-settings-window.sh` exercises real Qt widgets with the offscreen platform plugin for entry coalescing, modality, focus, dismissal, and health changes. Perform a native macOS check after the automated gate.
