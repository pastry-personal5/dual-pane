# P1-M2 architecture: Stack spike

Status: Done

P1-M2 builds the smallest possible outer-ring desktop executable. It converts the temporary root package to the planned workspace, keeps all Qt and C++ code private to `dual-pane-desktop`, and deliberately introduces no inner application boundary before one has behavior to protect.

## Target boundary

The workspace after this milestone contains a root virtual workspace manifest, its Cargo-command Makefile façade, and this package:

```text
.
├── Cargo.toml
├── Makefile
├── rustfmt.toml
├── .clang-format
├── .clang-tidy
├── scripts/
│   └── lint-cpp.sh
└── crates/
    └── dual-pane-desktop/
        ├── Cargo.toml
        ├── build.rs
        ├── src/
        │   ├── main.rs
        │   └── desktop_bridge.rs
        └── cpp/
            ├── include/dual_pane_desktop/desktop_window.hpp
            └── src/desktop_window.cpp
```

`desktop_bridge.rs` registers the private `#[cxx_qt::bridge]` boundary with `CxxQtBuilder` and exposes the one no-argument call into the C++ shim through a private C ABI. CXX-Qt 0.10 does not generate a callable C++ wrapper for a bridge containing only an `extern "C++"` function; a RustQt `QObject` would be required as a workaround, but is explicitly out of scope. The C++ header and implementation own `QApplication`, `QMainWindow`, and their Qt lifetime; they use `QApplication` rather than `QGuiApplication` because the UI is Qt Widgets. `main.rs` is the composition root: it invokes the bridge function, maps its exit status to the process result, and contains no UI policy or application state.

The `cpp/` header and source have no general-purpose public API. Their narrow bridge function creates `QApplication`, constructs a `QMainWindow` titled `Dual Pane`, calls `show()`, and enters the Qt event loop. The function returns only after the user closes the window. This is the sole intentional GUI-thread event loop; the milestone schedules no blocking work and owns no workers.

## Build design

The root `Cargo.toml` becomes a virtual workspace with `crates/dual-pane-desktop` as its sole member. The root `Makefile` is a small developer-command façade, not a Cargo replacement: it declares phony `build`, `run`, `test`, `fmt`, `fmt-check`, `lint-rust`, `lint-cpp`, and `check` targets. `build`, `run`, `test`, and `lint-rust` delegate to Cargo; `fmt` and `fmt-check` run both language formatters; `lint-cpp` delegates only to `scripts/lint-cpp.sh`; and `check` composes `fmt-check`, both lint targets, and `test`. No Make target compiles C++, discovers Qt, invokes a linker, creates an application bundle, or removes files. The desktop package is the only binary. Its manifest declares the approved direct `cxx`, `cxx-qt`, and `cxx-qt-lib` dependencies plus `cxx-qt-build` as a build dependency, all with one compatible exact version line selected during the approved dependency change.

`build.rs` constructs `CxxQtBuilder`, registers `src/desktop_bridge.rs`, registers `cpp/src/desktop_window.cpp`, and requests the `Widgets` Qt module. CXX-Qt's Cargo builder performs the C++ compilation and Qt linkage; no CMake, qmake project, custom shell linker invocation, static Qt, or downloaded Qt distribution is added. The build uses the documented CXX-Qt `qmake` selection order: `QMAKE`, then `PATH`, with `QT_VERSION_MAJOR=6` available to disambiguate installed major versions.

## Code-quality tooling

Rust uses Clippy with warnings denied, as already required by [AGENTS.md](../../../../AGENTS.md). C++ uses `clang-tidy` with an explicit checked-in `.clang-tidy` profile: bug-prone, CERT, C++ Core Guidelines, modernize, performance, portability, and readability checks are enabled, and diagnostics from those checks are errors. Any exclusion must name the specific check and explain why CXX-Qt or Qt makes it inapplicable; a broad group-wide suppression is not allowed.

