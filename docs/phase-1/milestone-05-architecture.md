# P1-M5 architecture: Real directory browsing in one pane

Status: Proposal

P1-M5 supplies the real filesystem gateway that P1-M4's `ListingSource` seam was built to receive, adds application-owned single selection, and connects selection and navigation to a dark Qt pane. It does not move policy outward or replace the worker runtime: native paths and errors stay in `dual-pane-desktop`, selection and navigation state stay in `Workspace`, and hover plus widgets remain C++/CXX-Qt concerns.

## Target boundary

```text
crates/dual-pane-domain/src/
└── selection.rs               # one exact selected entry, or none
crates/dual-pane-application/src/
├── input.rs                   # + SelectEntry
├── output.rs                  # + SelectionChanged
└── workspace.rs               # committed-list selection owner and invariants
crates/dual-pane-adapters/src/
├── input_controller.rs        # + SelectRow mapping
└── pane_presenter.rs          # + selected exact name and selected row
crates/dual-pane-desktop/
├── Cargo.toml                  # + approved tempfile dev-dependency
├── src/
│   ├── main.rs                 # composition root: start location, real source, run
│   ├── directory_listing.rs    # Qt-free native reader, classification, filter, sort, error map
│   ├── native_location.rs      # byte-exact Path <-> Location conversion
│   ├── pane_session.rs         # + selection/navigation UI submission
│   ├── listing_model.rs        # + selected-row property and mouse-action invokables
│   └── runtime.rs              # retained: supervised worker, cancellation, delivery
└── cpp/src/desktop_window.cpp  # + mouse wiring, up-arrow button, and dark stylesheet
```

The inner crates gain only the selection surface this GUI behavior requires: a domain `Selection` holding at most one exact `EntryName`; application select input, output, and workspace state; and adapter row-selection input and selected-row presentation. `synthetic_listing.rs` is deleted.

## End-to-end flow

```text
left-click row / double-left-click row / click Up
  -> ListingModel bridge method
  -> PaneSession::submit_ui(UiEvent)
  -> InputController -> Command
  -> Workspace::handle
  -> WorkRequest::ReadDirectory or Cancel
  -> Runtime listing worker
  -> directory_listing::read
  -> Event::ListingLoaded / ListingFailed / ListingCancelled
  -> bounded GUI drain
  -> Workspace -> PanePresenter
  -> selection sync, model reset, or status update
```

The old listing remains the model's immutable backing data while a child or parent read is pending. Only a matching successful result swaps rows; a matching failure changes status, and any stale terminal event is ignored by the workspace.

## Native location conversion

`native_location.rs` adds `path_from_location(&Location) -> PathBuf`. It starts at `/` and appends each component with `OsStr::from_bytes`, so invalid UTF-8 is not normalized or replaced. Because `EntryName` excludes empty names, `/`, NUL, `.` and `..`, every domain `Location` can be converted without I/O or fallible text encoding.

Round-trip tests cover `location_from_path(path_from_location(location)) == Some(location)` for root and representative byte sequences. This conversion runs on the worker as part of a read; the startup-only `current_dir` conversion remains in the composition root.

## Directory gateway (`directory_listing.rs`)

The gateway exposes a function with the existing source shape:

```text
read(
  location: &Location,
  cancelled: &AtomicBool,
) -> Option<Result<Arc<[Entry]>, ListingErrorKind>>
```

`None` means cancellation won the read. `Some(Ok(...))` contains a complete, filtered, sorted snapshot, and `Some(Err(...))` contains only an application-owned error category. Native paths, `DirEntry`, `Metadata`, and `io::Error` never cross this module.

The worker performs these steps:

