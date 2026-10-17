# P4-M5: Deliver clipboard commands

Status: Planned

This milestone exposes Copy, Cut, and Paste through the desktop interface and the approved macOS clipboard behavior. The [architecture plan](milestone-05-architecture.md) defines native delivery; this checklist is the definition of done.

## Scope

In scope: context-menu and keyboard entry points, private clipboard writing and observation, Finder-compatible file Copy publishing and reading, Paste availability and feedback, owner-approved Cut cue, shortcut-editor integration, accessibility, and native verification.

Out of scope: replacing the existing direct other-Browser commands, new file-operation conflict policy, importing non-file clipboard content, exporting Cut as a Finder move, and external drag and drop.

## Completion checklist

- [ ] Before implementation, the owner decides [P4-M5 menus, Cut cue, and clipboard feedback](milestone-01-overview.md#deferred-owner-decisions), and the accepted behavior is recorded in [UX GUI](../ux-gui.md).
- [ ] Copy, Cut, and Paste use the M1-approved shortcut scopes and defaults in the M5-approved menus, with stable action IDs that do not collide with text editing or reserved macOS shortcuts.
- [ ] Copy publishes both Dual Pane's private transfer intent and a Finder-compatible representation of all exact selected Items, including files, folders, packages, symbolic links, and multiple Items. The Finder representation remains usable after Dual Pane quits or relaunches until the clipboard is replaced; Cut publishes only the private intent. Paste into Dual Pane accepts a valid private intent or Finder-copied file references. The private intent expires on relaunch; Finder-sourced Paste is a Copy and remains eligible while its clipboard content is unchanged. Unsupported, malformed, stale, or non-file contents cannot start a Dual Pane operation and receive M5-approved feedback.
- [ ] Availability and Cut feedback track clipboard ownership, selection, active tab, and destination changes without implying a move completed before Paste succeeds.
- [ ] Clipboard and inline-editor keyboard focus are correctly separated; copy or paste of text in a focused editor retains ordinary text-editing behavior.
- [ ] Native macOS checks use temporary source and destination folders to Copy one and multiple files, folders, packages, and symbolic links in Dual Pane and Paste them into Finder before and after Dual Pane quits or relaunches, until clipboard replacement; verify the resulting Item types, contents, and unchanged sources. Copy the same Item types in Finder and Paste them into either Dual Pane Browser, including after Dual Pane relaunches; verify safe Copy results and unchanged Finder sources. Also cover Copy, Cut, Paste within and across Browsers, private-intent expiry on relaunch, clipboard replacement by another app, rejection of malformed or non-file content, conflict decision, and cancellation in a disposable profile; record results here.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M5 Done.

## Acceptance evidence

Pending M5 owner decisions and implementation.
