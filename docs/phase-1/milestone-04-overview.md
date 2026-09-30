# P1-M4 overview: Qt listing model and worker delivery

Status: Done

Connect the desktop window to the inner crates. A Rust Qt list model shows one pane's listing in a `QTreeView`, a bounded worker produces the listing off the GUI thread, and results reach the GUI thread through a coalesced, bounded delivery path. The listing comes from a synthetic source of 100,000 rows that the next milestone replaces with the real directory reader.

## Goal

Launch the app into a window whose single pane shows a 100,000-row synthetic listing for the launch working directory. The listing is produced on a worker thread, scrolls smoothly, and never stalls the GUI thread. Stale results are ignored and cancelled work stops.

## Scope

**In:**

- Wire `dual-pane-desktop` to `dual-pane-domain`, `dual-pane-application`, and `dual-pane-adapters`.
- A CXX-Qt `QObject` list model written in Rust, shown by a `QTreeView` in the existing main window, with a status line that shows the pane's location, loading state, or error.
- A pane session that owns the workspace, presenter, and runtime on the GUI thread and drains worker results in bounded slices.
- A supervised single-worker runtime with a cancellable job queue, typed terminal cancellation, panic recovery, and a coalesced C++ queued GUI wake.
- A synthetic listing source that returns 100,000 deterministic, sorted entries for any location and stops when cancelled.
- Conversion from the launch working directory to a domain `Location` without lossy name conversion.
- Moving `cxx-qt-lib` from a dev-dependency to a normal dependency of `dual-pane-desktop`, as approved.
- Qt-free unit tests for the runtime, pane session, synthetic source, and location conversion, plus the manual launch evidence below.

**Out:**

- Reading real directories, opening rows, going to the parent, key bindings, and error dialogs. The next milestone adds the real directory reader (and its `tempfile` tests), connects row activation and go-to-parent, deletes the synthetic source, and completes the Phase 1 exit criteria.
- Two panes, tabs, icons, metadata columns, sorting controls, and incremental row deltas.
- Any dependency other than the approved `cxx-qt-lib` and `cxx` changes. `tempfile` is approved but is added only with the first test that needs it.
- A GUI automation framework or C++ test runner.

## Prerequisites and decisions

The user decided these on 2026-09-30, recorded in the [phase changelog](changelog.md):

1. **Model:** the list model is a Rust `QObject` built with CXX-Qt, subclassing `QAbstractListModel`. A C++ `QObject`-side scheduler posts coalesced queued drain events and suppresses late wakes during teardown.
2. **`cxx-qt-lib`:** approved to move from `[dev-dependencies]` to `[dependencies]` of `dual-pane-desktop` at the same pinned `=0.10.0` (MIT OR Apache-2.0), for `QString`, `QModelIndex`, and `QVariant`. During implementation the user also approved bumping `cxx` from `=1.0.176` to `=1.0.202` to match CXX-Qt's code generator (see the [changelog](changelog.md)).
3. **Data source:** a synthetic source of 100,000 rows runs in the app for this milestone. It is live code, not dead code; the next milestone deletes it when the real reader replaces it.
4. **Start location:** the launch working directory. If it cannot be determined or represented, the pane starts at the root.
5. **`tempfile`:** approved as a `dual-pane-desktop` dev-dependency for directory-reader tests, to be added in the next milestone with its first test.

## Completion checklist

### Post-review corrections

