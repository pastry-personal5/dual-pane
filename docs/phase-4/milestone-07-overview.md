# P4-M7: Drag and drop file items

Status: Planned

This milestone adds file drag and drop within Dual Pane. The [architecture plan](milestone-07-architecture.md) defines the safe handoff; this checklist is the definition of done.

## Scope

In scope: selected-Item drag payloads, Browser folder, folder-row, and Browser Tab targets, copy/move intent and modifier feedback, rejection, operation launch, cancellation, accessibility alternatives, and native verification.

Out of scope: moving Favorites or tabs, recursive search, and drags to or from other apps.

## Completion checklist

- [ ] Before implementation, the owner decides [P4-M7 drop edge cases and feedback](milestone-01-overview.md#deferred-owner-decisions), and the accepted behavior is recorded in product behavior or UX GUI.
- [ ] A drag uses the exact selected source Items and approved target types. Hover feedback identifies the target and copy or move action; unsupported or stale payloads cannot start an operation.
- [ ] Hovering an inactive Browser Tab does not activate it; dropping on that tab targets its confirmed folder without changing the frozen source.
- [ ] Dropping freezes source and destination and uses the same conflict, progress, cancellation, partial-result, cleanup, Notice, and refresh behavior as P4-M4 and existing direct file commands.
- [ ] Same-folder, source-inside-destination, folder/file mismatch, symlink, package, unavailable target, and cancelled-drop cases preserve the existing safety invariants and give clear rejection feedback.
- [ ] Pointer drag does not accidentally reorder Folder Items or change a selection when only beginning or cancelling a drag. Equivalent keyboard and menu actions remain available.
- [ ] Native macOS checks cover both Browsers, folder-row and tab targets (including inactive tabs), modifiers, failure and cancellation, and rejection of external drags using only temporary file data; record results here.
- [ ] Accessible feedback, affected docs, direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass before marking P4-M7 Done. Phase 4 remains open until P4-M8 and every exit criterion are complete.

## Acceptance evidence

Pending M7 owner decisions and implementation.
