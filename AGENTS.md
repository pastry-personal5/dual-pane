# AGENTS.md

Instructions for AI coding agents and contributors. This file is the **single source of agent instructions**. Do not create `CLAUDE.md` or any other agent-specific instruction file (copy, symlink, or stub). Put new guidance here.

## Project

`dual-pane` is a dual-Browser file manager for macOS, written in Rust with a Qt 6 Widgets UI. Version 2.0.0 is released. The Qt-free domain, application, and adapter crates own and test the behavior; the desktop crate executes file operations on a two-worker lane with a safety journal.

Start from [README.md](README.md), [docs/README.md](docs/README.md) (doc index), [docs/product-behavior.md](docs/product-behavior.md), and [docs/architecture.md](docs/architecture.md).

## Agent workflow

- Work autonomously only on a clearly scoped task within the active milestone. [docs/roadmap.md](docs/roadmap.md) names the active phase; follow its milestones in order unless the phase plan says otherwise.
- Before implementing, read the active phase and milestone plans and the relevant design docs.
- Before executing a milestone, add a completion checklist to its overview doc. Check an item only after its acceptance evidence exists. Keep implementation, tests, and affected docs in the same change.
- A milestone is complete only when every item is checked and the gate passes: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (all run by `make check`). If a command cannot run, leave its item unchecked and report the exact blocker.
- Ask before: deciding an "Undecided" item (none are open), adding or changing a dependency or build tool, deleting user data, or broadly restructuring the repository.

## Decided — do not change without asking

- **Architecture and stack:** Clean Architecture with MVVM presentation and command-based input ([architecture.md](docs/architecture.md#architectural-styles)).
- **Product scope:** [product-behavior.md](docs/product-behavior.md).
- **Interaction model and key bindings:** [ux-gui.md](docs/ux-gui.md).
- **License:** Apache-2.0 for all project code.
- **Process:** numbered phases and milestones (`P1-M2`), per [development-process.md](docs/development-process.md).

## Environment

- Rust stable (`rustfmt`, `clippy`), Xcode Command Line Tools, Qt 6.11.2+ (`brew install qt`), and LLVM `clang-format`/`clang-tidy` (`brew install llvm`; override with `CLANG_FORMAT`/`CLANG_TIDY`).
- CXX-Qt finds Qt via `QMAKE`, then `qmake6`/`qmake` on `PATH`. Set `QT_VERSION_MAJOR=6` if Qt 5 is also installed:

  ```sh
  export QMAKE="$(brew --prefix qt)/bin/qmake"
  ```

## Commands

```sh
make build        # Cargo build
make run          # launch the two-Browser window
make install      # link the release binary as ~/.local/bin/dual-pane (BIN_DIR overrides)
make test         # Cargo tests
make fmt          # Rust and C++ formatters
make fmt-check    # formatter checks
make lint-rust    # Clippy, warnings denied
make lint-cpp     # build desktop crate, then clang-tidy
make check        # full gate: formatters, linters, tests

scripts/check-settings-window.sh          # offscreen Settings Window lifecycle
scripts/check-text-editor-focus.sh        # text fields keep clipboard keys; composed text commits once
scripts/check-application-title-row.sh    # Sidebar title row and close button
scripts/check-folder-items-context-menu.sh # asynchronous Folder Items menu lifecycle
scripts/check-single-instance.sh          # forwarding, isolation, races, socket cleanup
scripts/clipboard-acceptance.sh           # opt-in guided Finder check; replaces the global clipboard; not in make check
```

Debug builds accept `DUAL_PANE_TEST_PROFILE` (an existing absolute disposable directory) for isolated native verification; the settings, text-editor, title-row, single-instance, and clipboard scripts use it. Release builds use the real home directory.

Run `make check` before calling a change done. Formatting is fixed in `rustfmt.toml` and `.clang-format` (no line-length limit); Clippy and clang-tidy have no line-length diagnostic.

## Code conventions

- Default `rustfmt`; no Clippy warnings.
- `unsafe` only for CXX-Qt bridges and the macOS file calls in `crates/dual-pane-desktop/src/native_calls.rs` (the only module that calls `libc`). Any other `unsafe` needs a `// SAFETY:` comment.
- Return errors as `Result`; no `unwrap()`/`expect()` on I/O in non-test code. File failures must reach the user.
- No `phase` words (`phase`, `phase-1`, `phase_1`, …) in source names: crates, modules, files, types, functions, variables, constants, tests, build or script targets. Name code after the concept it owns.
- No `MVP` (any case) in source code or scripts, including comments and strings; it belongs only in `docs/`.

## Safety for a file manager

- File-system tests and examples run inside a temporary directory (e.g. `tempfile`). Never use real paths such as `~`, `/`, or `/Users/...`.
- Follow the safeguards in [product-behavior.md](docs/product-behavior.md) and the [data-safety invariants](docs/architecture.md#44-data-safety-invariants).

## Dependencies and licensing

- Approved dependencies need Apache-2.0-compatible licenses (MIT, Apache-2.0, BSD, Zlib). No GPL-only crates.
- Qt is linked dynamically under the LGPLv3. Never link it statically.

## Documentation

- **Placement:** `README.md` is user-facing; `AGENTS.md` holds agent and contributor rules; `docs/` holds design, research, process, and plans; `docs/archive/` holds finished or superseded docs.
- **Format:** lowercase kebab-case filenames. Start with a title, a `Status:` line, then a one- to three-sentence summary. Phase overviews are `phase-N.md`, never `phase.md`. Add every new doc to [docs/README.md](docs/README.md) with one line.
- **Milestone checklists** replace "Done when" sections; do not mark a milestone `Done` until every item is checked and the gate passes.
- **Keep docs current:** update docs in the same change as the code or decision. Update `README.md` for user-visible features, requirements, or build steps; update this file for changed commands, tooling, or decisions.
- **Link, don't copy.** Each fact lives in one place.
- **Archive, don't delete:** set the status line, `git mv` into `docs/archive/` (a finished `docs/phase-N/` moves whole into `docs/archive/phases/`), and fix every link. Archived docs are not maintained.

## Token efficiency

- Never read or search `target/`. Search with `rg` or `git grep`. Skip `Cargo.lock`, `LICENSE`, and `docs/archive/` unless needed.
- Read docs narrowly: start at `docs/README.md`; for long docs list headings with `rg -n '^#' <file>` and read only the section you need.
- Use `-q --message-format=short` with cargo and filter long logs (`2>&1 | rg 'error|warning' | head -n 40`); look for `error:` before reading C++ output.
- While iterating, test only what is affected: `-p <crate>` for a Qt-free crate (`dual-pane-domain`, `-application`, `-adapters`) skips the Qt/C++ build. Run the full gate once at the end.
- Look up APIs at the source: for a bridged type, its `#[cxx_qt::bridge]` module; for CXX-Qt, the pinned version (`rg -A1 'name = "cxx-qt"' Cargo.lock`) under `~/.cargo/registry/src/`; for Qt, one header under `$(brew --prefix qt)`.
- Edit, don't rewrite. Don't re-read a file to confirm an edit. Check `git diff --stat` before a full diff.
- Reply briefly; summarize output instead of pasting it. The repo is small: search it directly rather than delegating searches.
