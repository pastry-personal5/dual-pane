# P1-M4 architecture: Qt listing model and worker delivery

Status: Done

P1-M4 builds the desktop ring's delivery path: a Rust Qt list model, a pane session that owns the application state on the GUI thread, and a bounded worker runtime. Everything except the synthetic listing source is kept by the next milestone, which only swaps the source for the real directory reader.

## Target boundary

```text
crates/dual-pane-desktop/
├── Cargo.toml                  # + inner crates, cxx-qt-lib moves to [dependencies]
├── build.rs                    # registers listing_model.rs; keeps Widgets
├── src/
│   ├── main.rs                 # composition root: start location, source, run
│   ├── listing_model.rs        # CXX-Qt bridge: ListingModel QObject, run_desktop
│   ├── pane_session.rs         # Qt-free: workspace + presenter + runtime, bounded drain
│   ├── runtime.rs              # Qt-free: worker thread, job queue, cancellation, wake
│   ├── synthetic_listing.rs    # Qt-free: 100,000 sorted entries, cancellable
│   └── native_location.rs      # Qt-free: Path ↔ Location without lossy names
└── cpp/
    ├── include/dual_pane_desktop/desktop_window.hpp
    └── src/desktop_window.cpp  # QApplication, QMainWindow, QTreeView, status line
```

`desktop_bridge.rs` and the `extern "C"` entry point are removed: the bridge declares `run_desktop(Box<PaneStartup>)` in an `extern "C++"` block and CXX-Qt generates its call wrapper. That requires `cxx` and CXX-Qt's `cxx-gen` to share a patch version, because cxx embeds it in bridge symbol names; `cxx` is pinned to `=1.0.202` for that reason.

`synthetic_listing.rs` is the only module the next milestone deletes. Every other module keeps its role.

## Post-review corrections

Every accepted runtime request now ends in exactly one typed terminal event: loaded, failed, or cancelled. Cancellation has no presentation error. A per-job atomic terminal claim is shared by the GUI and worker paths, so cancellation cannot race a queued completion into a duplicate. Each job constructs a fresh source instance inside its worker panic boundary. The worker reduces a panic to a payload-free marker, drops the source with the worker, and a process panic hook emits only a sanitized worker-failure diagnostic. The supervisor emits `Internal` and continues after worker-creation failures, disconnected workers, source-construction panics, or source-execution panics with exponential bounded backoff. Dispatch to an unavailable supervisor also becomes an immediate `Internal` terminal event. The C++ shim owns queued GUI-drain scheduling so RustQt locks have returned before a follow-up slice is posted; it also owns lifetime-safe wake suppression during teardown and launch precondition guards.

## Composition and startup

1. `main.rs` computes the start location: `std::env::current_dir()` converted by `native_location::location_from_path`, or `Location::root()` if either step fails. It builds a `PaneStartup` holding that location and a factory for fresh listing-source instances (each backed by a `SyntheticListing` with `rows = 100_000`) and calls `run_desktop(Box<PaneStartup>)`. It maps the returned status to the process exit code.
2. The C++ `run_desktop` creates `QApplication`, a `QMainWindow` titled `Dual Pane`, a `ListingModel`, a `QTreeView` with uniform row heights, no root decoration, and no header, and a status `QLabel`, all on the stack in that order so each is destroyed before the window that holds it. It sets the model on the view, connects the model's `statusText` change to the label, calls `model->start(std::move(startup))`, shows the window, and enters the event loop. C++ owns every Qt object's lifetime.
3. `ListingModel::start` creates the `PaneSession` from the startup value and the C++ queued-drain wake function, then submits `Command::Navigate(start)`.

`main.rs` stays the only place that chooses concrete sources and the start location. `ListingModel::start` wires only what needs the live `QObject`: its thread handle.

## Listing model (`listing_model.rs`)

A `#[cxx_qt::bridge]` declares `ListingModel` as `#[qobject] #[base = QAbstractListModel]` with `impl cxx_qt::Threading`. Its Rust struct holds `Option<PaneSession>` and a `status_text: QString` property.

