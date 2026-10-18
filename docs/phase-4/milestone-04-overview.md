# P4-M4: Model clipboard transfer safely

Status: Done

This milestone adds the Qt-free clipboard-transfer state machine and connects a validated Paste to the existing file-operation safety workflow. The [architecture plan](milestone-04-architecture.md) defines the typed clipboard boundary, race handling, and operation-model extension; this checklist is the definition of done.

## Scope

In scope: byte-exact source capture from Dual Pane, typed snapshots of Finder file references, private-transfer identity, clipboard publication/read acknowledgements, replacement events, pending Cut state, destination capture and validation, Paste operation creation, mixed-parent Finder selections, conflict and cancellation reuse, partial-result behavior, and Qt-free availability and rejection outputs under the M1 rules.

Out of scope: desktop menus, pasteboard formats, macOS clipboard access, Finder integration, and user-facing clipboard wording or Cut decoration, which belong to P4-M5; replacing the existing Copy/Move to Other Browser commands; changing conflict choices or deletion rules.

## Owner decisions

On 2026-10-18, the owner accepted the following behavior for M4.

| Topic | Accepted behavior |
|---|---|
| Copy repeat | Copy remains available for repeated Paste until the clipboard is replaced. Each Paste creates an independent frozen Copy operation, and operations from the same Copy may overlap. |
| Cut rejection | A Paste rejected before an operation starts leaves the Cut pending so the person can correct the destination or source condition and try again. Once a Cut Paste starts, it is reserved against a second start and is consumed at every terminal outcome. |
| Destination unavailable | Paste requires a ready active tab with a confirmed, successfully loaded folder and no pending load or error. It is unavailable otherwise. The worker still detects a destination that becomes unavailable or unwritable after invocation. |
| Finder-source preflight | Before starting a Finder-sourced Copy, asynchronously validate every byte-exact root without following links. Files, folders and packages, and symbolic links are supported. Any malformed, duplicate, unavailable, or unsupported root rejects the entire Paste; no partial job starts. The operation scan revalidates accepted roots to cover later races. |

## Completion checklist

- [x] Before implementation, the owner decides every item in [Owner decisions](#owner-decisions), and the accepted rules are recorded in product behavior and the Phase 4 changelog.
- [x] Copy and Cut are observed-selection commands: they capture only explicit selected Items still present in the ready visible projection, preserve byte-exact locations and kinds, and never infer identity from display text. Cut alone changes no file.
- [x] The application models publication, read, replacement, and failure as generation- and request-tagged transitions. A late acknowledgement or clipboard read cannot resurrect replaced content, overwrite a newer transfer, or start work against a newer destination.
- [x] Copy exposes the frozen selection for M5 to publish both a private transfer reference and Finder-compatible file references without starting a Dual Pane operation. Cut exposes only the private reference. Publication failure does not claim that new clipboard state became active.
- [x] Paste captures the active tab's confirmed destination when invoked. A later tab, Browser, navigation, selection, filter, or clipboard change cannot retarget a pending read, preflight, or running operation.
- [x] A private transfer resolves only to application-owned state from this launch and current clipboard generation. Relaunch, replacement, an unknown private ID, or a mismatched generation rejects it without falling back to untrusted path text.
- [x] A typed Finder snapshot can authorize Copy only after structural validation and the owner-approved preflight. It accepts byte-exact files, folders, packages, and symbolic links from one or several parent folders; malformed, duplicate, unavailable, unsupported, or non-file payloads cannot bypass operation validation. Finder sources remain in place, including after relaunch while their clipboard content is current.
- [x] The operation model represents arbitrary frozen source roots without losing their leaf names or flattening their descendants incorrectly. Existing direct Copy/Move to Other Browser behavior and its single-folder capture API remain compatible.
- [x] Same-folder, destination-inside-source, unavailable-destination, missing or changed source, stale visible selection, duplicate source, same-name roots, and partial-result cases follow the accepted rules and produce typed rejection or operation output without unsafe fallback.
- [x] A started Cut Paste is reserved against a second start and is consumed at every terminal outcome, including failure, cancellation, and partial completion. Clipboard replacement affects only future Paste; it cannot retarget or cancel the frozen job.
- [x] Existing scan, conflict, progress, cancellation, cleanup, operation Notice, safety-journal, and affected-folder refresh paths are reused. Copy never removes a source, and Move removes each source only after its destination result is verified.
- [x] Qt-free tests cover both Browsers, inactive tabs, filtered selection, stale observed revisions, publication/read acknowledgement races, clipboard replacement, relaunch semantics, repeated Copy Paste per the owner decision, rejected and started Cut Paste, mixed-parent Finder roots, duplicate and same-name roots, multiple Item kinds, conflicts, source changes between capture/scan, partial move, cancellation, and affected-folder refresh.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M4 Done.

## Acceptance evidence

On 2026-10-18, the owner decided that Copy supports repeated and overlapping Paste operations until clipboard replacement; a rejected-before-start Cut remains pending; Paste requires a ready, successfully loaded destination tab; and Finder sources receive an asynchronous, no-follow, all-or-nothing preflight before an operation starts. The decisions are recorded in [product behavior](../product-behavior.md) and the [Phase 4 changelog](changelog.md).

Implementation added explicit mixed-parent operation roots; generation-, request-, and transfer-tagged clipboard transitions; observed visible-selection capture; destination-frozen Paste; asynchronous no-follow external-source preflight; Copy reuse; Cut reservation and terminal consumption; bounded cleanup of superseded captured state; typed rejection output; and root-aware scan, plan validation, lane scopes, conflict presentation, execution safeguards, and affected-folder refresh. Tests cover state-machine races, filtered and stale capture, failed publication with prior-content preservation, replacement invalidation, same-folder retry, overlapping Cut reservation, mixed parents, duplicate and nested roots, multiple kinds, relaunch-compatible external snapshots, repeated Copy, terminal Cut consumption, root-relative scanning, and no-follow symlink preflight. Existing operation-workflow tests continue to cover conflicts, cancellation, partial results, verified-before-removal behavior, cleanup, Notices, and refresh.

Acceptance commands passed on 2026-10-18: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (all tests passed; one existing desktop test remained ignored), and `make check`, including Rust and C++ formatting, Clippy, clang-tidy, and tests.
