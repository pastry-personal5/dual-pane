# UX terms

Status: Active

This is the canonical vocabulary for visible Dual Pane components and their code-facing names. Use the exact component terms in UI text and accessibility-visible names; use the mapped lower-camel or snake-case names in implementation identifiers when a component is represented there.

## Canonical components

| UX term | Code-name mapping |
|---|---|
| Standard Layout | `standard_layout` |
| Sidebar | `sidebar` |
| Sidebar Splitter | `sidebar_splitter` |
| Browser Divider | `browser_divider` |
| Left Browser / Right Browser | `BrowserSide::Left` / `BrowserSide::Right` |
| Navigation Pane | `navigation_pane` |
| Browser Tabs | `browser_tabs` |
| Path Edit Control | `path_edit_control` |
| Folder Pane | `folder_pane` |
| Folder Pane Toolbar Row #1 | `folder_pane_toolbar_row_1` |
| Folder Pane Toolbar Row #2 | `folder_pane_toolbar_row_2` |
| Up Button | `up_button` |
| Folder Items List | `folder_items_list` |
| Browser Status Bar | `browser_status_bar` |

`Browser` replaces the former logical side-specific component term: `BrowserSide`, `BrowserState`, `BrowserStartup`, `BrowserPresenter`, and `BrowserViewModel` are the corresponding code names. `Folder Items` names a displayed directory result: `FolderItems`, `FolderItemsListModel`, and `FolderItemsList` are the corresponding code names.

Mapped legacy component terms are not used in source or tests. The product and crate namespace `dual-pane` / `dual_pane` is exempt, as are technical primitives that do not name a UX component, such as directory-reading errors and entry sort keys.

## Deferred components

Browser Tabs are not rendered by the current desktop UI. The Sidebar currently displays static group labels, but Sidebar navigation behavior is deferred to a later Phase 3 milestone.
