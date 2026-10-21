# P4-M11 architecture

Status: Planned

This milestone is a move-and-reseam, not a new feature: every moved class, function, or struct keeps its current interface and behavior. The [overview checklist](milestone-11-overview.md#completion-checklist) is the definition of done.

## Current layout

`desktop_window.cpp` defines roughly thirty classes in one anonymous namespace, from low-level widgets (`ThinSplitterHandle`, `IconLoader`, `ItemIconDelegate`) through mid-level panes (`FolderPane`, `Browser`, `FavoritesPanel`, `OperationPanels`) up to the top-level shell (`WorkspaceWindow`, `SettingsWindow`, `MainToolbar`, `Sidebar`, `ShortcutBinder`). Its header, `desktop_window.hpp`, exposes only `run_desktop` and `schedule_gui_drain`; everything else is file-local. On the Rust side, `runtime.rs`, `settings_storage.rs`, `operation_step.rs`, `browser_session.rs`, `workspace_bridge.rs`, and `folder_items_list_model.rs` each grew past 1,000 lines because they each accumulated logic for several of these same feature areas rather than being organized by them.

The P4-M10 commits show the resulting shotgun surgery concretely: adding the context-menu feature touched `desktop_window.cpp` (the menu-building and settings-page code, scattered across different classes in the same file), `browser_session.rs` (the request bridge), `folder_items_list_model.rs` (model-side availability), `settings_storage.rs` (schema and persistence), and `workspace_bridge.rs` (the CXX-Qt bridge surface) — five files for one action.

## Target module layout

Group both sides by the same feature areas, so a feature's C++ and Rust code sit under matching names:

| Feature area | New C++ files (`cpp/src/`, `cpp/include/dual_pane_desktop/`) | Classes moved in |
|---|---|---|
| Window shell | `workspace_window.{hpp,cpp}` | `WorkspaceWindow`, `WaitingWindow`, `NoticesWindow`, `activate_workspace_window`, `DrainScheduler`, `SchedulerState` |
| Toolbar and sidebar | `toolbar_sidebar.{hpp,cpp}` | `MainToolbar`, `Sidebar`, `ApplicationCloseButton`, `BrowserHighlightController` |
| Tabs strip | `tabs_strip.{hpp,cpp}` | `BrowserTabButton`, `BrowserTabsStrip` |
| Folder pane | `folder_pane.{hpp,cpp}` | `ThinSplitterHandle`, `ThinSplitter`, `IconLoader`, `ItemIconDelegate`, `FolderItemsList`, `ElidedStatusLabel`, `FolderFilterTextEdit`, `FilterRowState`, `FolderPane`, `FileNameEditor`, `NameEditController`, `PreviewPane`, `FolderItemsMenuSession`, `FolderItemsMenuCoordinator`, `Browser` |
| Favorites panel | `favorites_panel.{hpp,cpp}` | `InlineNameEditor`, `FavoriteItemRow`, `FavoriteGroupRow`, `FavoritesPanel` |
| Operation panels | `operation_panels.{hpp,cpp}` | `OperationPanel`, `FloatingPanelWindow`, `OperationPanelStrip`, `OperationPanels`, `DeleteConfirmations` |
| Settings window | `settings_window.{hpp,cpp}` | `ToggleSwitch`, `SettingsTitleRow`, `SettingsWindow`, `SettingsWindowController` |
| Shortcuts | `shortcuts.{hpp,cpp}` | `ShortcutGlyphButton`, `ShortcutCapture`, `ShortcutBinder` |

`desktop_window.cpp` keeps only `run_desktop`, `schedule_gui_drain`, and whatever top-level wiring assembles these pieces into the running application. Shared file-local helpers used by more than one area (for example `repolish`) move to whichever area uses them first, or to a small shared internal header if genuinely common; this is a mechanical call made during implementation, not a design decision.

On the Rust side, each of the six large files splits into a module per feature area it currently mixes together (for example `settings_storage::context_menus`, `settings_storage::favorites`, `settings_storage::shortcuts`), keeping the existing public functions and types at their current paths through re-exports where other crates or CXX-Qt-generated code reference them by path. No public API changes.

## Folder-item action seam

Today, a folder-item action (rename, open, copy, cut, paste, move to trash, etc.) exists as: an `ActionId` and label in the Qt-free application catalogue (already centralized — not part of this seam), a menu-building branch in `desktop_window.cpp`'s folder-pane classes, a visibility row in the Context Menus Settings page (also in `desktop_window.cpp`), an availability check in `folder_items_list_model.rs`, a persistence row in `settings_storage.rs`, and a bridge method in `workspace_bridge.rs`.

This milestone adds one Rust-side table — a `const`/static array keyed by the existing `ActionId`, held alongside the application catalogue it already matches — that each of the five touch points reads instead of maintaining its own list:

- the folder-pane menu-building code iterates the table to build menu rows instead of a hand-written per-action branch;
- the Context Menus Settings page iterates the same table to build its visibility matrix rows;
- `folder_items_list_model.rs` looks up an action's availability rule in the table instead of a parallel `match`;
- `settings_storage.rs` persists visibility keyed by the table's `ActionId`s instead of a separately maintained schema list;
- `workspace_bridge.rs` dispatches an invoked action through the table's bridge-call descriptor instead of a separate `match` arm per action.

Adding a thirteenth action becomes one new table row plus (when the action needs genuinely new logic, such as a new kind of bridge call) one new case in the small set of execution functions the table can reference — not five separate edits across five files. This does not change the schema-11 persisted format, the catalogue's `ActionId` values, or any Settings or menu behavior; it only changes where the five touch points get their data from.

## Verification

Since behavior does not change, verification is the existing full gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, all five native scripts (`check-settings-window.sh`, `check-text-editor-focus.sh`, `check-application-title-row.sh`, `check-folder-items-context-menu.sh`, `check-single-instance.sh`), and `make check`. Passing these without modification to their assertions is the proof that the split and the seam preserved behavior. No new tests are required by this milestone; if a moved module turns out to need test-only visibility it did not have before (for example, a `pub(crate)` that was private), that is a mechanical implementation detail, not new test coverage.
