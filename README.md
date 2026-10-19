# Dual Pane

Dual Pane is a super-fast file manager for macOS, written in Rust.

> **Version v3.0.0 has been released.** Dual Pane now includes tabbed Browsers, editable Favorites, safe file operations, persistent workspace sessions, live folder refresh, image previews, and a redesigned Settings Window. The frameless macOS window moves from its Sidebar title row and supports the system full-screen shortcut.

## Goals

Two Browsers sit side by side. Each Browser browses its own directory, and future file operations move items between them with as little friction as possible.

The [product behavior](docs/product-behavior.md) defines included features, file-operation safeguards, session recovery, and exclusions. The [UX documents](docs/ux-terms.md) define the current interface.

Dual Pane shows two independently browsable Browsers with compact, edge-to-edge layouts. `Command+Up` opens the parent folder; `Command+Down` enters the single selected folder or opens the single selected file with its default app. Return renames the selected Item, `Shift+Command+N` creates a folder, `Command+Delete` or `Command+Forward Delete` moves selected Items to Trash, and `Option+Command+Delete` deletes them permanently. `Command+W` closes the active tab; `Command+Q`, or the close button that appears when you point at the right end of the Sidebar's **Dual Pane** title row, quits the app; `Control+Command+F` toggles full screen.

Version v3.0.0 adds:

- Each Browser has up to eight tabs with Back, Forward, Up, and `Command+R` refresh.
- `Command+T` opens a tab and `Command+W` closes the active tab; Close Window has no default shortcut.
- Sidebar Favorites can be edited.
- Folder Items show six columns, including native icons and age-colored Relative Dates that dim with their Browser, with direct sort buttons and a summary row.
- Multiple Items can be selected with Command-click, Shift-click, Shift-arrow, and `Command+A`.
- A Notices window reports settings and Favorites problems and offers a confirmed Reset Settings.
- The Settings gear and `Command+,` open Settings while settings storage is healthy. Its title row can be dragged to move the window. General controls startup Notices, whether the Sidebar close button asks before quitting, Reset Settings, and three independent visibility switches for macOS metadata and service items, dotfiles, and items marked invisible by macOS. Keyboard Shortcuts groups actions by purpose and uses icon controls to add, remove, and restore multiple bindings per action.
- Workspace sessions restore tabs, active Browsers, and window layout; a delayed settings load shows an accessible Waiting Window and protects the prior session after its timeout.
- Space previews the Folder Items cursor with macOS Quick Look; a tap opens its normal panel, while a 256 ms hold opens a panel at the largest usable screen frame with a short, motion-aware native transition.
- Each Browser has a collapsible Preview Pane above its always-visible one-line Status Bar. Expanding the pane shows a selected image supported by the installed Qt reader and item details; the chosen pane height is restored with the workspace.
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

`make install` builds a release binary and links it as `~/.local/bin/dual-pane` (override with `BIN_DIR=...`; make sure that folder is on your `PATH`). Then:

```sh
dual-pane              # open normally
dual-pane <folder>     # show the folder in the running workspace
dual-pane <file>       # show the file's folder with the file selected
dual-pane --new-instance [<folder> | <file>]  # start an independent window
```

The path may be relative or start with `~`. By default, a launch reuses the running instance for the same profile: an existing tab on the folder is activated in either Browser, or the active Browser opens it, and the workspace comes forward. A launch without a path only brings the running workspace forward. `--new-instance` bypasses forwarding. A missing or unreadable path, or more than one path, prints an error and exits without opening a window.

## Development

```sh
make check
scripts/check-single-instance.sh
```

`make fmt`, `make fmt-check`, `make lint-rust`, `make lint-cpp`, and `make test` expose each part of the gate. The single-instance check launches isolated offscreen processes and profiles. Rustfmt has a practical unlimited width (`max_width = 1000000`); clang-format has no column limit (`ColumnLimit: 0`); neither linter applies a line-length diagnostic.

The contributor and coding-agent guidelines are in [AGENTS.md](AGENTS.md). Design docs, the development process (phases and milestones), and plans are in [docs/](docs/README.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

Qt is a separate project with its own license. Dual Pane is intended to link Qt dynamically and use it under the LGPLv3.