`scripts/lint-cpp.sh` is the sole C++ lint entry point. It locates the same Qt installation selected by `QMAKE`/`PATH`, obtains only the include paths required to parse the handwritten `cpp/` files, and invokes `clang-tidy` on each handwritten source and header with C++17 arguments. It does not build, link, create a compilation database, or run a code generator. `clang-tidy` supports supplying compilation options after `--`, which keeps this Cargo-only repository independent of CMake and a generated compilation database.

Line width is a formatter setting, not a Clippy or `clang-tidy` diagnostic. The repository deliberately enables no line-length lint. `.clang-format` sets `ColumnLimit: 0`, which LLVM defines as no column limit. Rustfmt's stable `max_width` accepts a nonnegative integer but has no no-limit sentinel; `rustfmt.toml` therefore uses the verified safe practical-unlimited `max_width = 1000000` and `use_small_heuristics = "Max"`. This is the closest safe representation of an infinite line length: attempting the largest representable integer overflows rustfmt. The configuration must not enable overflow-as-error behavior.

## Implementation sequence

1. Obtain and record dependency approval, then select compatible exact versions and verify the local `qmake` resolves Qt 6.11.2+.
2. Convert the root manifest to the virtual workspace, remove the temporary root executable, and create the root Makefile, quality-tool configuration, C++ lint entry point, `dual-pane-desktop`, and only the directories required by those source files.
3. Add the Makefile's documented phony targets. Keep it declarative and small: it may compose validation targets and dispatch to `scripts/lint-cpp.sh`, but it must not duplicate Cargo, CXX-Qt, or Qt configuration.
4. Add `.clang-tidy`, `.clang-format`, and `rustfmt.toml`. Use the explicit check profile and no-line-length configuration above, then verify that the Rust width setting remains safe with the repository's stable rustfmt.
5. Add the desktop manifest and `build.rs`; keep every Qt/CXX-Qt dependency desktop-local and request C++17 or newer in the builder configuration.
6. Add the bridge and private C++ shim. Keep the bridge to one `run_desktop`-style call, and keep window construction, event-loop lifetime, and widget ownership in C++.
7. Run the package build, launch it with `make run` for the selected proof, then run `make check` and the equivalent direct repository-wide gate. Address any Qt discovery failure through the documented environment configuration rather than adding a second compilation or project-generation system.

## Constraints and rejection criteria

- The bridge must not expose `QWidget`, `QString`, native handles, or any widget ownership to another Dual Pane crate. There are no inner crates in this milestone.
- A CXX-Qt bridge source registered with `CxxQtBuilder` and the Widgets module are required. A Rust binary that only shells out to a C++ program, or a Qt-only C++ build outside Cargo, is not evidence for this spike.
- Make targets are [phony developer actions](https://www.gnu.org/software/make/manual/html_node/Phony-Targets.html) and must remain direct wrappers around Cargo. A Makefile that encodes a second dependency graph, performs direct C++ compilation, or runs destructive cleanup is outside this milestone.
- Clippy and `clang-tidy` must not impose a character-per-line maximum. A literal Rustfmt maximum integer is rejected because it crashes the formatter; only the documented safe practical-unlimited value is permitted.
- Do not add a Rust `QObject`, model, controller, application state, or thread runtime merely to exercise the bridge. Those must wait for a milestone with behavior that needs them.
- The manual launch is intentionally the proof of an actual visible Widgets window. A unit test that merely constructs a helper, a process that exits before a window is shown, or an unverified successful link does not replace it.

The current CXX-Qt documentation confirms that `CxxQtBuilder` can be used in a Cargo-only build, requires registered bridge sources in `build.rs`, compiles additional C++ files, and links named Qt modules such as `Widgets`. See the [Cargo build guide](https://kdab.github.io/cxx-qt/book/getting-started/4-cargo-executable.html) and [`CxxQtBuilder` reference](https://docs.rs/cxx-qt-build/latest/cxx_qt_build/struct.CxxQtBuilder.html). The quality-tool configuration relies on [Rustfmt's `max_width` setting](https://rust-lang.github.io/rustfmt/?search=&version=main) and LLVM's [`ColumnLimit: 0` no-limit setting](https://clang.llvm.org/docs/ClangFormatStyleOptions.html).