- [x] Every accepted runtime read produces exactly one typed terminal event (`ListingLoaded`, `ListingFailed`, or `ListingCancelled`), except teardown when the receiver is discarded. A shared per-job atomic terminal claim makes cancellation and queued completion mutually exclusive; current cancellation clears loading silently, and stale events are no-ops. Evidence: `cargo test -q -p dual-pane-application` and `cargo test -q -p dual-pane-desktop runtime` on 2026-09-30.
- [x] A panicking source, failed worker creation, disconnected worker, or unavailable supervisor is reported as `ListingErrorKind::Internal` without exposing a panic payload. The supervisor remains alive and uses 100 ms exponential backoff capped at 5 s; completed work resets the delay, FIFO work survives restarts, and queued work remains cancellable. Evidence: `cargo test -q -p dual-pane-desktop runtime` on 2026-09-30.
- [x] GUI draining is scheduled by a C++ queued-event scheduler: it invokes one Rust drain slice and only queues a follow-up after that call returns. Late wakes during teardown are harmless. Evidence: `make lint-cpp` and desktop model/runtime review on 2026-09-30.
- [x] `PanePresenter` owns all status-line wording, including loading, location, ordinary errors, internal errors, startup failure, and silent cancellation. Evidence: `cargo test -q -p dual-pane-adapters` on 2026-09-30.
- [x] `run_desktop` rejects a non-main thread, an existing `QApplication`, and every second invocation with a defined nonzero result before widget construction. Evidence: C++ launch-precondition guard review and manual launch evidence in the phase changelog.
- [x] The planned repository tree lists `synthetic_listing.rs` and says P1-M5 removes it. Evidence: `docs/planned-repository-architecture.md`.

- [x] `dual-pane-desktop` depends on the three inner crates and on `cxx-qt-lib = "=0.10.0"` as a normal dependency; the `[dev-dependencies]` entry for `cxx-qt-lib` is gone, and no other dependency changes except the approved `cxx = "=1.0.202"` bump. Evidence: `crates/dual-pane-desktop/Cargo.toml` and `git diff Cargo.lock`.
- [x] Runtime tests prove that a read request runs on the worker and its result is delivered; that cancelling a queued request removes it before it runs; that cancelling a running request stops it without delivering a result; that results are delivered in completion order; and that many results cause only one pending GUI wake. Evidence: `cargo test -q -p dual-pane-desktop runtime`.
- [x] Pane-session tests prove that the initial navigation dispatches one read; that a delivered result updates the presenter and reports a model reset; that a stale result reports nothing; and that one drain handles at most the configured slice and reports that more remain. Evidence: `cargo test -q -p dual-pane-desktop pane_session`.
- [x] Synthetic-source tests prove that it returns the configured number of entries in `listing_sort_key` order, mixing folders and files, and that it stops early and returns nothing when cancelled. Evidence: `cargo test -q -p dual-pane-desktop synthetic_listing`.
- [x] Location-conversion tests prove that an absolute path becomes a `Location` with byte-exact components, including a name that is not valid UTF-8, and that a relative path or one containing `..` is rejected. Evidence: `cargo test -q -p dual-pane-desktop native_location`.
- [x] Manual launch evidence: `make run` from a directory opens the `Dual Pane` window immediately; the status line shows loading and then that directory's path; the pane shows 100,000 rows with folders first; scrolling from top to bottom and back stays smooth; and closing the window returns the process normally. Evidence: user confirmation recorded in the [phase changelog](changelog.md).
- [x] No dead code or stray files: every desktop module is reachable from `main.rs` or is a `#[cfg(test)]` module; no `#[allow(dead_code)]` or `#[allow(unused…)]` is added; the P1-M2 C ABI shim and any code the new bridge replaces are removed; and `git status` shows only intended files, with no scratch output, logs, or `proptest-regressions/` directories. Evidence: `rg -n 'allow\((dead_code|unused)' crates/dual-pane-desktop` prints nothing, and `git status --short` is reviewed.
- [x] No source-code name uses a phase word, and no source code or script uses `MVP` wording. Evidence: `rg -n -i 'phase|mvp' crates scripts Makefile` prints nothing.
- [x] `make check` and the equivalent direct commands (`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make lint-cpp`, `cargo test`) each exit 0. Evidence: on 2026-09-30, `make check`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make lint-cpp`, and `cargo test` each exited 0.
- [x] Documentation is current: the "Project" line and any changed build notes in [AGENTS.md](../../AGENTS.md), the status in [README.md](../../README.md), [planned-repository-architecture.md](../planned-repository-architecture.md) if `dual-pane-desktop` gains files the tree does not show, and this milestone's status in [phase.md](phase.md). Evidence: the same change updates them.
