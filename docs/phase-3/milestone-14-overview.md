# P3-M14: Basic Settings and Keyboard Shortcuts

Status: Planned

This milestone fills the Settings Window with approved basic preferences and editable keyboard shortcuts while retaining application-owned validation, persistence, and live command rebinding. [Architecture](milestone-14-architecture.md) defines the boundary; this checklist defines completion.

## Owner decisions

The owner approved these decisions on 2026-10-11. They supersede the earlier one-shortcut-per-action editor proposal and the same-context collision allowance in [P3-M2](milestone-02-architecture.md#actions-and-shortcut-settings). No implementation may invent another basic setting or shortcut policy.

| Decision | Approved behavior |
|---|---|
| General | Show only the existing “Don’t show notices at startup” preference and whole-app Reset Settings. The preference applies immediately and stays synchronized with its Notices control. Reset keeps its existing explicit confirmation, closes Settings after confirmation, and restores the established workspace and settings defaults. |
| Window heading and help | Show **Settings** as visible text at the upper left of the Settings Window. Place a small local bitmap circled **i** beside each General control and each shortcut action. A click opens a detailed, item-specific overlay anchored to that item; Escape or an outside click dismisses it and returns focus to its button. The button and explanation are keyboard accessible. |
| Shortcut table | Show every catalogued action in a dark, scrollable modern-style table with Action, Shortcuts, Scope, and Restore Default columns. Each action has an accessible description. Show its shortcuts as individually removable entries and provide an Add shortcut control in its row. |
| Cardinality and grammar | An action may have any number of distinct shortcuts, including zero; impose no application-defined count cap. Each shortcut is one key with any combination of Command, Control, Option, and Shift. Modifier-only input and multi-step chords are unsupported. |
| Key set | Support A–Z, 0–9, arrows, Return, backward Delete, Forward Delete, Space, Tab, Escape, F1–F24, and the punctuation keys minus, equals, left/right bracket, backslash, semicolon, apostrophe, comma, period, slash, and grave. No other key is assignable. |
| Plain-key protection | Letters, digits, punctuation, arrows, Return, either Delete, Space, Tab, and Escape require at least one of Command, Control, or Option; Shift alone does not qualify. Function keys may stand alone or use any supported modifiers. |
| Key identity and labels | Normalize a printable key to its keyboard layout's unshifted logical character and store Shift separately: Shift+1 is **Shift+1**, not **!**. Display written modifier names in **Control+Option+Shift+Command+Key** order. Use **Delete** for backward deletion and **Forward Delete** for forward deletion. Labels and help are English; stable stored ActionIds are never localized. |
| Capture | Add enters a focused capture state. It consumes the captured key event and suppresses all workspace shortcuts, including application-scope bindings, until capture ends. Bare Escape cancels capture and bare Tab moves focus; modified Escape and Tab can be captured. Invalid input leaves the current entries intact and shows an inline reason. |
| Conflicts | A shortcut is unique across all actions regardless of scope. Reject a duplicate within the same action or one owned by another action; for the latter, name the owning action in inline feedback. Never silently move or remove another binding. |
| Reserved combinations | Reject fixed Settings `Command+,` and known macOS combinations: Command+H, Command+M, Command+Tab, Command+Grave, Command+Space, Control+Space, Command+Option+Escape, and Command+Shift+3/4/5. Each remains reserved when additional modifiers are held. Explain the rejection inline. This fixed list does not claim to detect every user-configured system shortcut. |
| Removal and defaults | Removing the final entry deliberately unbinds an action. Restore Default replaces *all* of that action's entries with its single compiled default, or an empty list if none exists. A default already owned by another action is rejected by the same conflict rule. |
| Apply, migration, and recovery | An accepted change updates effective in-memory bindings and live dispatch immediately, then schedules the existing settings save. Transactionally migrate each old action's valid binding to its first list entry and retain explicit unbinding. Preserve other settings and session data. Fresh profiles and Reset Settings use compiled defaults. On a save failure, keep effective in-memory values for this launch, dismiss Settings, disable its entry points, and offer the existing Notices recovery route. |

## Scope and fixed inputs

Use the [P3-M2 action and shortcut settings contract](milestone-02-architecture.md#actions-and-shortcut-settings), [P3-M9 startup preference and reset behavior](milestone-09-overview.md), [shortcut editor UX](../ux-gui.md#future-shortcut-editor), and P3-M13. Finish P3-M13's remaining native verification before claiming P3-M14 complete. In scope: the approved General controls, shortcut rows for every catalogued action, typed application commands and projections, live rebinding, persistence, validation feedback, tests, and native verification.

Out of scope: any unrecorded basic preference, a second settings store, Qt-owned persistence, changing a file-operation safety policy, or a dependency/build-tool change without the approval required by AGENTS.md.

The three file and folder visibility settings approved for [P3-M15](milestone-15-overview.md#owner-decisions) are delivered in that milestone, not in P3-M14's basic-settings inventory or migration.

## Completion checklist

- [x] The owner-complete decision table above records the basic-settings inventory and keyboard-capture, modifier, conflict, migration, and reset policies before implementation.
- [ ] Settings shows its visible top-left heading, the approved General controls, and accessible item-level bitmap information buttons with anchored explanation overlays.
- [ ] Keyboard Shortcuts shows every catalogued action with its visible name, all effective shortcuts, scope, accessible description, Add and per-entry Remove controls, and per-row Restore Default.
- [ ] Typed Qt-free application commands validate each proposed binding, update effective in-memory state immediately on acceptance, schedule the existing settings save, and expose typed rejection and effective state to the bridge. Qt capture and rendering never become a second validation or persistence authority.
- [ ] Accepted shortcut changes rebind every entry live. Capture cannot dispatch a workspace action; invalid, reserved, or conflicting proposals leave all prior entries intact and show the approved inline feedback.
- [ ] Multiple shortcuts and deliberate unbinding survive relaunch and Reset restores compiled defaults. The compatible migration preserves existing valid single bindings, while a failed save retains the established Settings dismissal and Notices recovery behavior.
- [ ] Application, storage, adapter, bridge, and desktop tests cover the decision table; native macOS verification covers live updates, capture, conflict/error feedback, help overlays, Restore Default, reset, focus, accessibility, and relaunch persistence.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; documentation and acceptance evidence are recorded before P3-M14 is marked Done.
