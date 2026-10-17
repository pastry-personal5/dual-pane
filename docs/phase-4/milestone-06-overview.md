# P4-M6: Navigate by entered path

Status: Planned

This milestone turns the existing read-only Path Edit Control into a navigation entry point. The [architecture plan](milestone-06-architecture.md) defines validation and state handoff; this checklist is the definition of done.

## Scope

In scope: editing and submitting the path under M1 syntax rules, validation feedback, focus and shortcut behavior, normal tab history, pending and failed navigation behavior, and native verification.

Out of scope: a new workspace window, network mounting, shell command interpretation, path completion unless separately accepted during M6 planning, and breadcrumb or expanded-column interaction, which belongs to P4-M8.

## Completion checklist

- [ ] Before implementation, the owner decides [P4-M6 path validation and focus details](milestone-01-overview.md#deferred-owner-decisions), and the accepted behavior is recorded in product behavior or UX GUI.
- [ ] The active Browser's Path Edit Control accepts and submits an M1-approved folder path; successful navigation updates that tab's location and history through the ordinary navigation workflow.
- [ ] Empty, malformed, missing, non-directory, unreadable, and slow paths receive distinct appropriate feedback. A failure leaves the last successful Folder Items and path available under the existing recovery policy.
- [ ] Switching tabs or Browsers while editing follows the M1 draft and focus rules; a late read cannot navigate a tab after a newer request.
- [ ] Editor keystrokes do not trigger workspace file commands; Escape, Return, focus loss, selection, and accessibility behavior follow the M1 core policy and M6-approved details.
- [ ] Qt-free and native tests cover exact and non-UTF-8 paths where supported, symlinks, root, history, failure, cancellation, and tab switching using temporary directories and a disposable profile.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M6 Done.

## Acceptance evidence

Pending M6 owner decisions and implementation.
