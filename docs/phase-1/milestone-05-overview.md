# P1-M5 overview: Real directory browsing in one pane

Status: Draft

This partial plan names P1-M5's goal and known scope so the P1-M4 work can hand over cleanly. The architecture doc and the full completion checklist are written before implementation starts.

## Goal

Replace P1-M4's synthetic listing with real directories, and let a person change directory, so that the Phase 1 exit criteria in [phase.md](phase.md) hold.

## Known scope

**In:**

- A real directory reader in `dual-pane-desktop` that plugs into P1-M4's runtime as its listing source. It reads on the worker, classifies entry kinds (including symbolic links to folders), sorts with `listing_sort_key`, checks cancellation, and maps native errors to `ListingErrorKind`.
- `tempfile` as a `dual-pane-desktop` dev-dependency for reader tests, as approved on 2026-09-30.
- Opening a listed folder, and going to the parent, from the window through the adapters' `UiEvent`s.
- Showing a directory that cannot be listed as a visible error.

**Open before planning:** whether keyboard access waits for the key-binding design, which is still undecided in [AGENTS.md](../../AGENTS.md#undecided--ask-before-inventing). The mouse and menus do not depend on it.

## Removing dead and demo code

P1-M5 must end with no dead or demo code. It deletes `synthetic_listing.rs`, the startup wiring that selects it, and its tests; removes anything the real reader makes unused; and adds no `#[allow(dead_code)]` or `#[allow(unused…)]`. The full checklist will require evidence for this, as P1-M4's does: the `allow` search prints nothing, `rg -n -i synthetic crates` prints nothing, and `git status --short` shows no stray files.

## Completion checklist

To be written with the architecture doc, before implementation starts.