| Member | Behavior |
|---|---|
| `rowCount(parent)` override | `0` for a valid parent; otherwise the presenter view's `row_count()`, saturated to `i32`. |
| `data(index, role)` override | For `Qt::DisplayRole`, the row's display name as a `QString`, formatted on demand; otherwise an empty `QVariant`. |
| `beginResetModel` / `endResetModel` | `#[inherit]` calls used for `ListingReplaced`. A reset is constant-time for a list model; the view requests only visible rows. |
| `start(Box<PaneStartup>)` invokable | Creates the session and submits the start navigation. |
| `drain()` | Called only by queued wake closures on the GUI thread. Asks the session to drain one count-and-time-bounded slice, applies the reported model change and status text, and asks the C++ scheduler for another queued turn if the session reports more pending results, so input and painting run between slices. |

The C++ scheduler uses a queued `QMetaObject::invokeMethod` with its `QObject` as the context. Qt removes pending posted events when that context is destroyed, and the scheduler suppresses new wakes during teardown.

## Pane session (`pane_session.rs`)

`PaneSession` is Qt-free and lives on the GUI thread. It owns `Workspace`, `PanePresenter`, and a `WorkRunner`: the `Runtime` in the app, or a deterministic fake in tests. `WorkRunner` has two methods: non-blocking `dispatch(WorkRequest)` returns an optional immediate terminal event, and `take_events(max)` returns cross-thread events plus whether more remain. Returning immediate cancellation and unavailable-supervisor events lets the serialized pane session handle them directly instead of entering the cross-thread wake path from the GUI thread.

- `submit(input)` runs `Workspace::handle`, applies every output to the presenter, dispatches every work request to the runtime, and serially handles any immediate terminal events returned by dispatch. It returns a `ViewChange` (`None`, `Status`, or `Reset`) describing what the Qt model must notify.
- `drain()` handles no more than `DRAIN_SLICE` (32) delivered events and yields after the provisional 4 ms `DRAIN_TIME_BUDGET`. It buffers fetched-but-unhandled events and polls the runtime when that buffer empties so a concurrent worker wake cannot be consumed by a turn that never reaches the runtime queue. It returns the combined `ViewChange` plus whether more events remain.
- It also exposes the presenter view, and `status_text` formats a view's status line (the error message, loading, or the location path).
- The Qt model keeps its own copy of the view (cheap, because entries are shared through an `Arc`) and replaces it only between `beginResetModel` and `endResetModel`, so Qt never sees data change outside a reset.

Keeping this logic out of the bridge makes the bounded-drain rule testable without Qt.

## Runtime (`runtime.rs`)

`Runtime` owns a supervisor and one active named `listing-worker`. That is the Phase 1 capacity; a source panic is caught per job, reported as `Internal`, and followed by replacement after 100 ms exponential backoff capped at 5 s. A completed job resets the delay.

| Part | Design |
|---|---|
| Job queue | `std::sync::mpsc` channel of jobs `{ token, location, state: Arc<JobState> }`. A closed supervisor queue immediately produces one `Internal` failure instead of silently losing the request. |
| Source lifetime | `ListingSourceFactory` is shared configuration. Every accepted job creates one fresh `ListingSource` inside the worker panic boundary, so a failed job cannot contaminate the next worker with source-local state. |
| Cancellation | A map from `RequestToken` to per-job state. Its atomic terminal claim is shared with the worker: `WorkRequest::Cancel` sets the flag and returns `ListingCancelled` directly to the pane session only if it wins that claim, while a worker delivers its result only if it wins. Taking or immediately returning a terminal event forgets its token, so the map stays bounded. The worker skips a job whose flag is already set and passes the flag to the source, which checks it at least every 1,024 entries. |
| Results | A second channel carries application `Event`s from the worker to the GUI thread. |
| Wake | After sending a cross-thread result, the supervisor calls the wake function only if an `AtomicBool` "wake pending" flag was clear. `drain` clears the flag before reading, so no result is left without a wake. GUI-originated immediate events do not call the wake function. |
| Panic and recovery | The worker catches panics from both source construction and execution, drops the panic payload in that worker, and sends only `Panicked` to the supervisor. A payload destructor that panics disconnects that worker and follows the same recovery path. The named-worker panic hook emits a fixed diagnostic without formatting the payload. The failed job is never replayed; independent queued jobs continue on fresh workers after 100 ms exponential backoff capped at 5 s, and any completed job resets the delay. |
| Shutdown | Dropping the runtime closes the job channel and sets every outstanding flag. The GUI thread never joins or waits for the worker; the worker exits after its current job. |

