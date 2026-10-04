# UX terms

Status: Active

This is the canonical vocabulary for visible Dual Pane components, shortcut-capable actions, and their code-facing names. Use the exact component terms in UI text and accessibility-visible names; use the mapped lower-camel or snake-case names in implementation identifiers when a component is represented there.

## Canonical components

| UX term | Code-name mapping |
|---|---|
| Standard Layout | `standard_layout` |
| Sidebar | `sidebar` |
| Main Toolbar | `main_toolbar` |
| Settings | `settings` |
| Favorites Groups | `favorites_groups` |
| Favorite Group | `favorite_group` |
| Favorite Group Name | `favorite_group_name` |
| Favorite Group Menu Button | `favorite_group_menu_button` |
| Favorite Group Menu | `favorite_group_menu` |
| New Group Button | `new_group_button` |
| Favorite Item | `favorite_item` |
| Favorite Item Alias | `favorite_item_alias` |
| Favorite Item Context Menu | `favorite_item_context_menu` |
| Add Favorite Item Button | `add_favorite_item_button` |
| Favorite Group Name Editor | `favorite_group_name_editor` |
| Favorite Item Alias Editor | `favorite_item_alias_editor` |
| Sidebar Splitter | `sidebar_splitter` |
| Browser Divider | `browser_divider` |
| Left Browser / Right Browser | `BrowserSide::Left` / `BrowserSide::Right` |
| Navigation Pane | `navigation_pane` |
| Browser Tabs Strip | `browser_tabs_strip` |
| Browser Tab | `browser_tab` |
| New Tab Button | `new_tab_button` |
| Close Tab Button | `close_tab_button` |
| Back Button | `back_button` |
| Forward Button | `forward_button` |
| Path Edit Control | `path_edit_control` |
| Folder Pane | `folder_pane` |
| Folder Pane Toolbar Row #1 | `folder_pane_toolbar_row_1` |
| Folder Pane Toolbar Row #2 | `folder_pane_toolbar_row_2` |
| Folder Pane Toolbar Row #3 | `folder_pane_toolbar_row_3` |
| Up Button | `up_button` |
| Name Ascending Sort Button | `name_ascending_sort_button` |
| Name Descending Sort Button | `name_descending_sort_button` |
| Type Ascending Sort Button | `type_ascending_sort_button` |
| Type Descending Sort Button | `type_descending_sort_button` |
| Date Ascending Sort Button | `date_ascending_sort_button` |
| Date Descending Sort Button | `date_descending_sort_button` |
| Size Ascending Sort Button | `size_ascending_sort_button` |
| Size Descending Sort Button | `size_descending_sort_button` |
| Folder Items List | `folder_items_list` |
| Folder Items Context Menu | `folder_items_context_menu` |
| Missing Folder Overlay | `missing_folder_overlay` |
| Rename Item Editor | `rename_item_editor` |
| New Folder Name Editor | `new_folder_name_editor` |
| Operation Panel Strip | `operation_panel_strip` |
| Operation Panel | `operation_panel` |
| Operation Progress Indicator | `operation_progress_indicator` |
| Cancel Operation Button | `cancel_operation_button` |
| Operation Decision Card | `operation_decision_card` |
| Permanent Delete Confirmation Window | `permanent_delete_confirmation_window` |
| Quit Confirmation Window | `quit_confirmation_window` |
| Waiting Window | `waiting_window` |
| Notices | `notices` |
| Notices Button | `notices_button` |
| Notice | `notice` |
| Notices Startup Checkbox | `notices_startup_checkbox` |
| Quick Look Panel | `quick_look_panel` |
| Item Icon Column | `item_icon_column` |
| Relative Date Column | `relative_date_column` |
| Exact Date Column | `exact_date_column` |
| Browser Status Bar | `browser_status_bar` |

`Browser` replaces the former logical side-specific component term: `BrowserSide`, `BrowserState`, `BrowserStartup`, `BrowserPresenter`, and `BrowserViewModel` are the corresponding code names. `Folder Items` names a displayed directory result: `FolderItems`, `FolderItemsListModel`, and `FolderItemsList` are the corresponding code names.

`Favorite Item` remains the entity name. `Favorite Item Alias` names only its displayed label.

Mapped legacy component terms are not used in source or tests. The product and crate namespace `dual-pane` / `dual_pane` is exempt, as are technical primitives that do not name a UX component, such as directory-reading errors and entry sort keys.

## Canonical actions

Action IDs are stable UpperCamelCase compatibility keys for shortcut-capable commands. "Catalogued" means the ID already exists in the persisted action catalogue; "planned ID" reserves a name here and does not add it to that catalogue. A catalogued action can still have a planned command. List-movement gestures and contextual Favorite commands are outside this table.

| Visible name | Action ID | Status |
|---|---|---|
| Focus Other Browser | `FocusOtherBrowser` | Catalogued; current command |
| Navigate Parent | `NavigateParent` | Catalogued; current command |
| Close Window | `CloseWindow` | Catalogued; current command, no default shortcut |
| Quit Application | `QuitApplication` | Catalogued; current command |
| New Folder | `NewFolder` | Catalogued; current command |
| Sort by Name Ascending | `SortByNameAscending` | Catalogued; current command |
| Sort by Name Descending | `SortByNameDescending` | Catalogued; current command |
| Sort by Type Ascending | `SortByTypeAscending` | Catalogued; current command |
| Sort by Type Descending | `SortByTypeDescending` | Catalogued; current command |
| Sort by Date Ascending | `SortByDateAscending` | Catalogued; current command |
| Sort by Date Descending | `SortByDateDescending` | Catalogued; current command |
| Sort by Size Ascending | `SortBySizeAscending` | Catalogued; current command |
| Sort by Size Descending | `SortBySizeDescending` | Catalogued; current command |
| New Tab | `NewTab` | Catalogued; current command |
| Close Tab | `CloseTab` | Catalogued; current command |
| Back | `NavigateBack` | Catalogued; current command |
| Forward | `NavigateForward` | Catalogued; current command |
| Refresh Folder | `RefreshFolder` | Catalogued; current command |
| Copy to Other Browser | `CopyToOtherBrowser` | Catalogued; current command |
| Move to Other Browser | `MoveToOtherBrowser` | Catalogued; current command |
| Rename Item | `RenameItem` | Catalogued; current command |
| Move to Trash | `MoveToTrash` | Catalogued; current command |
| Delete Permanently | `DeletePermanently` | Catalogued; current command |
| Show Package Contents | `ShowPackageContents` | Catalogued; current command, no default shortcut |

## Deferred components

Since P3-M6, the development build renders the Browser Tabs Strip, Browser Tabs, Sidebar Favorites, the six Folder Items columns, the eight sort buttons, the Notices Button, and Notices. Their accessible names use the terms above; each sort button's accessible name and tooltip read like "Sort items by type (A-Z)". Since P3-M8, it also renders the Folder Items Context Menu, Rename Item Editor, New Folder Name Editor, Missing Folder Overlay, Operation Panel components, Permanent Delete Confirmation Window, and Quit Confirmation Window under these names. The active P3-M9 development build also renders the Notices Startup Checkbox and Waiting Window; their behavior is in [ux-gui.md](ux-gui.md#planned-phase-3-gui-decisions). P3-M11 adds the system-owned Quick Look Panel.
