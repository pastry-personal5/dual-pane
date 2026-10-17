# P4-M4: Model clipboard transfer safely

Status: Planned

This milestone adds application-owned Copy, Cut, and Paste intent while reusing the existing file-operation safety workflow. The [architecture plan](milestone-04-architecture.md) defines the handoff; this checklist is the definition of done.

## Scope

In scope: captured source identities from Dual Pane and Finder file Copy, cut state, destination validation, paste availability, operation creation, conflict and cancellation reuse, partial-result behavior, and clipboard-change events under the M1 rules.

Out of scope: desktop menus and native clipboard mechanics, which belong to P4-M5; replacing the existing Copy/Move to Other Browser commands; changing conflict choices or deletion rules.

## Completion checklist

- [ ] Before implementation, the owner decides [P4-M4 repeat and preflight details](milestone-01-overview.md#deferred-owner-decisions), and the accepted rules are recorded in product behavior.
- [ ] Copy and Cut capture explicit selected Items and their source folder. Cut alone changes no file; a Paste starts the approved copy or move workflow only after validating the current destination.
- [ ] Copy's frozen selection supplies exact native source identities for every selected Item type and multiple Items for the desktop to publish to Finder without changing source files or starting a Dual Pane operation; internal Paste still requires the private transfer intent.
- [ ] A typed snapshot of Finder-copied file references can become a Copy intent only after validating exact source identities, supported Item types, the current clipboard generation, and the active tab's confirmed destination. Finder sources remain in place; malformed, unavailable, or non-file payloads cannot create an operation. Finder Copy remains eligible after Dual Pane relaunches while the clipboard is unchanged.
- [ ] The source and destination for a running Paste are frozen at command start. Later selection, filter, tab, Browser, or clipboard changes cannot retarget the operation.
- [ ] Same-folder, unavailable-destination, missing-source, stale-selection, and partial-result cases follow the M1 decisions and report actionable status without unsafe fallback.
- [ ] Existing conflict, progress, cancellation, cleanup, operation Notice, and affected-folder refresh paths are reused; a Cut never deletes a source before a verified destination result.
- [ ] Qt-free tests cover cross-tab and cross-Browser Paste, Finder-sourced Copy with multiple Item types, clipboard replacement, two consecutive Pastes, conflicts, partial move, cancellation, stale or malformed external sources, and filtered selection.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M4 Done.

## Acceptance evidence

Pending M4 owner decisions and implementation.
