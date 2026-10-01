# P2-M1 overview: Decide two-pane interaction details

Status: Done
This documentation-only milestone records the agreed two-pane activation and keyboard model before P2-M2 through P2-M4 depend on it. The canonical user-facing interaction table is in [mvp.md](../../mvp.md#interaction).

## Goal

Record the agreed interaction model for pane activation and keyboard access to the existing browsing commands.

## Scope

**In:**
- Canonicalize the agreed interaction model in [mvp.md](../../mvp.md#interaction).
- Record the resolved key bindings under "Decided" in [AGENTS.md](../../../../AGENTS.md).
- Preserve implementation neutrality for P2-M2 through P2-M4.

**Out:**
- Implementing the interaction model or changing application behavior.
- Designing bindings for tabs, file operations, or other features outside Phase 2.
- Planning the technical implementation details of P2-M2, P2-M3, or P2-M4.

## Completion checklist

- [x] The canonical table covers activation, selection, keyboard focus, pointer, keyboard, visual state, zero rows, and no-selection and inactive-window cases. Evidence: 2026-09-30 manual review of [mvp.md](../../mvp.md#interaction).
- [x] The canonical table maps supported mouse and keyboard gestures to pane activation, row selection, clear selection, row activation, parent navigation, or no-op. Evidence: 2026-09-30 manual review of [mvp.md](../../mvp.md#interaction).
- [x] [mvp.md](../../mvp.md#interaction) records the agreed interaction model, and AGENTS.md records concrete key bindings under "Decided." Evidence: 2026-09-30 link and decision-record review.
- [x] The phase changelog records the owner decision; [phase.md](phase.md), this overview, [milestone-01-architecture.md](milestone-01-architecture.md), and [docs/README.md](../../../README.md) link consistently. Evidence: 2026-09-30 link check with `rg`; all referenced documents and anchors were present.
- [x] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass. Evidence: 2026-09-30 all commands exited 0; Cargo emitted only the pre-existing C++ archive-table warning from `ranlib`.
