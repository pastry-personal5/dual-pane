# P3-M13 architecture: Settings Window skeleton

Status: Planned

P3-M13 is desktop-only composition around the application’s existing settings-health facts. It creates no editable configuration state and does not alter the settings database.

## Entry, lifecycle, and availability

The Main Toolbar owns its existing Settings button. The desktop adds a fixed `Command+,` shortcut that opens the same dialog; it is a standard presentation command, not a persisted or customizable `ActionId`. The dialog is a single parented Qt window-modal dialog opened asynchronously, so it blocks workspace input without a nested event loop or GUI-thread wait. A second entry raises and activates the existing instance.

The bridge exposes one read-only settings-interaction availability fact derived from the application’s load/save health. It is false during the first load and after a settings load or save failure. The desktop applies that fact to both entry points. If it becomes false while the dialog is visible, presentation closes the dialog before disabling its entry points; Notices continues to expose the established failure and Reset Settings recovery flow.

The dialog owns a left category list and a stacked content area. It starts with General Settings and Keyboard Shortcuts Settings placeholders only. It has accessible labels and standard focus order, opens with category-list focus, and rejects on Escape or the title-bar close control. P3-M14 replaces placeholders with live controls.

## Verification

Use a fakeable dialog/lifecycle seam around the desktop composition to verify entry coalescing, modality, focus, dismissal, and health changes without requiring a display. Keep the application, adapter, and storage tests unchanged except for the read-only availability projection. Perform a native macOS check after the automated gate.
