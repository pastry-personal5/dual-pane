# P1-M5 overview: Real directory browsing in one pane

Status: Proposal

P1-M5 replaces the synthetic listing with a real macOS directory reader and connects one pane's selection and navigation behavior to a dark Qt window. Directory enumeration, filtering, entry classification, and sorting stay on the worker; the GUI thread handles transient hover styling, submits action-named inputs, and applies completed state or errors.

## Goal

Launch into a real directory listing with Phase 1's dark theme and mouse interactions: hover a row, select one row with one left click, enter a folder with a double left click, and return to the logical parent with the Up button. If a directory cannot be read because of ordinary permissions or macOS privacy controls, keep the last successful listing and show the reason to the person without blocking the interface.

## Scope

**In:**

- A Qt-free `directory_listing.rs` gateway in `dual-pane-desktop` that converts a domain `Location` to an exact native path, enumerates it through `std::fs::read_dir`, classifies files, folders, other entries, and symbolic links to folders, defensively filters the exact entries `.` and `..` plus entries that disappear during the scan, sorts with `listing_sort_key`, and supplies fresh instances through the existing `ListingSourceFactory` seam. Other dot-prefixed entries remain visible.
- All enumeration, per-entry metadata work, filtering, and sorting inside the existing supervised listing worker. Cancellation is checked before the read, during enumeration and classification, and before and after sorting.
- The already approved `tempfile` dev-dependency in `dual-pane-desktop`, used only for reader tests. Tests create all filesystem state below temporary directories.
- The reverse byte-exact conversion `Location` to `PathBuf` in `native_location.rs`, alongside the existing `Path` to `Location` conversion.
- Single-selection state owned by the application: one left click selects that exact entry and replaces the prior selection; loading keeps the old listing and selection, a failed navigation preserves both, and a successful directory change clears selection.
- Mouse handling in the Qt view: moving over an unselected row changes its background to the hover color, one left click selects the row, and a double left click submits activation after selection. Activation enters only a folder or a symbolic link to a folder; double-clicking any other row only leaves it selected.
- A shortcut-free `QToolButton` directly above the item list, using Qt's standard up-arrow icon. Clicking it goes to the current location's logical parent; hovering it uses the same background color as a selected row. Selection and navigation cross the existing `UiEvent` -> `Command` boundary.
- A scoped Phase 1 dark theme for the window, button strip, item list, status area, ordinary text, row hover, row selection in active and inactive focus states, and button hover. Theme colors are centralized in the Qt shim rather than scattered through widget setup.
- Mapping native listing failures to `ListingErrorKind`, including macOS `EPERM` as `PrivacyRestricted` and POSIX `EACCES` as `PermissionDenied`.
- Visible, non-blocking error presentation in the existing status area. A failed child navigation keeps the prior location and rows; the next navigation clears the error.
- Removal of P1-M4's synthetic source, startup constant, wiring, and tests.

**Out:**

- Concrete key bindings. P1-M5 does not connect Qt's platform-dependent `activated` signal or assign a shortcut; the key-binding decision in [AGENTS.md](../../AGENTS.md#undecided--ask-before-inventing) remains open.
- User-selectable hidden-file or name filtering and sort controls. This milestone omits only the exact navigation pseudo-entries `.` and `..`; other dot-prefixed entries remain visible. Its other worker-side filter omits an entry that vanished between enumeration and classification. Later filter specifications must travel in the application-owned read request and still execute on a worker.
- Retry controls, a System Settings deep link, or a blocking error dialog. The status area reports the terminal listing error without entering a nested event loop or preventing another navigation.
- Opening regular files, multi-selection, range selection, deselection by clicking empty space, icons, metadata columns, two panes, tabs, watching, persistence, row deltas, or file operations.
- Any new dependency other than the previously approved `tempfile` dev-dependency.

## Decisions carried forward

- A failed navigation keeps the last successful directory and reports the attempted logical location; a first-load failure leaves the pane empty and reports the error.
- Listings place folders and links to folders first, then use the existing case-insensitive natural order with exact bytes as the final tie-breaker.
- Entry names and native paths remain byte-exact. Lossy text exists only at the presentation boundary.
- Entering a symbolic link to a folder appends the link name to the logical location. Up therefore returns to the folder that contained the link, not the resolved target's parent.
- A stale result or stale failure remains a no-op, and a newer navigation cancels the older read.
- Hover is transient Qt presentation state. Selection is behaviorally meaningful state identified by the entry's exact name and remains owned by the application, with the presenter exposing its current row to Qt.
- The Phase 1 theme uses a dark neutral surface, light text, a distinct dark hover background, and a blue selection background. A selected row keeps the selection color while hovered and while the view is inactive; the Up button's hover color is that same selection blue.

## Completion checklist

