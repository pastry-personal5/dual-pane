# P4-M10 architecture

Status: Planned

The application owns two persistent Folder Items menu configurations and produces one immutable, observation-tagged menu snapshot for the desktop to render. Qt classifies the native invocation, asks for that snapshot, and draws it; it does not decide command availability, grouping, targets, or settings validity. The [overview checklist](milestone-10-overview.md#completion-checklist) defines acceptance.

## Settings model and persistence

Add application types equivalent to `ContextMenuKind::{Normal, Comprehensive}`, `ContextMenuGroup`, and `ContextMenuPreferences`, keyed by an explicit `CONTEXT_MENU_ACTIONS` subset of the eleven existing `ActionId` values approved in the overview. One application catalogue defines eligible IDs, group/order, and factory visibility. Labels and native shortcut notation remain presentation concerns. This subset, rather than `ActionId::ALL`, is the source of truth for validation and storage.

`SettingsState` owns the preferences with Favorites, shortcuts, and visibility policy. `SettingsSnapshot::default()` contains complete factory preferences, so fresh profiles and the many existing default-snapshot callers cannot create an invalid empty configuration. Add typed commands to change one action's visibility and to reset Normal, Comprehensive, or both. A duplicate value is a no-op. A change that would leave one variant empty returns a typed rejection without increasing the settings revision or requesting a save. Accepted changes publish updated workspace chrome immediately and use the existing debounced save path. Global Reset Settings supplies `ContextMenuPreferences::default()` with the other compiled defaults.

Extend `SettingsSnapshot` and the SQLite driver at schema version 11. A `context_menu_action` table stores menu kind, stable `ActionId` name, and visibility with a composite primary key plus kind and Boolean checks. The version-11 step creates the table and inserts both rows for every eligible action in the same transaction as the version change; because migrations are cumulative, it must work for a fresh database and every supported schema through version 10, with a focused version-10 migration test. Loading requires exactly one valid row per eligible action and variant and at least one visible action per variant. Extra or unknown kinds/actions, invalid Booleans, duplicates, missing rows, and empty variants are corrupt settings rather than silent policy changes. Saving validates first and replaces the rows in the existing settings transaction. Preserve the current recovery behavior for corrupt and newer databases. No dependency or build-tool change is needed.

## Menu projection and dispatch

Add a single application input equivalent to `RequestFolderItemsMenu` containing the requested variant plus an observation of the source Browser, tab, visible revision, and invocation target. A row target carries the displayed row and exact `EntryName`; an empty-space target is explicit. Handling the request activates the source Browser, applies the existing secondary-selection rule for a valid row, and emits the menu result atomically. `BrowserSession` captures that output for the synchronous bridge request; the application reducer does not return a Qt model or expose a callback. A stale row observation emits no snapshot, so Qt cannot open a menu for a different Item after a refresh or tab switch. A keyboard or accessibility request is translated to the current cursor row when one is visible, otherwise to empty space.

The application produces a `FolderItemsMenuSnapshot` or equivalent from:

- the requested variant;
- the observed Browser, tab, listing revision, and row-or-empty invocation target;
- the exact current selection after the row-selection rule;
- application command availability, including the other Browser destination;
- cached clipboard offer state; and
- the effective ordered shortcut lists.

The application applies configured visibility and current availability, excludes selection actions from an empty-space invocation even though the selection is preserved, drops empty groups, and emits only nonempty groups. It returns no snapshot when no action remains. The request does no I/O and never synchronously refreshes the clipboard. Each row carries its stable `ActionId` and optional first effective `Shortcut`. The adapter supplies canonical labels and a portable key sequence; Qt converts that sequence to `QKeySequence::NativeText` for display without creating another binding.

Each nonempty snapshot receives a short-lived token and an allow-list of the actions actually emitted. Its observation retains the exact tab and visible revision, row identity and selection required by source actions, and the captured source or destination locations and other-Browser tab required by each action. The CXX-Qt bridge exposes immutable groups/rows and sends only the token plus chosen `ActionId` back. Keep at most one live Folder Items menu observation for the workspace; a newer request, dismissal, invocation, tab/listing invalidation, or shutdown retires the previous token. An unknown, retired, repeated, or non-emitted action is rejected.

Add application inputs equivalent to `InvokeFolderItemsMenu { token, action }` and `DismissFolderItemsMenu { token }`. Before dispatch, the application consumes the token and applies command-specific validation against current state; dismissal only retires its matching token. Selection-dependent commands require the captured tab/listing and exact selection. Open Item and Show Package Contents require the exact captured single Item, with the latter still a package. Copy/Move to Other Browser also require the captured destination Browser, tab, and location. New Folder and Paste require the captured source tab and destination location to remain current; they never fall through to a newly active tab. Paste then performs its ordinary fresh clipboard generation check and asynchronous read. A stale snapshot starts no editor, navigation, clipboard read, or file operation and changes no files. Existing command rejection and Browser Status Bar behavior remain authoritative after a valid snapshot reaches a command whose external conditions changed.

## Native interaction and Settings presentation

Route mouse, keyboard, and accessibility `QContextMenuEvent` entry through one `FolderItemsList` path, and suppress the old right-button `mousePressEvent` menu path so one gesture cannot open twice. Preserve the current selection timing: the application selection change occurs before the menu is displayed and remains if the menu is dismissed. Exact user modifiers after ignoring non-user flags such as `KeypadModifier` select Comprehensive only when Shift is the sole modifier; every other secondary invocation selects Normal. A non-pointer request anchors beside the visible cursor row, or at a documented inset within the viewport when there is no visible cursor. The same row-or-empty classification is sent to the application.

Render both variants through one `QMenu` builder and the existing dark native style. Give them distinct accessible names without a visible heading. Render shortcut text in the native right-hand column without calling an API that registers another active shortcut. Let `QMenu::exec` return the chosen `QAction`, then submit exactly one token/action pair after the nested event loop returns; submit dismissal when it returns no action. This avoids relying on `triggered` versus `aboutToHide` signal order. Do not live-update an open menu; validity is checked when its action is submitted.

Add a third `SettingsWindow` page after Keyboard Shortcuts Settings. Its scrollable categorized matrix uses the overview's Action, Normal, Comprehensive, and Shortcut columns. Checkbox changes call the typed application command; a final-item rejection restores the checked state and shows “Normal Context Menu must keep at least one action.” or its Comprehensive equivalent inline until the next accepted edit, reset, category change, or window dismissal. Controls follow the existing `settings_interaction_available` state. A dedicated context-menu-preferences revision/signal resynchronizes checkboxes after accepted edits, loads, and resets; the existing bindings revision/signal resynchronizes the read-only first-shortcut column. Rebuild or update controls with signals blocked.

Reset Normal…, Reset Comprehensive…, and Reset Both… each open a window-modal confirmation with Cancel as default and Escape button. Each confirmation names its exact scope; an accepted no-op reset makes no save request. Accepted resets leave the Settings Window open, clear rejection feedback, and immediately resynchronize the matrix. Use the existing Settings surfaces, cards, table headers, focus treatment, and compact controls; both menu preferences are global, not Browser-specific.

## Verification and sequencing

Implement in this order:

1. Add the application catalogue, complete defaults, validation, request/invoke/dismiss inputs, snapshot/token lifecycle, and Qt-free tests.
2. Add schema-11 storage plus fresh, migration, round-trip, and corruption tests.
3. Add adapter labels/view models and expose settings plus observation-tagged snapshots through the CXX-Qt bridges.
4. Replace the hard-coded native menu with the single `QContextMenuEvent`/`QMenu::exec` path and add the Settings page.
5. Extend `scripts/check-settings-window.sh`, add `scripts/check-folder-items-context-menu.sh`, list the new command in AGENTS.md and README, update active docs, run the complete gate, and then run the guided macOS acceptance.

Tests cover zero, one, and multiple selections; selected and unselected secondary-clicks; exact Shift and every other modifier class; empty space; regular files, folders, links, packages, and unsupported Items; loading, error, missing-folder, and filtered states; other-Browser destination changes; cached supported and unsupported clipboard offers; customized, multiple, reset, and unbound shortcuts; configuration changes and resets; both Browsers; pointer, keyboard, and accessibility events; menu dismissal and single-dispatch behavior; stale listing, selection, tab, Item, and destination state; settings-unavailable behavior; fresh schema creation, every supported upgrade path, malformed rows, relaunch, and newer-schema recovery. Native checks inject clipboard state through the existing isolated seam and never replace the global clipboard.
