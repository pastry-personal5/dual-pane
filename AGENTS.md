# AGENTS.md

Instructions for AI coding agents and contributors working in this repository.

This file is the **single source of agent instructions**. Do not create `CLAUDE.md` or any other agent-specific instruction file, whether as a copy, a symlink, or a stub. Put new guidance here.

## Project

`dual-pane` is a dual-pane (two-panel) file manager for macOS, written in Rust with a Qt 6 Widgets UI. It is in the **pre-alpha** stage: so far only a placeholder `src/main.rs` exists.

See [README.md](README.md) for the user-facing overview and the MVP scope.

## Decided — do not change without asking

- **Language:** Rust, edition 2024.
- **GUI:** Qt 6 **Widgets**. Do not introduce QML/Qt Quick.
- **Rust ↔ Qt:** [CXX-Qt](https://github.com/KDAB/cxx-qt) (`cxx-qt`, `cxx-qt-lib`, `cxx-qt-build`). Qt Widgets APIs have no Rust bindings, so a thin C++ layer builds the widgets.
- **Build system:** Cargo only. `cxx-qt-build` in `build.rs` compiles the C++ and links Qt. Do not add CMake, Corrosion, or qmake project files.
- **Platform:** macOS only. Don't add Windows or Linux code paths or CI unless asked.
- **License:** Apache-2.0 for all project code.
- **MVP scope:** two panes, copy, move, rename, delete, mkdir, and tabs in each pane. The viewer or Quick Look, archive browsing, and remote file systems are out of scope for now.

## Undecided — ask before inventing

- **Interaction model and keymap:** to be designed. Don't hard-code keybindings modeled on Total Commander, Midnight Commander, or another file manager.
- **Module and directory layout:** not decided yet, including where the C++ shim lives. Propose a layout before creating one.
- **Delete semantics:** whether delete means move to Trash or permanent deletion, and how confirmations work.

## Environment setup

- Rust stable via rustup, plus the `rustfmt` and `clippy` components.
- Xcode Command Line Tools for the C++ compiler.
- Qt 6, for example from `brew install qt`. CXX-Qt locates Qt through `qmake`. It checks the `QMAKE` env var first, then looks for `qmake6` or `qmake` on `PATH`. If both Qt 5 and Qt 6 are installed, set `QT_VERSION_MAJOR=6`.

  ```sh
  export QMAKE="$(brew --prefix qt)/bin/qmake"
  ```

The current scaffold has no Qt dependency. Qt is needed only once CXX-Qt is added.

## Commands

```sh
cargo build                                  # build
cargo run                                    # run the app
cargo test                                   # run tests
cargo fmt --all                              # format
cargo fmt --all -- --check                   # check formatting
cargo clippy --all-targets -- -D warnings    # lint; must be clean
```

Before you consider a change done, run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`. All three must pass.

## Architecture rules

- **Keep logic in Rust.** File-system operations, pane and tab state, and file-operation jobs live in Rust modules that do not depend on Qt, so they can be unit-tested without a GUI.
- **Bridge modules stay thin.** `#[cxx_qt::bridge]` modules expose Rust state to C++ as `QObject`s (properties, signals, invokables). They should mostly delegate to the Qt-free core.
- **C++ is a view layer only.** The C++ shim constructs and wires up widgets. Don't put business logic there. Use C++17 or newer, as Qt 6 requires.
- **Don't block the UI thread.** Long-running file operations (copying, moving, deleting, scanning large directories) must run off the GUI thread and report progress back.

## Code conventions

- Use default `rustfmt` formatting. No `clippy` warnings.
- Limit `unsafe` to what CXX-Qt bridges require. Any other `unsafe` block needs a `// SAFETY:` comment that explains why it is sound.
- Return errors as `Result` and don't `unwrap()` or `expect()` on I/O in non-test code. File operations fail routinely (permissions, missing files, full disks) and must be reported to the user.

## Safety rules for a file manager

- Tests and examples that touch the file system must work inside a temporary directory (for example, with the `tempfile` crate). Never use real user paths such as `~`, `/`, or `/Users/...`.
- Never silently overwrite or delete user data. Conflicts (for example, when the destination exists) must be surfaced for a decision.

## Dependencies and licensing

- New dependencies must have licenses compatible with Apache-2.0, such as MIT, Apache-2.0, BSD, or Zlib. Don't add GPL-only crates.
- Qt is linked dynamically and used under the LGPLv3. Don't introduce static linking of Qt.

## Keeping docs current

- Update `README.md` when user-visible features, requirements, or build steps change.
- Update this file when build commands, tooling, or project decisions change, or when an item under "Undecided" gets decided.