The source factory is an `Arc<dyn Fn() -> ListingSource + Send + Sync>`. Each produced `ListingSource` is a `Box<dyn Fn(&Location, &AtomicBool) -> Option<Result<Arc<[Entry]>, ListingErrorKind>> + Send>`, where `None` means cancelled. The runtime is thus independent of the source, the tests can create fresh blocking fakes, and the next milestone swaps in a factory for the real reader without changing the runtime.

P1-M4 bounds execution to one active worker but retains the standard library's unbounded job and terminal-event channels. Finite admission capacity, typed saturation, lane circuits, and periodic recovery probes remain part of the explicitly post-P1-M4 planned execution model in [architecture.md](../architecture.md#52-planned-desktop-execution-model); this milestone does not claim that later queue model.

## Synthetic source (`synthetic_listing.rs`)

`SyntheticListing { rows }` ignores the location and builds `rows` entries on the worker. The names are deterministic, such as `Folder 00040` for every tenth row and `Item 00042.txt` otherwise. It sorts them with `sort_by_cached_key(listing_sort_key)` and returns them as `Arc<[Entry]>`. It checks cancellation before allocating, while building, and before sorting.

## Location conversion (`native_location.rs`)

`location_from_path(&Path) -> Option<Location>` accepts only absolute paths made of root and normal components. It converts each component's `OsStr` bytes (`std::os::unix::ffi::OsStrExt`) into an `EntryName` without lossy conversion. It rejects `.`, `..`, and relative paths. The next milestone adds the reverse conversion with its first user.

## Tests

- Unit tests live in `#[cfg(test)]` modules of the Qt-free files and run under the desktop crate's normal `cargo test`. They use fake sources and wake counters, never Qt objects or real user paths.
- Runtime and pane-session tests cover immediate terminal events without a wake, count-and-time-bounded draining, and the race where a cross-thread event arrives behind a locally buffered batch.
- Panic and recovery tests cover fresh source instances, source-construction and source-execution panics, payload-free supervisor outcomes, worker disconnection during panic-payload destruction, no replay, FIFO queue survival, continued cancellation during backoff, and the backoff increase, cap, and reset sequence.
- The Qt model and window are proven by the manual launch evidence in the [overview](milestone-04-overview.md#completion-checklist).

## Implementation sequence

1. Move `cxx-qt-lib` to `[dependencies]`, add the inner-crate path dependencies, and verify with a minimal `ListingModel` that CXX-Qt 0.10.0 generates the `run_desktop` wrapper, `#[base = QAbstractListModel]`, `#[cxx_override]`, `#[inherit]`, and `Threading`. Record the outcome, including any fallback, in the changelog.
2. Add `native_location.rs`, `synthetic_listing.rs`, and `runtime.rs` with their tests.
3. Add `pane_session.rs` with its tests.
4. Complete `listing_model.rs` and the C++ window, remove `desktop_bridge.rs` and the old C ABI, and update `build.rs`.
5. Run the manual launch, then the dead-code, stray-file, and naming checks, then `make check` and the direct commands once.
6. Update AGENTS.md, README.md, `planned-repository-architecture.md`, and `phase.md`, and check the completed items.

## Constraints and rejection criteria

- The GUI thread never generates, sorts, or reads listings; never joins or waits for the worker; and handles events within both `DRAIN_SLICE` and `DRAIN_TIME_BUDGET` per event-loop turn.
- Workers never touch Qt objects, the presenter, or the workspace; they reach the GUI only through the wake function.
- No Qt or CXX-Qt type appears outside `listing_model.rs` and the C++ shim; the other desktop modules stay Qt-free.
- No unbounded thread creation, no busy waiting, and no `unwrap()` or `expect()` on I/O outside tests.
- No leftover code: the old bridge is removed, no `#[allow(dead_code)]` is added, and nothing is kept "for later" except what the next milestone uses unchanged.
