# P3-M3: UX Design and refactoring

Status: Done

This milestone refactors the native desktop shell into the revised Standard Layout without changing navigation, runtime scheduling, selection, or CXX-Qt interfaces. It adds presentation structure only.

## Scope

In scope: inert Browser Tabs Strip, static Sidebar and Main Toolbar composition, a three-row Folder Pane, a packaged local Settings glyph, disabled sort controls, UX documentation, and verification. Out of scope: tab behavior, Settings interaction, Sidebar Favorites data, live Folder Items metrics, sorting, and any model or runtime change.

## Completion checklist

- [x] The active UX architecture, GUI rules, and terms define the Main Toolbar, disabled Settings control, inert Browser Tabs Strip, and all three Folder Pane toolbar rows.
- [x] The native shell is composed from private Sidebar, Main Toolbar, Browser, Browser Tabs Strip, Folder Pane, summary-row, command-row, and window-coordination components, without changing Rust or CXX-Qt interfaces.
- [x] Each Browser Tabs Strip is label-only and its sole data source is that Browser model's current folder-name property.
- [x] The Sidebar retains static Drives and Favorites labels; its bottom Main Toolbar has one local-glyph Settings button that is disabled, non-focusable, named `Settings`, and emits no command.
- [x] Folder Pane Toolbar Row #2 is reserved, and Row #3 retains the functional Up Button plus eight disabled, adjacent Name/Type/Date/Size ascending-descending carets with descriptive accessibility text.
- [x] Existing workspace, navigation, selection, runtime, and presenter coverage remains green; `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed.
- [x] Native launch was attempted after the full gate. Visual verification is deferred because the environment reported `Cannot create window: no screens available`.
