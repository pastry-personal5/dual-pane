# Phase 1 changelog

Status: Active

Chronological record of decisions, owner calls, and design changes made during Phase 1. Newest entry first.

## Entries

- 2026-09-29 — User — Confirmed that the empty native `Dual Pane` window launched by `make run` closes normally and returns the process. P1-M2 is complete.
- 2026-09-29 — Codex — Installed Homebrew LLVM 23.1.2. `make check` and the equivalent direct formatter, Clippy, clang-tidy, and test commands pass with Qt 6.11.2. The `make run` process remains in Qt's event loop, but this remote session's desktop capture does not expose its raw Qt window, so P1-M2's manual visible-window/normal-close evidence remains pending.
- 2026-09-29 — Codex — Kept `CxxQtBuilder` as the Cargo-only C++/Qt integration and registered the private bridge source, but used the shim's private C ABI for the one no-argument call into Widgets. CXX-Qt 0.10 does not generate a callable wrapper for a bridge containing only an `extern "C++"` function; creating a RustQt `QObject` to satisfy it would violate P1-M2's explicit no-`QObject` scope. `cxx-qt-lib` remains an approved direct dev dependency, ready for the first desktop test that needs a Qt type, without unnecessarily linking its unused C++ bridge code into the binary.
- 2026-09-29 — User — Approved P1-M2's direct Cargo dependencies: `cxx = 1.0.176`, `cxx-qt = 0.10.0`, `cxx-qt-lib = 0.10.0`, and build dependency `cxx-qt-build = 0.10.0`; all are MIT OR Apache-2.0. Also approved installing Homebrew LLVM for `clang-tidy` and `clang-format`.
- 2026-09-29 — User — Clarified that app operations must never block people from using the application and that the interface must remain responsive. [architecture.md](../architecture.md) now defines bounded worker execution, GUI-thread budgeting, coalescing, cancellation, and thread ownership; [mvp.md](../mvp.md) defines the corresponding user-visible behavior.
- 2026-09-29 — User — Made [mvp.md](../mvp.md) the source of truth for MVP product scope and [architecture.md](../architecture.md) the source of truth for technical architecture.
- 2026-09-29 — User — Decided to follow Clean Architecture's dependency rule, recorded in [architecture.md](../architecture.md).
- 2026-09-28 — Pastry Personal 5 — Decided minimum versions: macOS 26.7+, Qt 6.11.2+ (the version installed via Homebrew), recorded in AGENTS.md under "Decided".
- 2026-09-28 — Pastry Personal 5 — Split the planned "stack spike" milestone in two: P1-M1 (this milestone) covers architecture design and review only, producing no code; P1-M2 covers the actual build-and-window spike, narrowed from the original full spike (model + worker thread) to just build + empty window. The workspace layout was deferred at that time.
