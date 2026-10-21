# P4-M11 architecture

Status: Done

This milestone is a move-and-reseam, not a new feature: every moved class, function, or struct keeps its current interface and behavior. The [overview checklist](milestone-11-overview.md#completion-checklist) is the definition of done.

## Starting layout

`desktop_window.cpp` defined roughly thirty classes in one anonymous namespace, from low-level widgets (`ThinSplitterHandle`, `IconLoader`, `ItemIconDelegate`) through mid-level panes (`FolderPane`, `Browser`, `FavoritesPanel`, `OperationPanels`) up to the top-level shell (`WorkspaceWindow`, `SettingsWindow`, `MainToolbar`, `Sidebar`, `ShortcutBinder`). On the Rust side, `runtime.rs`, `settings_storage.rs`, `operation_step.rs`, `browser_session.rs`, `workspace_bridge.rs`, and `folder_items_list_model.rs` each grew past 1,000 lines because they each accumulated logic for several feature areas rather than being organized by them.

## C++ modules

All moved code lives in `namespace dual_pane_desktop::gui`. No class has a Q_OBJECT macro, so no moc step is involved. Classes keep their in-class member definitions and now live in area headers; free functions are declared in the header and defined in the matching `.cpp`. An area with no free function has a header only. `gui_common.hpp` carries the Qt and standard includes every area needs, the shared constants (as `inline constexpr`), and the helpers more than one area uses.

| Feature area | Header (`cpp/include/dual_pane_desktop/`) | Source (`cpp/src/`) | Contents |
|---|---|---|---|
| Shared | `gui_common.hpp` | `gui_common.cpp` | constants, `style_sheet`, `repolish`, `fit_saved_frame`, `inside`, command and navigation icon helpers, `Movement` and `user_modifiers` |
| Window shell | `workspace_window.hpp` | `workspace_window.cpp` | `WorkspaceWindow`, `WaitingWindow`, `NoticesWindow`, `activate_workspace_window`, `DrainScheduler`, `SchedulerState` |
| Toolbar and sidebar | `toolbar_sidebar.hpp` | `toolbar_sidebar.cpp` | `MainToolbar`, `Sidebar`, `ApplicationCloseButton`, `BrowserHighlightController` |
| Tabs strip | `tabs_strip.hpp` | none | `BrowserTabButton`, `BrowserTabsStrip` |
| Folder pane | `folder_pane.hpp` | `folder_pane.cpp` | splitters, `IconLoader`, `ItemIconDelegate`, `FolderItemsList`, `FolderPane`, name editing, `PreviewPane`, `FolderItemsMenuSession`, `FolderItemsMenuCoordinator`, `Browser` |
| Favorites panel | `favorites_panel.hpp` | none | `InlineNameEditor`, `FavoriteItemRow`, `FavoriteGroupRow`, `FavoritesPanel` |
| Operation panels | `operation_panels.hpp` | none | `OperationPanel`, `FloatingPanelWindow`, `OperationPanelStrip`, `OperationPanels`, `DeleteConfirmations` |
| Settings window | `settings_window.hpp` | none | `ToggleSwitch`, `SettingsTitleRow`, `SettingsWindow`, `SettingsWindowController` |
| Shortcuts | `shortcuts.hpp` | `shortcuts.cpp` | `ShortcutGlyphButton`, `ShortcutCapture`, `ShortcutBinder` |
| Offscreen checks | `desktop_checks.hpp` | `desktop_checks.cpp` | the four `run_*_checks` entry points the native scripts start |

`desktop_window.cpp` keeps `run_desktop`, `schedule_gui_drain`, and `RefreshCoordinator`, the wiring that assembles these pieces into the running application. `build.rs` lists the new `.cpp` files, and `scripts/format-cpp.sh` and `scripts/lint-cpp.sh` list every new header and source.

The classes stay in headers rather than moving out of line because they form a tight graph (`Browser` owns `FolderPane`, `FolderItemsList`, and the tabs strip; `Sidebar` owns `FavoritesPanel`) and because an out-of-line rewrite of roughly 150 member functions would risk behavior changes this milestone forbids. Each header is wrapped in the same clang-tidy suppression block the single file used.

## Rust modules

Each large file keeps its type definitions, public API, and orchestration, and moves feature code into child modules. A child starts with `use super::*;`, widened `pub(super)` visibility replaces the old private visibility where a sibling or the parent calls in, and the parent re-exports anything that was `pub` so no path outside the file changed. Test modules moved to `tests.rs` files beside their parents.

| File | Child modules |
|---|---|
| `settings_storage.rs` | `codec`, `context_menus`, `error`, `favorites`, `folder_sorts`, `markers`, `migration`, `reset`, `session`, `shortcuts`, `worker`. `SettingsDatabase::load` and `save` call one `load` and `save` function per table group, in the same order as before. |
| `runtime.rs` | `filter_lane`, `job_queue`, `probes`, `clipboard`, `supervision` |
| `operation_step.rs` | `naming`, `removal`, `copying`, `moving`, `finalization`, `stops`. The large `impl OperationState` is split by operation kind. |
| `browser_session.rs` | `shortcuts`, `context_menus`, `operations`, `favorites`, `browser_input` |
| `workspace_bridge.rs` | `desktop_session`, `favorites`, `notices`, `settings`, `shortcuts`, `context_menus`. The `#[cxx_qt::bridge]` module stays in the parent file because CXX-Qt reads it from there; its method bodies live in the children. |
| `folder_items_list_model.rs` | `preview`, `context_menus`, `selection`, `navigation`, `tabs`, `filter`, `operations`, `status`. Same bridge rule as above. |

Where a Rust module and a C++ file share a feature name (`shortcuts`, `context_menus`, `favorites`, `settings`), they are the two halves of that feature.

## Folder-item action catalogue

The plan expected the menu UI, Settings pages, Folder Items model, storage, and bridge each to keep a per-action list. Implementation found the desktop side already generic after P4-M10: the model reads the typed menu snapshot, storage and the Context Menus page iterate `CONTEXT_MENU_ACTIONS`, and the bridge exposes rows by index. The per-action facts were instead spread across `settings.rs` (identifier list, name, scope, default shortcuts, menu group, eligible-action list, factory visibility), two `match` blocks in `workspace.rs` (availability and invocation), and one hard-coded action list in the C++ Shortcuts page. The owner chose the full table.

`crates/dual-pane-application/src/actions.rs` holds one `ActionSpec` row per `ActionId`, in declaration order:

- `name`, `scope`, `category` (the Shortcuts page section), and the factory `shortcuts` (the first is the primary one).
- For actions eligible for a Folder Items Context Menu, a `FolderItemSpec`: `group`, row `position`, `normal_default` visibility, `target` (the selected Items, or the Folder, which also works from empty space), `needs_other_browser`, a `MenuAvailability` rule, and a `MenuInvocation`.

What reads it:

- `ActionId::ALL`, `as_str`, `scope`, `default_shortcut`, `default_shortcuts`, `context_menu_group`, `CONTEXT_MENU_ACTIONS`, and the factory `ContextMenuPreferences` derive from the table, so their public signatures are unchanged.
- `Workspace` builds the menu, decides availability, and dispatches an invoked action by interpreting the row's availability and invocation descriptors. It has one case per descriptor kind, not one per action.
- The Shortcuts page asks the bridge for each action's category (`actionCategory`) instead of keeping its own list of action names.
- Storage, the Folder Items model, and the Context Menus page needed no change: they already consume the derived `CONTEXT_MENU_ACTIONS` and the menu snapshot.

Adding a thirteenth action is one new `ActionId` variant, one row, and, only when the action needs a new kind of availability rule or invocation, one new case in the interpreters in `workspace.rs`. A new action that needs new work also adds that work (the `Command`, `Output`, and native effect), as `b0e3ffb` did. The table does not change the schema-11 persisted format, the `ActionId` order or values, or any Settings or menu behavior. Compile-time assertions check that rows follow the `ActionId` order and that menu positions are unique, dense, and follow the fixed group order.

## Verification

Since behavior does not change, verification is the existing full gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, all five native scripts (`check-settings-window.sh`, `check-text-editor-focus.sh`, `check-application-title-row.sh`, `check-folder-items-context-menu.sh`, `check-single-instance.sh`), and `make check`. Passing these without modification to their assertions is the proof that the split and the catalogue preserved behavior. No new tests were required.