1. Check cancellation and convert the logical location to a native path.
2. Call `std::fs::read_dir`. Treat an initial error as a failed listing.
3. Iterate `io::Result<DirEntry>` values. Check cancellation before advancing and before any additional metadata lookup. An iterator error fails the listing; it never yields a partial success.
4. Read `DirEntry::file_name()` as exact `OsStr` bytes and defensively filter byte-exact `.` and `..`, even though `std::fs::read_dir` already omits them. Convert every other name to `EntryName`; failure is an internal invariant breach, not a lossy conversion or silently renamed row.
5. Call `DirEntry::file_type()`, which does not follow links. A directory becomes `Directory`, a regular file becomes `File`, and an unrecognized native kind becomes `Other`.
6. For a symbolic link, call `std::fs::metadata(entry.path())`, which follows the link. A directory target becomes `Symlink { points_to_directory: true }`; a regular or other target becomes `Symlink { points_to_directory: false }`. `NotFound` or `NotADirectory` while following the target means a dangling, non-folder link; any other target lookup error fails the listing with its mapped category rather than guessing the entry's behavior.
7. If an entry disappears between enumeration and classification, omit it. Exact `.`/`..` removal and this race handling are P1-M5's only listing filters. Other dot-prefixed names and stable `Other` entries remain visible; user-controlled filter specifications are deferred.
8. Check cancellation, sort the retained entries with `sort_by_cached_key(listing_sort_key)`, check cancellation again, and convert the vector to `Arc<[Entry]>`.

These operations happen because `Runtime` invokes the source only inside its listing worker. Neither `ListingModel::data`, `PaneSession`, `Workspace`, nor the presenter performs enumeration, metadata calls, filtering, or sorting.

## Error classification

The gateway maps errors at the native boundary. It checks the raw macOS errno before the portable `ErrorKind`, because Rust otherwise groups both of the access cases under `PermissionDenied`.

| Native condition | `ListingErrorKind` | Reason |
|---|---|---|
| `EPERM` (`1`, operation not permitted) | `PrivacyRestricted` | macOS Files & Folders/TCC denial is reported this way for directory access. |
| `EACCES` (`13`) | `PermissionDenied` | POSIX mode bits or an ACL denied traversal/read access. |
| `NotFound` / `ENOENT` | `ItemMissing` | The requested location disappeared or never existed. |
| `NotADirectory` / `ENOTDIR` | `NotADirectory` | A logical component no longer names a folder. |
| Other expected I/O failures | `Unknown` | The application has no different action for them in this milestone. |

`Internal` remains reserved for the runtime losing a worker, a source panic, or a driver invariant failure. The raw error and path remain driver-local; user wording continues to come exclusively from `PanePresenter`.

The reader tests construct raw errno values to prove the `EPERM`/`EACCES` split without depending on a developer machine's current privacy grants. Real directory tests cover missing and non-directory paths. A manual denial scenario proves that the mapped result reaches the visible status area while the prior listing remains usable.

## Single-selection state

Hover does not cross the Qt boundary: it has no behavioral effect and disappears as soon as the pointer leaves. Selection does cross the boundary because later actions must have one authoritative answer to which entry is selected.

The domain adds a small `Selection` value holding `Option<EntryName>`. The application owns it beside the committed listing and enforces these rules:

- `Command::SelectEntry(EntryName)` selects only a byte-exact name in the committed listing;
- selecting another entry replaces the previous selection, while selecting the same entry is a no-op;
- selection may change while a navigation read is pending because the committed listing remains interactive;
- `LoadingStarted`, `ListingFailed`, and `ListingCancelled` preserve selection;
- a matching `ListingLoaded` clears selection when it commits the replacement listing;
- stale events never change selection.

`Output::SelectionChanged` carries the application-owned `Selection`. `PanePresenter` retains the selected exact name, clears it on `ListingReplaced`, and exposes `selected_row() -> Option<usize>` by finding that name in its shared entry snapshot. The row is presentation data only; the exact name remains authoritative.

The adapter adds `UiEvent::SelectRow { row }`. `InputController` maps a displayed row to `Command::SelectEntry` using the entry's exact name, just as activation already maps by exact name. It returns no command for an invalid row.

## Pane session and input boundary

`PaneSession` gains an `InputController` and `submit_ui(UiEvent) -> ViewChange`. It asks the controller for a command using the presenter's current `PaneViewModel`; an invalid row returns `ViewChange::None`. A valid command goes through the existing `submit`, so work dispatch, stale-token handling, presentation, and Qt notification selection keep one path.

`ViewChange` adds `Selection` between `Status` and `Reset`. A selection-only transition changes no rows and emits no model reset; it only synchronizes the view's selection model. A listing replacement still dominates other changes and clears the selection after the reset.

