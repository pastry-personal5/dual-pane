# Dual Pane

Dual Pane is a super-fast file manager for macOS, written in Rust.

> **Version 1.0.0 has been released.** The current development build shows two independently browsable directories in a dark native window. Each Browser can select entries, enter folders and folder links, and go to the logical parent. Directory reads run on separate workers per Browser, and access failures leave the last successful listing visible.

## Goals

Two file panes sit side by side. Each pane browses its own directory, and files move between them with as little friction as possible.

The [MVP scope](docs/mvp.md) defines the included features, user interaction, file-operation safeguards, session behavior, and exclusions.

The completed Phase 2 build shows two independently browsable panes with compact, edge-to-edge layouts. `Command+W` closes its window and exits the app; `Command+Q` quits the app.

## Technology and architecture

The [architecture](docs/architecture.md) defines the selected technologies, Clean Architecture boundaries, threading, file-operation safety, and verification strategy.

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
```

## Development

```sh
make check
```

`make fmt`, `make fmt-check`, `make lint-rust`, `make lint-cpp`, and `make test` expose each part of the gate. Rustfmt has a practical unlimited width (`max_width = 1000000`); clang-format has no column limit (`ColumnLimit: 0`); neither linter applies a line-length diagnostic.

The contributor and coding-agent guidelines are in [AGENTS.md](AGENTS.md). Design docs, the development process (phases and milestones), and plans are in [docs/](docs/README.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

Qt is a separate project with its own license. Dual Pane is intended to link Qt dynamically and use it under the LGPLv3.
