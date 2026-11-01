# Contributing to Dual Pane

Status: Active

This guide covers the local development checks and contribution workflow for Dual Pane. Read [AGENTS.md](../AGENTS.md) for the complete repository rules and [docs/README.md](README.md) for the design and milestone documentation index.

## Requirements

- macOS
- Rust stable, installed through [rustup](https://rustup.rs/), with the `rustfmt` and `clippy` components
- Xcode Command Line Tools (`xcode-select --install`), which provide the C++ compiler
- Qt 6, installed with Homebrew (`brew install qt`) or the official Qt installer
- LLVM's `clang-format` and `clang-tidy` (`brew install llvm`)

CXX-Qt finds Qt through `qmake`. Either put Qt 6's `qmake` on your `PATH`, or point the `QMAKE` environment variable at it:

```sh
export QMAKE="$(brew --prefix qt)/bin/qmake"
```

The desktop requires Qt 6.11.2+ and dynamically links Qt Widgets through CXX-Qt. The quality scripts locate Homebrew LLVM automatically; set `CLANG_FORMAT` or `CLANG_TIDY` to override those binaries.

## Building and running

```sh
make build
make run
make package-macos
```

`make install` builds a release binary and links it as `~/.local/bin/dual-pane` (override with `BIN_DIR=...`; make sure that folder is on your `PATH`).

`make package-macos` creates the local, unnotarized application bundle at `target/package/Dual Pane.app` (override with `APP_BUNDLE="/path/to/Dual Pane.app"`).

`make install-macos` replaces `/Applications/Dual Pane.app` and links its executable as `~/.local/bin/dual-pane`. This may require permission to write `/Applications`; use `APP_INSTALL_DIR="$HOME/Applications"` for a per-user install. Ensure `~/.local/bin` is on your `PATH`.

```sh
dual-pane              # open normally
dual-pane <folder>     # show the folder in the running workspace
dual-pane <file>       # show the file's folder with the file selected
dual-pane --new-instance [<folder> | <file>]  # start an independent window
```

The path may be relative or start with `~`. By default, a launch reuses the running instance for the same profile: an existing tab on the folder is activated in either Browser, or the active Browser opens it, and the workspace comes forward. A launch without a path only brings the running workspace forward. `--new-instance` bypasses forwarding. A missing or unreadable path, or more than one path, prints an error and exits without opening a window.

## Development checks

Run the full quality gate before calling a change complete:

```sh
make check
```

The individual checks are also available:

```sh
make fmt
make fmt-check
make lint-rust
make lint-cpp
make test
```

The native acceptance scripts cover the most important macOS workflows:

```sh
scripts/check-folder-items-context-menu.sh
scripts/check-file-drag-drop.sh
scripts/check-file-associations.sh
scripts/check-single-instance.sh
```

The Folder Items Context Menu check exercises the owned asynchronous native-menu lifecycle without observing the global clipboard. The file drag-and-drop check uses only temporary source and destination folders. The file-associations check uses temporary text files and a disposable profile, discovers macOS applications, and never changes a Launch Services default. The single-instance check launches isolated offscreen processes and profiles.

Rustfmt has a practical unlimited width (`max_width = 1000000`); clang-format has no column limit (`ColumnLimit: 0`); neither linter applies a line-length diagnostic.

## Before opening a change

- Keep implementation, tests, and affected documentation together.
- Use temporary directories for file-system tests and examples; never test against real user paths.
- Preserve the project's Clean Architecture, MVVM presentation, command-based input, and file-operation safety boundaries.
- Do not add dependencies or build tools without asking first.
- Follow the numbered phase and milestone process in [docs/development-process.md](development-process.md) and the active plan in [docs/roadmap.md](roadmap.md).

The project is licensed under Apache-2.0. Qt is dynamically linked and remains a separate LGPLv3 project.
