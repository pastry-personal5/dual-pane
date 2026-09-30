# P1-M5 architecture: Real directory browsing in one pane

Status: Done

P1-M5 supplies the real filesystem gateway that P1-M4's `ListingSourceFactory` seam was built to receive, adds application-owned single selection, and connects selection and navigation to a dark Qt pane. It does not move policy outward or replace the worker runtime: native paths and errors stay in `dual-pane-desktop`, selection and navigation state stay in `Workspace`, and hover plus widgets remain C++/CXX-Qt concerns.

## Target boundary

```text
crates/dual-pane-domain/src/
└── selection.rs               # one exact selected entry, or none
crates/dual-pane-application/src/
├── input.rs                   # + row/name selection and activation validation
├── output.rs                  # + SelectionChanged
└── workspace.rs               # committed-list selection owner and invariants
crates/dual-pane-adapters/src/
├── input_controller.rs        # + SelectRow mapping
└── pane_presenter.rs          # + selected exact name and selected row
crates/dual-pane-desktop/
├── Cargo.toml                  # + exact tempfile workspace dev-dependency opt-in
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
4. Read `DirEntry::file_name()` as exact `OsStr` bytes and defensively reject byte-exact `.` and `..`, even though `std::fs::read_dir` already omits them. Convert every other name to `EntryName`; an unexpected conversion failure returns `ListingErrorKind::Internal` as a driver invariant breach rather than panicking, performing a lossy conversion, or silently renaming the row.
5. Call `DirEntry::file_type()`, which does not follow links. A directory becomes `Directory`, a regular file becomes `File`, and an unrecognized native kind becomes `Other`.
6. For a symbolic link, call `std::fs::metadata(entry.path())`, which follows the link. A directory target becomes `Symlink { points_to_directory: true }`; a regular or other target becomes `Symlink { points_to_directory: false }`. Any target lookup failure—including permission, privacy, a link loop, `NotFound`, or `NotADirectory`—retains the entry as `Symlink { points_to_directory: false }`. The reader must not fail its containing directory or claim that an unclassified target is enterable.
7. If `DirEntry::file_type()` reports `NotFound` because the entry itself disappeared between enumeration and classification, omit that entry. Any other failure to classify the entry itself and any iterator error fail the complete read. Exact `.`/`..` rejection and this own-entry race handling are P1-M5's only listing filters. Other dot-prefixed names and stable `Other` entries remain visible; user-controlled filter specifications are deferred.
8. Check cancellation, compute cached `listing_sort_key` values and sort the retained entries, check cancellation again, and convert the vector to `Arc<[Entry]>`. The standard in-memory sort is not interruptible; cancellation before key construction and immediately after sorting bounds when its result may be published without pretending that the sort itself can be stopped.

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

This table governs errors that terminate a read. A symbolic-link target-probe error is consumed by entry classification and retains that link as non-enterable; it is not promoted to a terminal listing error.

The reader tests construct raw errno values to prove the `EPERM`/`EACCES` split without depending on a developer machine's current privacy grants. Real directory tests cover missing and non-directory paths. Presenter and pane-session tests carry both mapped denial categories to their distinct visible status messages while preserving the prior listing. Manual acceptance covers an ordinary unreadable child inside the disposable hierarchy; it does not require navigating into real user data or changing macOS privacy grants.

## Single-selection state

Hover does not cross the Qt boundary: it has no behavioral effect and disappears as soon as the pointer leaves. Selection does cross the boundary because later actions must have one authoritative answer to which entry is selected.

The domain adds a small `Selection` value holding `Option<EntryName>`. The application owns it beside the committed listing. UI-originated selection and activation commands use `Command::SelectEntry { row, name }` and `Command::OpenEntry { row, name }`: the row is an O(1) lookup hint while the exact name remains the authority. The application accepts the command only when `listing.entries.get(row)` has that exact name; an out-of-range row or a row/name mismatch is stale input and changes nothing. This replaces `OpenEntry(EntryName)` so pointer activation never performs an unbounded name scan on the GUI thread.

The application enforces these rules:

- a valid `Command::SelectEntry { row, name }` stores the byte-exact name from the committed listing;
- selecting another entry replaces the previous selection, while selecting the same entry is a no-op;
- selection may change while a navigation read is pending because the committed listing remains interactive;
- `LoadingStarted`, `ListingFailed`, and `ListingCancelled` preserve selection;
- a matching `ListingLoaded` clears selection when it commits the replacement listing;
- stale events never change selection.

`Output::SelectionChanged { selection, row }` carries the application-owned `Selection` plus `Some(validated_row)` when selected or `None` when cleared, and is emitted for every actual selection change. A matching `ListingLoaded` that replaces a selected listing clears selection in `Workspace` and emits `SelectionChanged` with the empty selection and `row: None` in the same transition as `ListingReplaced`; selecting the same entry or invalid row/name input emits nothing. `PanePresenter` changes its retained selection and row only when applying this output. It does not infer clear-on-replacement policy from `ListingReplaced` or search the listing to recover a row. The row is derived output for the current immutable snapshot; the exact name remains authoritative application state.

The adapter adds `UiEvent::SelectRow { row }`. `InputController` maps a displayed row to `Command::SelectEntry { row, name }` using the entry's exact name and changes activation to `Command::OpenEntry { row, name }`. It returns no command for a row absent from the presented snapshot. The application still validates both fields against its committed snapshot, so neither a stale row nor a forged name can select or open a different entry.

## Pane session and input boundary

`PaneSession` gains an `InputController` and `submit_ui(UiEvent) -> ViewChange`. It asks the controller for a command using the presenter's current `PaneViewModel`; an invalid row returns `ViewChange::None`. A valid command goes through the existing `submit`, so work dispatch, stale-token handling, presentation, and Qt notification selection keep one path.

`ViewChange` adds `Selection` between `Status` and `Reset`. A selection-only transition changes no rows and emits no model reset; it only synchronizes the view's selection model. When one transition contains both `SelectionChanged` and `ListingReplaced`, `Reset` dominates and the model synchronizes the already-cleared selection after the reset.

This method is tested with the existing deterministic `FakeRunner`:

- selecting a displayed row records its exact-byte entry and validated row and reports a selection change without dispatching work;
- selecting another row replaces the prior selection, and an invalid or mismatched row/name pair changes nothing;
- activating a displayed folder dispatches a read for its exact-byte child location;
- activating a file or an invalid row dispatches nothing;
- Up dispatches the logical parent read and is a no-op at root;
- activating a new destination while another loads dispatches `Cancel` before the new `ReadDirectory`;
- a failed destination keeps the shown rows and selection and changes only status;
- a successful replacement reports a reset and clears selection.

## Qt delivery

`listing_model.rs` exposes an `i32 selectedRow` property, using `-1` for no selection, and three GUI-thread bridge methods. `ListingModelRust` uses an explicit `Default` implementation so the property starts at `-1`, never at Rust's integer default of row zero. The selected row comes directly from validated application output; applying it performs no scan over the shared entry snapshot:

- `select_row(row: i32)` validates the conversion to `usize`, submits `UiEvent::SelectRow`, and applies the returned `ViewChange`.
- `activate_row(row: i32)` validates the conversion to `usize`, submits `UiEvent::ActivateRow`, and applies the returned `ViewChange`.
- `go_to_parent()` submits `UiEvent::GoToParent` and applies the returned change.

The C++ window places a compact button row and a small `ListingView` subclass of `QTreeView` in a vertical central layout, so the Up button is immediately above the item list. The button is a `QToolButton` with `QStyle::SP_ArrowUp`, an accessible name and tooltip of `Up`, no assigned shortcut, and `Qt::NoFocus`. Its pointer-originated `clicked` signal calls `go_to_parent()`; it cannot receive ordinary focused-button keyboard activation.

The view configures `SingleSelection` and `SelectRows` as defensive widget defaults, has editing disabled, and enables mouse tracking. It does not connect `QAbstractItemView::clicked` or `doubleClicked` directly: both signals can result from any mouse button, and `SingleSelection` still permits Ctrl-click deselection and built-in keyboard selection. Instead, `ListingView` keeps Qt's mechanical state subordinate to the application:

- its selection command returns `NoUpdate` for user events, so Qt cannot independently select or deselect a row through modifiers, empty-space clicks, or built-in list keys;
- a valid row's left-button release calls `select_row(index.row())`; other buttons and an invalid index submit nothing;
- a valid row's left-button double-click calls `activate_row(index.row())`; the first click has already selected the row through the application path, and other buttons submit nothing;
- its key handler accepts every key press without delegating to `QAbstractItemView`, preventing current-row movement, focus traversal, selection, type-ahead, or activation until concrete bindings are designed; and
- `selectedRowChanged` validates the model index and updates the view's `QItemSelectionModel` programmatically with `ClearAndSelect | Rows`, or clears it for `-1` or an invalid row. Programmatic synchronization does not submit another application command.

This makes the application authoritative before Qt paints the stable selection. Ctrl-clicking the selected row and clicking empty space leave it selected, matching the milestone's explicit exclusion of deselection. A successful model reset clears both application and Qt selection; a failed navigation performs no reset and preserves it.

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

The item rules distinguish `QTreeView::item:hover:!selected`, `QTreeView::item:selected:active`, and `QTreeView::item:selected:!active`. The two selected states use the same selection and text colors, so selection remains visible whenever the view is inactive; selection rules also win while the selected row is hovered.

The `QToolButton:hover` rule uses the same selection color and defines a border, because Qt's native tool-button border can otherwise cover a stylesheet background. Normal and pressed button states remain visibly dark. Hover changes only Qt rendering: it sends no adapter event, mutates no application state, and performs no model notification.

## Error presentation

No new dialog protocol is required. `PanePresenter` already turns `PermissionDenied` and `PrivacyRestricted` into distinct messages and keeps the previous location and rows after `ListingFailed`. `ListingModel::apply(ViewChange::Status)` copies that view and emits `statusTextChanged`, and the existing status `QLabel` displays the message.

This path is non-blocking, creates no nested event loop, and permits an immediate Up or different navigation. `LoadingStarted` clears the prior error, and the next successful listing restores the location text.

## Startup and shutdown

`main.rs` keeps the current start-location rule but replaces the synthetic source factory in `PaneStartup` with one that creates a fresh real reader for each job. There is no row-count configuration or demo data. Runtime startup, coalesced GUI wake-up, terminal-event accounting, panic recovery, and shutdown remain unchanged.

The C++ entry point checks the macOS process main thread with `pthread_main_np()` before constructing `QApplication`. Qt's own main-thread identity is not initialized before the application object on the acceptance host, so using `QThread::isMainThread()` at that point would reject a valid launch. The existing one-application and one-run guards remain in place.

Dropping the window still detaches the scheduler before model destruction. Dropping the runtime marks outstanding reads cancelled and never joins a worker from the GUI thread.

## Tests and evidence

- **Gateway unit tests:** temporary files and folders, exact-byte names, all entry kinds, symlink targets, dangling and unclassifiable links, exact `.`/`..` rejection, ordinary dot-prefixed names, a simulated removed-before-own-type-lookup entry, permission/privacy/loop/missing/non-directory target-probe failures, entry-own failures, sorted output, cancellation, and native-error mapping.
- **Native conversion unit tests:** absolute round trips without lossy text.
- **Inner-crate tests:** single-selection invariants, O(1) row/name validation for selection and activation, preservation during pending/failed navigation, explicit clearing on replacement, selected-row output/presentation without a listing scan, domain ordering, stale results, and error wording without Qt.
- **Pane-session unit tests:** selection, activation, and Up event mapping; selection-only view changes; work requests; failure preservation; and reset-time clearing with no Qt dependency.
- **C++ lint and review:** left-button gating, suppression of independent item-view selection and activation, selected-row synchronization, hover state isolation, consumption of every list key press, up-arrow button placement and accessibility, no button focus or assigned shortcut, centralized dark styles, and GUI-thread-only model calls.
- **Manual macOS acceptance:** the three row visual states; one-click single selection; modifier/empty-space/other-button non-deselection; double-left-click folder-only navigation; consumption of every list key press; pointer-only Up placement, hover, and navigation; no `.`/`..` rows; logical link navigation; visible denial with preserved selection; responsive dark window; and normal close from a disposable hierarchy.

No test reads, changes permissions on, or creates files in a real user location. Tests that need a path use `tempfile`. A real privacy denial is deliberately not required for acceptance: no automated or manual step alters TCC state or probes a protected real user location.

## Implementation sequence

1. Pin `tempfile = "=3.27.0"` with default features in workspace dependencies, opt into it only as a desktop dev-dependency, implement the reverse native-location conversion, and add its focused tests.
2. Add `directory_listing.rs` and gateway tests for enumeration, exact `.`/`..` and race filtering, classification, sorting, cancellation, and error mapping.
3. Add domain single selection, row/name application validation for selection and activation, explicit selection state/output, adapter row selection and selected-row presentation, and their Qt-free tests.
4. Swap the composition root to the real source and delete `synthetic_listing.rs` and every synthetic constant, import, and test.
5. Add `PaneSession::submit_ui`, explicit selection outputs and view changes, the three bridge methods, selected-row synchronization, and their focused session tests.
6. Build the vertical pane layout with the up-arrow button directly above `ListingView`, add left-button-only pointer submission and suppression of independent Qt selection/key behavior, and apply the centralized dark/hover/selection stylesheet.
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
- Selection and activation validate a row/name pair in O(1); no GUI-thread transition or presenter update scans the complete listing.
- One left click never navigates, a double left click enters only an enterable row, other mouse buttons never submit either action, and clicking Up is the only Phase 1 parent-navigation pointer action.
- Every list key press is consumed, and the Up button has no shortcut or keyboard focus; neither control introduces an implicit Qt key binding.
- Normal, hover, selected-active, selected-inactive, and Up-button-hover states are distinguishable in the dark theme; the Up hover and selected row use the same background color.
- No synchronous worker wait, `QCoreApplication::processEvents`, nested event loop, blocking dialog, unbounded thread creation, or new cross-thread Qt access is introduced.
- No concrete key binding is selected.
- No unapproved dependency, dead/demo code, `#[allow(dead_code)]`, or unused suppression is introduced.
