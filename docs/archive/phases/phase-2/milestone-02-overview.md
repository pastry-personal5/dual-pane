# P2-M2 overview: Model two independent panes

Status: Done

This milestone replaces the one-pane Qt-free workspace state with two independent pane states and one application-owned active pane. It implements the interaction model's commands below the UI boundary; P2-M3 remains responsible for rendering two panes and translating Qt gestures.

## Goal

Make `Workspace::handle` model two concurrently usable directory panes so a command or listing result for either pane cannot alter the other pane's location, listing, selection, pending read, or error outcome.

## Scope

**In:**

- Add the platform-neutral left/right pane-side value in `dual-pane-domain`.
- Replace the application crate's private one-pane state with left and right pane states plus one active-pane value.
- Make browsing commands, listing events, outputs, and work requests explicitly pane-addressed.
- Add application commands to activate a pane and clear that pane's selection, completing P2-M1's agreed command vocabulary.
- Preserve globally unique request tokens while making cancellation and terminal-result acceptance independent per pane. Pane-addressed commands do not implicitly activate their target; P2-M3 submits the explicit activation required by a gesture before its pane command.
- Add public-API application tests for activation, independent listings, selection, navigation, cancellation, errors, and stale results.

**Out:**

- A second Qt pane, changed C++ widget behavior, CXX-Qt bridge surface, runtime, or filesystem-gateway behavior. P2-M3 owns the visible two-pane migration and canonical gesture translation.
- Application policy in adapters or desktop code. The existing one-pane adapter and desktop call sites may receive only the mechanical explicit-`Left` updates necessary to compile against the pane-addressed application boundary.
- Keyboard movement, scrolling, focus transfer, visual styling, tabs, file operations, watching, persistence, row deltas, or opening regular files.
- New dependencies or build tools.

## Completion checklist

- [x] `dual-pane-domain` exposes a copyable `PaneSide` value for exactly left and right, and the application has exactly two private pane states plus an application-owned active pane that begins on the left. Evidence: `pane_side.rs` unit test and `two_pane_workspace.rs` activation test passed on 2026-09-30.
- [x] Every P2-M2 browsing command, listing event, output, and work request carries a pane side; activation and clear-selection commands use the same application boundary. Evidence: `two_pane_workspace.rs` compiled and passed on 2026-09-30.
- [x] A pane-targeted transition changes only that pane and never implicitly changes the active pane; activation changes only the active-pane value. Successful replacement clears only the target pane's selection. Evidence: `two_pane_workspace.rs` independent-listing, selection, and activation tests passed on 2026-09-30.
- [x] Request tokens remain globally unique, while a newer navigation cancels only its own pane's pending read and a missing, stale, duplicated, or wrong-pane terminal event is a no-op. Evidence: `two_pane_workspace.rs` cancellation-and-terminal-events test passed on 2026-09-30.
- [x] Application integration tests cover initial/explicit activation, independent locations/listings/selections, cross-pane navigation while the other pane loads, clear selection, failures, cancellation, and terminal-event rejection. Evidence: five `two_pane_workspace.rs` integration tests passed on 2026-09-30.
- [x] [phase.md](phase.md), this overview, [milestone-02-architecture.md](milestone-02-architecture.md), [changelog.md](changelog.md), and [docs/README.md](../../../README.md) link consistently; `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass before the milestone is marked Done. Evidence: all three commands exited 0 on 2026-09-30.
