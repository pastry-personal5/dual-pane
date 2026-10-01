# P3-M1 architecture

Status: Active

This milestone is a naming and documentation migration. It keeps domain ownership, reducer behavior, runtime scheduling, and CXX-Qt composition unchanged.

## Approach

`BrowserSide` flows through the domain, application, adapter, runtime, and desktop session boundary. Browser presentation state uses `BrowserState`, `BrowserPresenter`, and `BrowserViewModel`. A displayed result uses `FolderItems`; the CXX-Qt list bridge and C++ view use `FolderItemsListModel` and `FolderItemsList`.

Desktop module names follow those terms, while directory errors and entry-sort primitives remain technical names because they do not represent UX components. The native layout keeps its containment and signals but exposes canonical object and accessibility names.
