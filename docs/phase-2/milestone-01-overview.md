# P2-M1 overview: Decide two-pane interaction details

Status: Planned
This milestone records the agreed two-pane activation and keyboard model before P2-M2 through P2-M4 depend on it. It produces documentation only.

## Goal

Record an agreed interaction model for pane activation and keyboard access to the Phase 1 browsing commands.

## Scope

**In:**
- Decide how pointer interaction activates a pane and how the active pane is indicated.
- Decide the keyboard bindings needed to switch panes and invoke the existing navigation commands.
- Ensure mouse and keyboard invoke the same application commands.
- Record the decision in the appropriate product/process documentation and remove the resolved key-binding item from AGENTS.md's "Undecided" section.

**Out:**
- Implementing the interaction model or changing application behavior.
- Designing bindings for tabs, file operations, or other features outside Phase 2.
- Planning the technical implementation details of P2-M2, P2-M3, or P2-M4.

## Agreed interaction model

- Both panes initially show the launch working directory. The left pane is active and its list has keyboard focus.
- A primary click anywhere in a pane activates it first. A row click selects the row; a row double-click selects and activates it; an empty-list click clears that pane's selection; a scrollbar click leaves selection unchanged; and an Up-control click activates the pane then goes to its parent. Secondary and middle clicks do nothing in this phase. Pointer selection modifiers are ignored.
- Keyboard focus entering either pane's list activates that pane. `Option+F` switches active pane and moves focus to its list while preserving both panes' selection and scroll position. Plain Tab remains available to macOS Full Keyboard Access.
- Up, Down, Home, End, Page Up, and Page Down select rows in the active pane. Movement at a boundary is a no-op. With no selection, Home, Page Up, Up, and Down select the first row; End and Page Down select the last. Shift-, Command-, and Option-modified movement keys do nothing.
- Return activates the selected row and is a no-op without a selection. Command-Up goes to the active pane's parent and is a no-op at root.
- Pointer and keyboard inputs use the same application commands: pane activation, row selection, row activation, and parent navigation.
- In a key window, the active list has a one-pixel `#2F6D9A` outline; the Up control retains the neutral border. Active selected rows use `#2F6D9A`, and inactive-pane selected rows use `#1E4668`, both with white text. In an inactive application window, neither list has an accent outline and both selected rows use `#1E4668`.

## Completion checklist

- [ ] This overview explicitly records activation, selection, keyboard focus, pointer, keyboard, and visual-state behavior, including no-selection and inactive-window cases.
- [ ] A mouse/keyboard-to-command mapping records that equivalent gestures use pane activation, row selection, row activation, and parent-navigation commands.
- [ ] [mvp.md](../mvp.md) records the agreed interaction model, and AGENTS.md moves concrete key bindings from "Undecided" to "Decided".
- [ ] The phase changelog records the owner decision; [phase.md](phase.md), this overview, [milestone-01-architecture.md](milestone-01-architecture.md), and [docs/README.md](../README.md) link consistently.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with acceptance evidence recorded before this milestone is marked Done.
