# P1-M2 overview: Stack spike

Status: Done

Convert the temporary root package into the smallest Cargo workspace that can dynamically link Qt Widgets through CXX-Qt and display an otherwise empty native window. This spike proves the outer desktop boundary and its build path; it does not begin file-manager behavior.

## Goal

Establish a reproducible Cargo-only Qt Widgets executable at `crates/dual-pane-desktop/` that launches a titled, empty window on the supported macOS and Qt versions, with a root Makefile that exposes the documented development commands without replacing Cargo.

## Scope

**In:**

- Replace the temporary root package with the virtual workspace and the sole `dual-pane-desktop` binary package described in [planned-repository-architecture.md](../planned-repository-architecture.md).
- Add a root `Makefile` with phony `build`, `run`, `test`, `fmt`, `fmt-check`, `lint-rust`, `lint-cpp`, and `check` targets. `lint-rust` runs Clippy; `lint-cpp` delegates to the dedicated C++ lint entry point; `check` runs formatting, both linters, and tests. The Makefile contains no compiler, linker, Qt-discovery, or project-generation logic.
- Add repository-owned configuration for the Rust and C++ quality tools: Clippy for Rust; `clang-tidy` for handwritten C++; `rustfmt` and `clang-format` for formatting. Both linters fail the milestone gate on their configured diagnostics.
- Do not impose a maximum character count per line. Clippy and `clang-tidy` have no line-length setting, so neither may enable a line-length diagnostic. `.clang-format` sets `ColumnLimit: 0`, LLVM's no-limit value. `rustfmt.toml` sets `max_width = 1000000` and `use_small_heuristics = "Max"`, the tested safe practical-unlimited value; a literal maximum integer crashes rustfmt, so it is deliberately not used.
- Add the approved CXX-Qt/CXX build dependencies and a `build.rs` that links Qt's `Widgets` module and compiles the one Rust-to-C++ bridge plus the minimal C++ widget shim.
- Start `QApplication`, create and show one empty `QMainWindow` titled `Dual Pane`, and return the Qt event-loop exit status from the executable.
- Demonstrate the window by manually launching `make run`, which delegates to `cargo run -p dual-pane-desktop`; this is the selected spike evidence. No desktop test API or speculative test target is introduced for this window-only milestone.
- Update user-facing build and development instructions for the workspace, Makefile targets, and Qt prerequisite.

**Out:**

- The domain, application, and adapter crates; application state; `QObject`-backed Rust state; and any file-manager behavior, including panes, tabs, commands, directory access, workers, persistence, or file operations.
- QML/Qt Quick, CMake, qmake project files, application bundles, packaging, distribution, and non-macOS support.
- A second compilation, linking, Qt-discovery, or project-generation system. The root Makefile is an approved command façade, not a replacement build system.
- A GUI automation framework, a C++ test runner, or a test-only abstraction solely to test this manual window proof.
- Any concrete key binding.

## Prerequisites and decisions

The requested root Makefile and the Rust/C++ quality-tool configuration are part of this milestone's approved scope. Implementation may start only after approval to add the direct Cargo dependencies required by the current CXX-Qt Cargo integration: `cxx`, `cxx-qt`, `cxx-qt-lib`, and build-dependency `cxx-qt-build`. The implementation change records their compatible exact version requirements and license review. It also records the installed `clang-tidy` and `clang-format` version used for the C++ gate; neither is a Cargo dependency. The change does not add a Qt download, static-Qt feature, CMake, or another compilation or project-generation system.

The supported machine must provide Xcode Command Line Tools and Qt 6.11.2+ through a `qmake` selected by `QMAKE` or `PATH` (with `QT_VERSION_MAJOR=6` when necessary), as specified in [AGENTS.md](../../AGENTS.md). The build must use dynamically linked Qt under LGPLv3.

## Completion checklist

- [x] Approval to add the four direct CXX-Qt/CXX Cargo dependencies, including their exact compatible version requirements, is recorded before the manifest changes. Evidence: 2026-09-29 user approval recorded in the [phase changelog](changelog.md): `cxx = 1.0.176`, `cxx-qt = 0.10.0`, `cxx-qt-lib = 0.10.0`, and `cxx-qt-build = 0.10.0` (all MIT OR Apache-2.0).
- [x] The root manifest is a virtual workspace whose only member is `crates/dual-pane-desktop`; the temporary root package and `src/main.rs` are removed, and no inner-ring crate or empty speculative directory is created. Evidence: workspace manifest and package tree.
- [x] The root `Makefile` declares phony `build`, `run`, `test`, `fmt`, `fmt-check`, `lint-rust`, `lint-cpp`, and `check` targets. It delegates to Cargo or the dedicated C++ lint entry point only, and `make check` completes the same formatting, Rust-lint, C++-lint, and test gate without adding build or Qt configuration. Evidence: `make check` exited 0 on 2026-09-29.
- [x] `rustfmt.toml`, `.clang-format`, and `.clang-tidy` are checked in. Rust formatting uses the tested practical-unlimited `max_width = 1000000` with `use_small_heuristics = "Max"`; C++ formatting uses `ColumnLimit: 0`; and neither Clippy nor `clang-tidy` enables a maximum-character-per-line diagnostic. Evidence: checked-in configurations and formatter checks passed on 2026-09-29.
- [x] Clippy runs with warnings denied for every workspace target. `clang-tidy` runs its configured bug-prone, CERT, C++ Core Guidelines, modernize, performance, portability, and readability checks against every handwritten C++ source and header; configured warnings are errors, and every excluded check has a documented Qt/CXX-Qt-specific rationale. Evidence: `make lint-rust` and `make lint-cpp` passed on 2026-09-29 using Homebrew LLVM 23.1.2.
- [x] `dual-pane-desktop` is the only binary package and owns `build.rs`, the Rust-to-C++ bridge, and the C++ Widgets shim. Qt, CXX-Qt, or native types do not cross into another Dual Pane crate. Evidence: the sole workspace member is `dual-pane-desktop`; all Qt and native source is package-local.
- [x] `build.rs` uses `CxxQtBuilder` to compile the bridge and C++ shim and links Qt's `Widgets` module through Cargo; it requires C++17 or newer and introduces neither CMake nor a qmake project file. Evidence: `build.rs`, the CXX-Qt builder's C++17 default, and the successful Cargo build.
- [x] `cargo build -p dual-pane-desktop` succeeds with a Qt 6.11.2+ installation selected through the documented `qmake` lookup. Evidence: command exited 0 on 2026-09-29 with `qmake -query QT_VERSION` reporting 6.11.2.
- [x] Manual spike evidence is recorded for the implementation change: `make run` opens a native window titled `Dual Pane` with no file-manager controls or content, and closing it returns the process normally. Evidence: user confirmed the empty `Dual Pane` window and normal close/process return on 2026-09-29.
- [x] `make check` and the equivalent direct commands — Rust and C++ formatter checks, `cargo clippy --all-targets -- -D warnings`, the configured `clang-tidy` entry point, and `cargo test` — pass from the workspace root. Evidence: all commands exited 0 on 2026-09-29.
- [x] [AGENTS.md](../../AGENTS.md) and [README.md](../../README.md) accurately describe the workspace, Makefile targets, Rust/C++ tool prerequisites, and the no-line-length-limit configuration; [docs/README.md](../README.md) indexes both P1-M2 plan documents. Evidence: documentation updated with the P1-M2 commands and LLVM 23.1.2 prerequisite.
