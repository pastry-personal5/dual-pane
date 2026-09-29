#

Status: Deprecated

## 15. Suggested phasing

This uses the IDs from [development-process.md](development-process.md). Once approved, it becomes `docs/roadmap.md` plus phase docs.

**Phase 1: Foundations.** Prove the stack and browse one directory.

- **P1-M1, stack spike.** It covers:
  - a Cargo-only build that links QtWidgets;
  - C++ `run_app()` creating the `QApplication` and a window;
  - a Rust `#[base = QAbstractTableModel]` model showing 100k synthetic rows in a `QTreeView`;
  - a worker thread updating the model through wake-and-drain (§6).

  Done when it runs with `cargo run`, scrolls smoothly, the gate passes, and the open bridge questions in §7 are answered.
- **P1-M2, workspace and listing.** Create the approved layout, implement bulk listing in `dp-fs`, and benchmark it against `read_dir`.
- **P1-M3, interaction model and keymap design.** A design doc only, needed before Phase 2.
- **P1-M4, single-pane browsing.** Navigate, open, go up, sort, and show hidden files. Errors, including privacy denials, reach the user.

**Phase 2: Dual-pane browsing.** Two panes with an active pane, tabs, history, watching with diff updates, the listing cache, and session restore.

**Phase 3: File operations.** The job engine, mkdir and rename, copy, move, delete (after the delete decision), conflict and error dialogs, progress and cancel, drag and drop, and the clipboard.

**Phase 4: Release hardening.** The `.app` bundle, signing, and notarization; privacy flows; a performance pass against §11; and a settings UI.