This method is tested with the existing deterministic `FakeRunner`:

- selecting a displayed row records its exact-byte entry and reports a selection change without dispatching work;
- selecting another row replaces the prior selection, and an invalid row changes nothing;
- activating a displayed folder dispatches a read for its exact-byte child location;
- activating a file or an invalid row dispatches nothing;
- Up dispatches the logical parent read and is a no-op at root;
- activating a new destination while another loads dispatches `Cancel` before the new `ReadDirectory`;
- a failed destination keeps the shown rows and selection and changes only status;
- a successful replacement reports a reset and clears selection.

## Qt delivery

`listing_model.rs` exposes an `i32 selectedRow` property, using `-1` for no selection, and three GUI-thread bridge methods. `ListingModelRust` uses an explicit `Default` implementation so the property starts at `-1`, never at Rust's integer default of row zero:

- `select_row(row: i32)` validates the conversion to `usize`, submits `UiEvent::SelectRow`, and applies the returned `ViewChange`.
- `activate_row(row: i32)` validates the conversion to `usize`, submits `UiEvent::ActivateRow`, and applies the returned `ViewChange`.
- `go_to_parent()` submits `UiEvent::GoToParent` and applies the returned change.

The C++ window places a compact button row and the `QTreeView` in a vertical central layout, so the Up button is immediately above the item list. The button is a `QToolButton` with `QStyle::SP_ArrowUp`, an accessible name and tooltip of `Up`, and no shortcut. Its `clicked` signal calls `go_to_parent()`.

The view uses `SingleSelection` and `SelectRows`, has editing disabled, and enables mouse tracking. Its mouse connections have separate responsibilities:

- `QTreeView::clicked(const QModelIndex&)` submits `select_row(index.row())`. Qt emits this signal for a valid item left click.
- `QTreeView::doubleClicked(const QModelIndex&)` submits `activate_row(index.row())`. The ordinary click path selects the row first; the application then enters it only if it is enterable.
- `selectedRowChanged` updates the view's `QItemSelectionModel` with one whole-row selection, or clears it for `-1`. Programmatic synchronization does not submit another application command.

This keeps the application authoritative even though Qt paints the selection immediately during the click. A successful model reset clears both application and Qt selection; a failed navigation performs no reset and preserves it.

The plan intentionally does not use `QAbstractItemView::activated`: Qt defines that signal in terms of platform activation, which can include Return or Enter. Avoiding it keeps P1-M5 from deciding a concrete key binding indirectly. The adapter events and application commands remain action-named, so a later key-binding design can invoke the same path.

## Phase 1 dark theme and hover

The C++ shim owns a small, centralized set of Phase 1 color constants and applies one scoped Qt style sheet. A style sheet is used instead of relying only on `QPalette`, because native macOS widget styles may ignore palette roles.

| Token | Initial color | Use |
|---|---|---|
| Window | `#1B1D21` | Main window and status background. |
| Surface | `#23262B` | Button strip and item list. |
| Text | `#ECEFF3` | Normal row, button, and status text. |
| Muted border | `#3A4048` | Subtle button and pane boundaries. |
| Row hover | `#303640` | Unselected row under the pointer. |
| Selection | `#2F6D9A` | Selected row and hovered Up button. |
| Selection text | `#FFFFFF` | Text on the selection color. |

The item rules distinguish `QTreeView::item:hover:!selected`, `QTreeView::item:selected:active`, and `QTreeView::item:selected:!active`. The two selected states use the same selection and text colors, so selection remains visible when focus moves to the Up button; selection rules also win while the selected row is hovered.

The `QToolButton:hover` rule uses the same selection color and defines a border, because Qt's native tool-button border can otherwise cover a stylesheet background. Normal and pressed button states remain visibly dark. Hover changes only Qt rendering: it sends no adapter event, mutates no application state, and performs no model notification.

## Error presentation

No new dialog protocol is required. `PanePresenter` already turns `PermissionDenied` and `PrivacyRestricted` into distinct messages and keeps the previous location and rows after `ListingFailed`. `ListingModel::apply(ViewChange::Status)` copies that view and emits `statusTextChanged`, and the existing status `QLabel` displays the message.

