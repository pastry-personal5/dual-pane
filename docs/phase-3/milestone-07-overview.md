# P3-M7: Establish safe file-operation workflow

Status: Done

This milestone adds Qt-free operation intent and lifecycle policy. It freezes targets when a command starts, validates decisions against a specific operation and item, and exposes typed work and progress for the later native executor. [Architecture](milestone-07-architecture.md) describes the boundary; this checklist defines completion.

## Scope and fixed inputs

Use the [file-operation safeguards](../product-behavior.md#file-operation-safeguards), [selection and file commands](../ux-gui.md#selection-and-file-commands), [operation panels and decisions](../ux-gui.md#operation-panels-and-decisions), and [application boundary](../architecture.md#43-file-operations-and-decisions). Keep source and destination byte-exact and captured at command acceptance. Multiple operations and their decisions remain independent.

In scope: Qt-free intents for copy, move, rename, new folder, Trash, and permanent deletion; availability and target validation; the permanent-delete confirmation state; typed scan and execution requests and results; progress, conflict and error decisions, per-operation conflict-only apply-to-all, cancellation, stale-result rejection, and terminal summaries. Out of scope: native scanning and file-system mutation, command widgets, Operation Panels, Notices integration, default-application opening, and affected-listing refresh, which P3-M8 delivers. The desktop must not expose these commands until its executor exists.

## Completion checklist

- [x] Domain types validate frozen operation targets, names, destination rules, and decision choices without I/O.
- [x] The application starts operations only from confirmed source state and valid selection; copy and move also require a confirmed, available, different destination. Later tab, location, and selection changes cannot retarget accepted operations.
- [x] Permanent deletion requires an explicit confirmation bound to its frozen operation ID and target count. Cancellation before confirmation issues no scan or execution request.
- [x] Independent jobs support scan, execution, progress, typed conflict and recoverable-error decisions, conflict-only apply-to-all, cancellation, stale-result rejection, and terminal partial/success/failure summaries. A worker never waits for a user decision.
- [x] Qt-free tests cover target capture, unavailable/same destination, stale events and choices, concurrent decisions, confirmation, cancellation, and permitted choices. No test touches real user data.
- [x] Directly affected docs reflect the implemented boundary. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass.

Acceptance evidence: `operation_workflow.rs` exercises the frozen workspace command and synthetic result contract; the desktop runtime test verifies executor-unavailable delivery. The four listed commands passed on 2026-10-02.
