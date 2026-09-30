# P2-M2 architecture: Model two independent panes

Status: Done

P2-M2 moves behaviorally significant two-pane state into the Qt-free domain and application rings. It leaves the existing single-pane presentation behavior intact until P2-M3 can replace it coherently; only mechanical explicit-`Left` call-site updates are allowed in adapters and desktop code so the workspace continues to compile.

## State and ownership

`dual-pane-domain` adds `PaneSide` with exactly `Left` and `Right`. It is a small copyable, comparable value; it is not a Qt index, widget handle, or layout position. P2-M2 needs no opposite-side helper: P2-M3 can select the other side when it implements the `Option+F` gesture.

`Workspace` owns two private instances of the current pane-local state (`Listing`, pending navigation, and `Selection`) and one `active_pane: PaneSide`. `Workspace::new()` initializes `active_pane` to `Left`; neither pane has a listing or pending request until explicitly navigated. The request-token issuer remains workspace-wide, rather than one counter per pane, because the later single runtime must distinguish simultaneous reads without adding a composite token type.

Read-only accessors take a `PaneSide` and expose only the requested pane's committed location, entries, loading location, and selection; `active_pane()` exposes the current side. They do not expose mutable pane state or an application UI model.

## Pane-addressed reducer boundary

P2-M2 replaces the one-pane command, event, output, and work-request shapes with pane-addressed equivalents:

| Boundary value | P2-M2 shape and rule |
|---|---|
| `Command` | `ActivatePane { pane }`, `Navigate { pane, location }`, `SelectEntry { pane, row, name }`, `ClearSelection { pane }`, `OpenEntry { pane, row, name }`, and `GoToParent { pane }`. A command names its target explicitly; it does not implicitly use or change the active pane. The P2-M3 adapter submits `ActivatePane` first for gestures that activate a pane. |
| `Event` | Each listing loaded, failed, or cancelled event carries `pane` and `token`. It is accepted only when both match that pane's pending read. |
| `Output` | Every existing listing, loading, selection, failure, and cancellation output carries `pane`; `ActivePaneChanged { pane }` is emitted only on an actual active-pane change. |
| `WorkRequest` | Directory read and cancellation requests carry `pane` as well as the workspace-global token, so the eventual runtime can return an unambiguous pane-addressed event. |

`ActivatePane` changes no listing, selection, pending read, or work request; the initial `Left` activation emits no output, and every later actual side change emits exactly one `ActivePaneChanged`. `ClearSelection` clears only the named pane and emits its pane-addressed selection output only if non-empty. Selection, entry activation, and parent navigation validate and operate solely against the named pane's committed listing, never changing `active_pane` as a side effect. A successful listing replacement clears only the target pane's selection; loading, failure, and cancellation retain the same target-local behavior already proven for one pane.

When navigation replaces a pending read, only that pane's prior token receives a cancellation request. A terminal event with a token that is unknown, already terminal, associated with the other pane, or no longer pending is ignored without affecting either pane. This preserves the current late-result guarantee while allowing both panes to load concurrently.

## File-level implementation plan

1. Add `pane_side.rs` and re-export `PaneSide` from `dual-pane-domain/src/lib.rs`; add focused domain tests for the two values and their equality/copy behavior.
2. Update `dual-pane-application/src/input.rs`, `output.rs`, and `work_request.rs` so the public reducer boundary is explicitly pane-addressed and includes activation and clear-selection commands.
3. Refactor `dual-pane-application/src/workspace.rs` around two private pane states selected by `PaneSide`. Keep listing state, validation, and request-token issuance in the application ring; do not move state into adapters or desktop code.
4. Update existing application tests and their support helpers for explicit pane sides, then add a `two_pane_workspace.rs` public-contract suite for the cross-pane cases below. Existing one-pane behavior remains asserted for each named side.
5. Update the existing single-pane adapter and desktop session only where the changed application boundary requires it: submit `Left` with their existing events and initial navigation, and consume pane-addressed outputs for that same one-pane path. Do not add a right-side view model, a second session or model, Qt/C++ signals, keyboard handling, or visual changes. P2-M3 owns the full adapter/session/bridge migration.

## Verification

Application tests must prove the following observable behavior without filesystem access or Qt:

- Newly created workspaces have two empty panes and `Left` active; activating `Left` is a no-op, activating `Right` emits one active-pane output, and activating `Right` again is a no-op.
- Left and right can each load different locations and entries; selecting, clearing, entering, or navigating on one side leaves the other's committed state and selection unchanged. These pane-addressed commands also leave `active_pane` unchanged, so gesture activation order remains explicit and testable.
- Simultaneous reads use distinct tokens. Replacing a pending left navigation cancels only the left token; right loading continues and can commit normally.
- A correct terminal result changes only its named pane. Replayed, superseded, unknown, and deliberately wrong-pane events are no-ops, including while the other pane has a current pending read.
- A failed or cancelled read preserves the target pane's prior committed listing and selection under the existing rules and never emits output for the other pane.

Run the focused domain and application tests while iterating, then the full required gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