This path is non-blocking, creates no nested event loop, and permits an immediate Up or different navigation. `LoadingStarted` clears the prior error, and the next successful listing restores the location text.

## Startup and shutdown

`main.rs` keeps the current start-location rule but replaces `SyntheticListing` with the real reader function in `PaneStartup`. There is no row-count configuration or demo data. Runtime startup, coalesced GUI wake-up, terminal-event accounting, panic recovery, and shutdown remain unchanged.

Dropping the window still detaches the scheduler before model destruction. Dropping the runtime marks outstanding reads cancelled and never joins a worker from the GUI thread.

## Tests and evidence

- **Gateway unit tests:** temporary files and folders, exact-byte names, all entry kinds, symlink targets, dangling links, exact `.`/`..` filtering, ordinary dot-prefixed names, a simulated removed-after-enumeration entry, sorted output, cancellation, and native-error mapping.
- **Native conversion unit tests:** absolute round trips without lossy text.
- **Inner-crate tests:** single-selection invariants, exact-name selection commands, preservation during pending/failed navigation, clearing on replacement, selected-row presentation, domain ordering, stale results, and error wording without Qt.
- **Pane-session unit tests:** selection, activation, and Up event mapping; selection-only view changes; work requests; failure preservation; and reset-time clearing with no Qt dependency.
- **C++ lint and review:** single-click and double-click connections, selected-row synchronization, hover state isolation, up-arrow button placement and accessibility, no shortcut, centralized dark styles, and GUI-thread-only model calls.
- **Manual macOS acceptance:** the three row visual states; one-click single selection; double-click folder-only navigation; up-arrow placement, hover, and navigation; no `.`/`..` rows; logical link navigation; visible denial with preserved selection; responsive dark window; and normal close from a disposable hierarchy.

No test reads, changes permissions on, or creates files in a real user location. Tests that need a path use `tempfile`; the real privacy denial is manual acceptance only and must not alter TCC state automatically.

## Implementation sequence

1. Add the approved desktop `tempfile` dev-dependency, implement the reverse native-location conversion, and add its focused tests.
2. Add `directory_listing.rs` and gateway tests for enumeration, exact `.`/`..` and race filtering, classification, sorting, cancellation, and error mapping.
3. Add domain single selection, application selection input/state/output, adapter row selection and selected-row presentation, and their Qt-free tests.
4. Swap the composition root to the real source and delete `synthetic_listing.rs` and every synthetic constant, import, and test.
5. Add `PaneSession::submit_ui`, selection view changes, the three bridge methods, selected-row synchronization, and their focused session tests.
6. Build the vertical pane layout with the up-arrow button directly above the tree, connect the separate click/double-click paths, and apply the centralized dark/hover/selection stylesheet.
7. Run the focused crate tests, then manual acceptance from a disposable hierarchy.
8. Run dead/demo/naming searches, `make check`, and each direct gate command once; update the current-status docs and record evidence before checking the overview items.

## Constraints and rejection criteria

- No filesystem call, full-list filter, or sort runs on the GUI thread.
- No worker touches Qt, presenter, or workspace state, and no Qt type crosses into an inner crate.
- A read returns one complete sorted snapshot, one typed failure, or cancellation; it never publishes a partial listing.
- Failed or stale navigation never replaces the last successful listing.
- Exact filename bytes survive listing, row activation, location construction, and native-path conversion.
- Exact `.` and `..` never become model rows; ordinary dot-prefixed names remain visible.
- Selection has one application owner and is identified by exact entry name; Qt owns only transient hover and the mechanical selection rendering synchronized from the model.
- One left click never navigates, a double left click enters only an enterable row, and clicking Up is the only Phase 1 parent-navigation pointer action.
- Normal, hover, selected-active, selected-inactive, and Up-button-hover states are distinguishable in the dark theme; the Up hover and selected row use the same background color.
- No synchronous worker wait, `QCoreApplication::processEvents`, nested event loop, blocking dialog, unbounded thread creation, or new cross-thread Qt access is introduced.
- No concrete key binding is selected.
- No unapproved dependency, dead/demo code, `#[allow(dead_code)]`, or unused suppression is introduced.
