# Dual Pane

Dual Pane is a super-fast file manager for macOS, written in Rust.

> **Version 2.0.0 has been released.** Dual Pane now offers two independently browsable directories in a dark native window. Each Browser can select entries, enter folders and folder links, and go to the logical parent. Directory reads run on separate workers per Browser, and access failures leave the last successful Folder Items visible.

## Goals

Two Browsers sit side by side. Each Browser browses its own directory, and future file operations move items between them with as little friction as possible.

The [product behavior](docs/product-behavior.md) defines included features, file-operation safeguards, session recovery, and exclusions. The [UX documents](docs/ux-terms.md) define the current interface.

Version 2.0.0 shows two independently browsable Browsers with compact, edge-to-edge layouts. `Command+W` closes its window and exits the app; `Command+Q` quits the app.

The current development build adds Phase 3 workspace features that are not in a release yet:

- Each Browser has up to eight tabs with Back, Forward, Up, and `Command+R` refresh.
- `Command+T` opens a tab and `Command+W` closes the active tab; Close Window has no default shortcut.
- Sidebar Favorites can be edited.
- Folder Items show six columns, including native icons and age-colored Relative Dates that dim with their Browser, with direct sort buttons and a summary row.
- Multiple Items can be selected with Command-click, Shift-click, Shift-arrow, and `Command+A`.
- A Notices window reports settings and Favorites problems and offers a confirmed Reset Settings.
- Workspace sessions restore tabs, active Browsers, and window layout; a delayed settings load shows an accessible Waiting Window and protects the prior session after its timeout.
- Space previews the Folder Items cursor with macOS Quick Look; a tap opens its normal panel, while a 256 ms hold opens a panel at the largest usable screen frame with a short, motion-aware native transition.
- File operations, delivered by milestone P3-M8, are in the development build:
  - Copy and Move to the other Browser, Rename, New Folder, Move to Trash, and Delete Permanently, from the Folder Items Context Menu or their shortcuts.
  - Each operation runs off the GUI thread, never overwrites without a choice, and reports its progress and decisions in an Operation Panel and its result in Notices.
  - Activating a file, an application, or another package opens it with its default application.

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