- [ ] `dual-pane-desktop` has a real directory gateway and no synthetic source. The composition root passes that gateway to the unchanged bounded runtime and starts at the launch working directory, falling back to `/` only when the working directory cannot be obtained or represented. Evidence: `main.rs`, `directory_listing.rs`, and focused desktop tests.
- [ ] Reader tests in a `tempfile` directory prove byte-exact names (including invalid UTF-8), regular files, directories, other entry kinds, links to directories, links to files, dangling links, ordinary dot-prefixed entries, and `listing_sort_key` order. Focused filter tests prove that the exact names `.` and `..` can never become rows and that a per-entry `NotFound` race is filtered while any other per-entry failure fails the whole read. Evidence: `cargo test -q -p dual-pane-desktop directory_listing`.
- [ ] Reader and runtime tests prove that enumeration, classification, filtering, and sorting run on the listing worker; a pre-cancelled read returns no result; cancellation checks occur throughout the scan; and neither the GUI thread nor the presenter performs filesystem work or whole-list sorting. Evidence: focused `directory_listing` and `runtime` tests plus boundary review.
- [ ] Native-location tests prove round trips between absolute `Path` values and `Location`, including `/`, spaces, dot-prefixed names, and names that are not valid UTF-8; relative paths and parent components remain rejected. Evidence: `cargo test -q -p dual-pane-desktop native_location`.
- [ ] Error-mapping tests prove `ENOENT`/`NotFound` -> `ItemMissing`, `ENOTDIR`/`NotADirectory` -> `NotADirectory`, `EACCES` -> `PermissionDenied`, `EPERM` -> `PrivacyRestricted`, and an unclassified error -> `Unknown`. Iterator and per-entry errors do not publish a partial listing. Evidence: `cargo test -q -p dual-pane-desktop directory_listing`.
- [ ] Domain, application, and adapter tests prove single-selection behavior: selecting a listed row stores its exact name and replaces any prior selection; an invalid row/name is ignored; selecting during a pending read remains possible; failure and cancellation preserve selection; a successful listing replacement clears it; and the presenter exposes the matching selected row. Evidence: focused Qt-free crate tests.
- [ ] Pane-session tests prove that row selection, row activation, and Up pass through `InputController`, issue the expected application commands and worker requests, preserve exact entry names, do nothing for an invalid row or Up at `/`, cancel a superseded read, and apply only the newest result. Evidence: `cargo test -q -p dual-pane-desktop pane_session` and the inner-crate selection/navigation suites.
- [ ] The Qt shim configures single-row selection, connects `QTreeView::clicked` only to selection, connects `QTreeView::doubleClicked` to activation, and connects a shortcut-free up-arrow `QToolButton` immediately above the list to `UiEvent::GoToParent`. No concrete key or platform-dependent activation binding is introduced. Evidence: C++/bridge review and `make lint-cpp`.
- [ ] The dark theme visibly distinguishes the normal, hovered, and selected row backgrounds; selection wins over hover and remains visible without focus; the Up button uses the selected-row background on hover; and text remains legible in each state. Evidence: centralized stylesheet review and manual launch evidence.
- [ ] Permission and privacy failures use the presenter's existing distinct wording, remain visible without blocking the event loop, keep the last successful rows and location, and clear when another navigation begins. Evidence: adapter and pane-session tests plus manual launch evidence.
- [ ] Manual launch evidence: from a disposable test hierarchy, `make run` shows the real launch directory without `.` or `..`; moving the pointer over a row changes only its hover background; one left click selects exactly that row; selecting another row moves the selection; double left click enters a real folder but not a file; the up-arrow button sits directly above the list, uses the selection color on hover, and returns to the logical parent; a link to a directory can be entered and Up returns through the link path; an unreadable directory leaves the prior listing and selection visible with an error; the dark theme remains legible and the window responsive; and closing it returns the process normally. Record the result in [changelog.md](changelog.md).
- [ ] No dead, demo, or stray code remains: `synthetic_listing.rs`, `SYNTHETIC_ROWS`, and their tests are deleted; no `#[allow(dead_code)]` or `#[allow(unused...)]` is added; and `git status --short` contains only intended files. Evidence: `rg -n -i 'synthetic' crates`, `rg -n 'allow\((dead_code|unused)' crates/dual-pane-desktop`, and `git status --short` print no unexpected matches.
- [ ] Dependency changes are limited to the approved `tempfile` dev-dependency, and every newly locked package has an Apache-2.0-compatible license. Evidence: `Cargo.toml`, `crates/dual-pane-desktop/Cargo.toml`, and the dependency/lockfile review.
- [ ] No source-code name uses a phase word, and no source code or script uses `MVP` wording. Evidence: `rg -n -i 'phase|mvp' crates scripts Makefile` prints nothing.
- [ ] `make check` and the equivalent direct commands (`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make lint-cpp`, and `cargo test`) each exit 0.
- [ ] Documentation is current: the project status in [AGENTS.md](../../AGENTS.md) and [README.md](../../README.md), the desktop tree in [planned-repository-architecture.md](../planned-repository-architecture.md), the milestone and phase status in [phase.md](phase.md) and [roadmap.md](../roadmap.md), and the acceptance evidence in [changelog.md](changelog.md) all describe the implemented result.
