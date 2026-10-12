# P3-M3 architecture

Status: Done

P3-M3 is a desktop-framework presentation refactor. It keeps the existing Folder Items model, application commands, runtime drain scheduling, and Browser-local selection intact.

## Native composition

Private C++ widgets own the static Sidebar and bottom Main Toolbar, Browser Tabs Strip, Folder Pane summary row and command row, Browser containment, and window coordination. Their Qt parent ownership remains local to the composition root. The Browser binds the model's `folderNameChanged` signal to both its Row #1 label and its Browser Tabs Strip label, so the model property remains the only tab-label data source. The existing path, status, and Up Button bindings are unchanged.

The Main Toolbar's Settings glyph is a checked-in private UI asset. Its disabled, non-focusable button has no connection and therefore cannot issue a domain or application command. The static Drives and Favorites labels likewise have no settings or Favorites-state connection.

## Deferred summary and controls

Row #2 reserves a summary display. P3-M5/P3-M6 will populate it after a successful listing with `N items`, an optional `X selected`, and selected total or folder total. Totals are non-recursive sums of known direct visible Folder Item sizes, never scans; an incompletely known total is `—`. The values remain paired with the last successful visible Folder Items during pending or failed navigation and are absent before the first successful listing. P3-M4 decides any multi-selection expansion and gestures.

Row #3 keeps the existing Up command path and adds disabled adjacent ascending/descending carets for Name, Type, Date, and Size. P3-M6 will enable, hide, and connect those controls under the later sorting model.
