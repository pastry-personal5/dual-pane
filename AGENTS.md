# AGENTS.md

Instructions for AI coding agents and contributors working in this repository.

This file is the **single source of agent instructions**. Do not create `CLAUDE.md` or any other agent-specific instruction file, whether as a copy, a symlink, or a stub. Put new guidance here.

## Project

`dual-pane` is a dual-Browser file manager for macOS, written in Rust with a Qt 6 Widgets UI. Version 2.0.0 has been released; it shows two dark Browsers that read real directories on separate workers. The Qt-free domain, application, and adapter crates own and test Folder Items, selection, navigation, and error behavior. File operations are not included yet.

See [README.md](README.md) for the user-facing overview, [docs/product-behavior.md](docs/product-behavior.md) for product scope, and [docs/architecture.md](docs/architecture.md) for architecture. Design and process docs are indexed at [docs/README.md](docs/README.md).

## Agent workflow

- Work autonomously only on a clearly scoped task within the active milestone. [docs/roadmap.md](docs/roadmap.md) identifies the active phase; follow its milestones in order unless its phase plan says otherwise.
- Before starting implementation, read the active phase and milestone plans, the relevant design docs, and the applicable items under "Undecided" below.
- Before executing a milestone, add a completion checklist to its overview doc. The checklist is its executable definition of done; it replaces a narrative "Done when" section.
- Check an item only after its acceptance evidence exists. Keep implementation, tests, and directly affected documentation in the same change.
- A milestone is complete only when every checklist item is checked and `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass. If a command cannot run, leave its related item unchecked and report the exact blocker.
- Ask before deciding an item in "Undecided", adding or changing a dependency or build tool, deleting user data, or broadly restructuring the repository. Everything else that is within the approved milestone and task scope may proceed without a separate approval.

## Decided — do not change without asking

- **Architecture and technical stack:** [architecture.md](docs/architecture.md).
- **Product scope:** [product-behavior.md](docs/product-behavior.md).
- **Interaction model:** [ux-gui.md](docs/ux-gui.md).
- **Concrete Phase 2 key bindings:** [ux-gui.md](docs/ux-gui.md#input-and-active-browser-state).
- **License:** Apache-2.0 for all project code.
- **Development process:** work is organized in numbered phases (Phase 1, 2, 3, …). Each phase has numbered milestones (Milestone 1, 2, 3, …), and milestone numbering restarts in every phase. Milestone IDs look like `P1-M2`. See [docs/development-process.md](docs/development-process.md).

## Undecided — ask before inventing

The product and architecture decisions above leave no open items.

## Environment setup

- Rust stable via rustup, plus the `rustfmt` and `clippy` components.
- Xcode Command Line Tools for the C++ compiler.
- Qt 6, for example from `brew install qt`. CXX-Qt locates Qt through `qmake`. It checks the `QMAKE` env var first, then looks for `qmake6` or `qmake` on `PATH`. If both Qt 5 and Qt 6 are installed, set `QT_VERSION_MAJOR=6`.
- LLVM's `clang-format` and `clang-tidy`, for example from `brew install llvm`. The repository scripts locate Homebrew LLVM automatically; `CLANG_FORMAT` and `CLANG_TIDY` can override their paths.

  ```sh
  export QMAKE="$(brew --prefix qt)/bin/qmake"
  ```

The desktop requires Qt 6.11.2+ and dynamically links its Widgets framework through CXX-Qt.

## Commands

```sh
make build                                   # Cargo build
make run                                     # launch the two-Browser Qt Widgets window
make test                                    # Cargo tests
make fmt                                     # Rust and C++ formatters
make fmt-check                               # formatter checks
make lint-rust                               # Clippy with warnings denied
make lint-cpp                                # builds the desktop crate, then configured clang-tidy
make check                                   # full formatter, linter, and test gate
```

Before you consider a change done, run `make check` and the equivalent direct commands. Rustfmt uses `max_width = 1000000` and `use_small_heuristics = "Max"`; `.clang-format` uses `ColumnLimit: 0`. Clippy and clang-tidy deliberately have no line-length diagnostic.

## Architecture

Follow the dependency, ownership, concurrency, and implementation-boundary rules in [architecture.md](docs/architecture.md).

## Code conventions

- Use default `rustfmt` formatting. No `clippy` warnings.
- Limit `unsafe` to what CXX-Qt bridges require. Any other `unsafe` block needs a `// SAFETY:` comment that explains why it is sound.
- Return errors as `Result` and don't `unwrap()` or `expect()` on I/O in non-test code. File operations fail routinely (permissions, missing files, full disks) and must be reported to the user.
- Don't use phase words such as `phase`, `phase-1`, `phase1`, or `phase_1` in source-code names: crates, modules, files, types, functions, variables, constants, tests, or build and script targets. Name code after the concept it owns.
- Don't use `MVP` in any form (`MVP`, `mvp`, `Mvp`) anywhere in source code or scripts, including identifiers, comments, and strings. Those terms belong only in `docs/`.

