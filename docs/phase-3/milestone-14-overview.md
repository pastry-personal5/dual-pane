# P3-M14: Basic Settings and Keyboard Shortcuts

Status: Planned

This milestone fills the Settings Window with approved basic preferences and editable keyboard shortcuts while retaining application-owned validation, persistence, and live command rebinding. [Architecture](milestone-14-architecture.md) defines the boundary; this checklist defines completion.

## Decision gate

Before implementation starts, the owner records in this overview the exact additional basic settings and the complete keyboard-shortcut design: capture grammar, supported keys/modifiers, unbinding, reserved combinations, conflict presentation and resolution, localization, migration, defaults, persistence, and reset behavior. No implementation may invent a setting or shortcut policy absent from that table.

The following decisions are already fixed: valid changes apply immediately; each shortcut row offers Restore Default; General contains the existing “Don’t show notices at startup” preference and whole-app Reset Settings; Reset retains its explicit confirmation, closes Settings after confirmation, and performs the established workspace reset; Settings stays unavailable when storage is unhealthy; and fixed macOS `Command+,` remains reserved for Settings and cannot be assigned to an ActionId.

## Scope and fixed inputs

Use the [P3-M2 action and shortcut settings contract](milestone-02-architecture.md#actions-and-shortcut-settings), [P3-M9 startup preference and reset behavior](milestone-09-overview.md), [Future shortcut editor](../ux-gui.md#future-shortcut-editor), and P3-M13. In scope: the decision gate, approved General controls, shortcut rows for every catalogued action, typed application commands and projections, live rebinding, persistence, validation feedback, tests, and native verification.

Out of scope: any unrecorded basic preference, a second settings store, Qt-owned persistence, changing a file-operation safety policy, or a dependency/build-tool change without the approval required by AGENTS.md.

## Completion checklist

- [ ] The owner-complete decision table exists before implementation and identifies every additional basic setting and every keyboard-capture/conflict behavior required by this milestone.
- [ ] General exposes the existing Notices Startup Checkbox and Reset Settings with their established immediate-apply, confirmation, reset, failure, and Settings-dismissal behavior, plus only owner-approved additional settings.
- [ ] Keyboard Shortcuts lists every catalogued action with its visible name, effective shortcut, scope, accessible description, and per-row Restore Default control.
- [ ] Typed Qt-free application commands validate each proposed binding or setting, update effective in-memory state immediately on acceptance, schedule the existing settings save, and expose the resulting state to the bridge. Qt capture and rendering never become a second validation or persistence authority.
- [ ] Accepted shortcut changes rebind live. Capture input cannot dispatch the workspace action it names. Invalid, reserved, or conflicting proposals follow the owner-recorded policy and leave the prior effective binding intact unless that policy explicitly says otherwise.
- [ ] Every approved new setting and binding persists across relaunch, has a compatible migration where needed, resets to its recorded default, and preserves the existing failed-save behavior: effective in-memory state remains, Settings becomes unavailable, and Notices provides recovery.
- [ ] Application, storage, adapter, bridge, and desktop tests cover the decision table; native macOS verification covers live updates, capture, conflict/error feedback, Restore Default, reset, focus, accessibility, and relaunch persistence.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; documentation and acceptance evidence are recorded before P3-M14 is marked Done.
