# Dual Pane

Dual Pane is a super-fast file manager for macOS, written in Rust.

> **Status: pre-alpha.** The project has just been initialized. Only a placeholder binary exists, and nothing is usable yet.

## Goals

Two file panes sit side by side. Each pane browses its own directory, and files move between them with as little friction as possible.

### MVP scope

- Two side-by-side panes, each with an independent current directory, and one active pane at a time
- Basic file operations: copy and move between panes, rename, delete, and create directory
- Tabs in each pane, so each side can hold several directories

Not part of the MVP: a built-in viewer or Quick Look, archive browsing, and remote file systems (SFTP/SMB).

### Interaction model

The keyboard and mouse interaction model is **to be designed**. The project will not copy an existing file manager's keymap (Total Commander, Midnight Commander, and so on) by default.

## Technology

| Area          | Choice                                                                 |
|---------------|------------------------------------------------------------------------|
| Language      | Rust (edition 2024)                                                    |
| GUI toolkit   | Qt 6, **Qt Widgets** (not QML/Qt Quick)                                |
| Rust ↔ Qt     | [CXX-Qt](https://github.com/KDAB/cxx-qt) plus a thin C++ layer for the widgets |
| Build system  | Cargo only; `cxx-qt-build` in `build.rs` compiles the C++ and links Qt |
| Platform      | macOS                                                                  |
| License       | Apache-2.0                                                             |

### Planned architecture

- **Rust core.** File-system operations, pane and tab state, and the application logic. It is kept independent of Qt so it can be tested without a GUI.
- **Bridge.** CXX-Qt exposes Rust types to C++ as `QObject`s with properties, signals, and invokables.
- **C++ widget shim.** A thin layer that builds the Qt Widgets UI (main window, pane views, tabs) and connects it to the bridged objects. It contains no application logic.

## Requirements

- macOS
- Rust stable, installed through [rustup](https://rustup.rs/), with the `rustfmt` and `clippy` components
- Xcode Command Line Tools (`xcode-select --install`), which provide the C++ compiler
- Qt 6, installed with Homebrew (`brew install qt`) or the official Qt installer

CXX-Qt finds Qt through `qmake`. Either put Qt 6's `qmake` on your `PATH`, or point the `QMAKE` environment variable at it:

```sh
export QMAKE="$(brew --prefix qt)/bin/qmake"
```

The current scaffold does not link Qt yet. Qt only becomes a hard requirement once the UI work starts.

## Building and running

```sh
cargo build
cargo run
```

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The contributor and coding-agent guidelines are in [AGENTS.md](AGENTS.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

Qt is a separate project with its own license. Dual Pane is intended to link Qt dynamically and use it under the LGPLv3.