## Safety rules for a file manager

- Tests and examples that touch the file system must work inside a temporary directory (for example, with the `tempfile` crate). Never use real user paths such as `~`, `/`, or `/Users/...`.
- Follow the file-operation safeguards in [product-behavior.md](docs/product-behavior.md) and the data-safety invariants in [architecture.md](docs/architecture.md).

## Dependencies and licensing

- Ask before adding or changing a dependency or build tool. Approved dependencies must have licenses compatible with Apache-2.0, such as MIT, Apache-2.0, BSD, or Zlib. Don't add GPL-only crates.
- Qt is linked dynamically and used under the LGPLv3. Don't introduce static linking of Qt.

## Documentation

- **Where docs go:**
  - `README.md` is user-facing.
  - `AGENTS.md` holds the rules for contributors and agents.
  - `docs/` holds design, research, process, and plan docs.
  - `docs/archive/` holds docs that are no longer active.
- **Format:** filenames are lowercase kebab-case. Each doc starts with a title, then a `Status:` line, then a one- to three-sentence summary.
- **Index:** add every new doc to [docs/README.md](docs/README.md) with a one-line description.
- **Phases and milestones:** phase plans and milestone status follow [docs/development-process.md](docs/development-process.md).
- **Phase-plan filenames:** every phase overview is named `phase-N.md`, where `N` is its phase number; never use a generic `phase.md`.
- **Milestone checklists:** each milestone overview has a completion checklist instead of a "Done when" section. Check an item only after its acceptance evidence exists; do not mark the milestone `Done` until every item is checked and the full gate passes.
- **Keep docs current:**
  - Update docs in the same change as the code or decision they describe.
  - Update `README.md` when user-visible features, requirements, or build steps change.
  - Update this file when build commands, tooling, or project decisions change, or when an item under "Undecided" gets decided.
- **Link, don't copy.** Each fact lives in one place, and other docs link to it.
- **Archive, don't delete:**
  1. When a doc is finished or superseded (for example a completed phase plan), set its status line.
  2. `git mv` it into `docs/archive/`. A finished phase directory (`docs/phase-N/`) moves as a whole into `docs/archive/phases/`.
  3. Fix every link to it.

  Archived docs are not maintained.

## Token efficiency

This file is loaded in every session, so keep it short. Long-form material belongs in `docs/`.

- **Skip generated and bulky files.**
  - Never read or search `target/`. It holds build output, including CXX-Qt-generated C++ and moc files.
  - Search with `rg` or `git grep`, which skip ignored files.
  - Don't open `Cargo.lock`, `LICENSE`, or `docs/archive/` unless the task needs them.
- **Read docs narrowly.** Start with `docs/README.md`, then read only what the task needs. For long docs such as `docs/architecture.md`, list the headings with `rg -n '^#' <file>` and read just the relevant section.
- **Keep build output small.**
  - Use `-q` and `--message-format=short` with cargo, for example `cargo clippy -q --message-format=short --all-targets -- -D warnings`.
  - Filter long logs, for example with `2>&1 | rg 'error|warning' | head -n 40`. Look for `error:` before reading C++ compiler output.
- **Iterate narrowly, verify once.**
  - While iterating, run only the affected tests: `cargo test -q <filter>`, or `-p <crate>` for a Qt-free crate (`dual-pane-domain`, `dual-pane-application`, `dual-pane-adapters`), which skips the Qt/C++ build.
  - Run the full gate once, before calling the change done.
- **Look up APIs at the source.**
  - For a bridged type, read its `#[cxx_qt::bridge]` module, not the generated headers.
  - For CXX-Qt, find the pinned version with `rg -A1 'name = "cxx-qt"' Cargo.lock`, then grep that version's source under `~/.cargo/registry/src/`.
  - For Qt, grep the one header you need under `$(brew --prefix qt)` instead of browsing framework directories.
- **Ask before building on an undecided item.** Redoing work is the most expensive outcome.
- **Edit, don't rewrite.**
  - Change files with targeted edits.
  - Don't re-read a file to confirm an edit.
  - Check `git diff --stat` before reading a full diff.
- **Reply briefly.** Summarize command output and diffs instead of pasting them. This repo is small, so search it directly instead of delegating searches to subagents.
