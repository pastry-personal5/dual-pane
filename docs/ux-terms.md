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
| Rename Item Editor | `rename_item_editor` |
| New Folder Name Editor | `new_folder_name_editor` |
| Operation Panel Strip | `operation_panel_strip` |
| Operation Panel | `operation_panel` |
| Operation Progress Indicator | `operation_progress_indicator` |
| Cancel Operation Button | `cancel_operation_button` |
| Operation Decision Card | `operation_decision_card` |
| Permanent Delete Confirmation Window | `permanent_delete_confirmation_window` |
| Notices | `notices` |
| Notices Button | `notices_button` |
| Notice | `notice` |
| Notices Startup Checkbox | `notices_startup_checkbox` |
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
| Close Window | `CloseWindow` | Catalogued; current command |
| Quit Application | `QuitApplication` | Catalogued; current command |
| New Folder | `NewFolder` | Catalogued; planned command |
| Sort by Name Ascending | `SortByNameAscending` | Catalogued; planned command |
| Sort by Name Descending | `SortByNameDescending` | Catalogued; planned command |
| Sort by Type Ascending | `SortByTypeAscending` | Catalogued; planned command |
| Sort by Type Descending | `SortByTypeDescending` | Catalogued; planned command |
| Sort by Date Ascending | `SortByDateAscending` | Catalogued; planned command |
| Sort by Date Descending | `SortByDateDescending` | Catalogued; planned command |
| Sort by Size Ascending | `SortBySizeAscending` | Catalogued; planned command |
| Sort by Size Descending | `SortBySizeDescending` | Catalogued; planned command |
| New Tab | `NewTab` | Planned ID |
| Close Tab | `CloseTab` | Planned ID |
| Back | `NavigateBack` | Planned ID |
| Forward | `NavigateForward` | Planned ID |
| Refresh Folder | `RefreshFolder` | Planned ID |
| Copy to Other Browser | `CopyToOtherBrowser` | Planned ID |
| Move to Other Browser | `MoveToOtherBrowser` | Planned ID |
| Rename Item | `RenameItem` | Planned ID |
| Move to Trash | `MoveToTrash` | Planned ID |
| Delete Permanently | `DeletePermanently` | Planned ID |

## Deferred components

Browser Tabs Strip is currently rendered as an inert current-folder label only. Favorite Groups, Favorite Items, the Relative Date Column, and the Exact Date Column are not rendered as interactive components by the current desktop UI. Their planned GUI behavior is in [ux-gui.md](ux-gui.md#planned-phase-3-gui-decisions). The Sidebar currently displays static group labels; its navigation and editing behavior is deferred to a later Phase 3 milestone.
