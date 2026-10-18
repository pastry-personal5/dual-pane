# P4-M4 architecture

Status: Done

Clipboard transfer is another entry point to the existing application-owned file-operation workflow. The [overview checklist](milestone-04-overview.md#completion-checklist) defines acceptance; P4-M1 fixes the transfer and safety boundary, and the accepted [M4 owner decisions](milestone-04-overview.md#owner-decisions) fix repeat, rejection, destination, and preflight behavior.

## Existing constraints and required extension

Today `OperationIntent` stores one source folder plus names below it. That is sufficient for selection in one Dual Pane tab, but a valid Finder clipboard can contain roots from different parent folders. Treating their common ancestor as the source would change destination names or recreate unwanted path segments, while splitting one Paste into independent jobs would change conflict, progress, cancellation, and feedback behavior. M4 therefore extends the operation model to represent a list of byte-exact source roots, each with its captured kind, and one destination folder. Each root retains its own leaf name; descendants map relative to that root.

Add a domain value such as `OperationRoot { source: Location, kind: EntryKind }` and a transfer constructor for `OperationIntent` in `crates/dual-pane-domain/src/operation.rs`. Keep a narrow constructor or capture adapter for existing same-folder commands so Copy/Move to Other Browser does not change behavior. Validation rejects an empty root list, duplicate byte-exact roots, a destination equal to a source parent for an internal same-folder Paste, and a destination within any directory root. Two distinct roots with the same leaf name are not silently deduplicated: they reach the ordinary destination-conflict workflow in stable clipboard order.

Update `crates/dual-pane-desktop/src/operation_scan.rs` and the Qt-free plan checks in `crates/dual-pane-application/src/operations.rs` to calculate every destination relative to its owning root rather than a single shared source. The scanner continues to use no-follow metadata, records native identity before execution, and never traverses a symbolic link. Presentation and refresh code must derive affected source folders and destination roots from all roots. Rename, New Folder, Trash, and permanent deletion retain their existing shapes and behavior.

This is a representation change, not a second executor. Existing `operations.rs`, `operation_lane.rs`, `operation_step.rs`, the safety journal, conflict decisions, replacement, cross-volume move verification, cancellation, cleanup, and operation refresh remain authoritative.

## Typed clipboard state machine

Add UI-neutral clipboard inputs and outputs in `crates/dual-pane-application/src/input.rs` and `output.rs`, application-owned state in `workspace.rs`, and any operation-origin metadata needed to consume a Cut at terminal completion. Exact type names may adjust during implementation, but the contracts are:

- `ClipboardGeneration` is an opaque native change identifier used only for equality and staleness. The application never increments or interprets it.
- `ClipboardRequestId` correlates a publication or read request with its acknowledgement so a late result is harmless.
- `TransferId` identifies a private Copy or Cut captured during this process launch. Native private content carries this ID, not source paths that could be trusted after application state is gone.
- `ExternalFileReferences` contains byte-exact absolute `Location` values translated from the native clipboard. Text renderings are never accepted as source identity.
- `TransferRoot` contains a byte-exact absolute `Location` and `EntryKind` for operation creation, obtained from a ready internal listing or a no-follow external preflight.
- Application state distinguishes a captured transfer awaiting publication, the current acknowledged transfer and generation, a destination-frozen pending Paste, and a Cut reserved by a started operation.

Copy and Cut enter as observed commands stamped with Browser, tab, and visible revision. Capture reuses the existing ready-listing checks and visible-selection reconciliation, but captures roots rather than the other Browser's destination. Copy produces a publication request containing the private ID and exact roots so M5 can also publish Finder-compatible file references. Cut produces a request containing only the private ID. A failed or stale publication acknowledgement does not make the proposed transfer current.

Paste is available only while the active tab has confirmed, successfully loaded Folder Items with no pending load or error. Invocation captures that destination immediately, allocates a request ID, and asks the desktop boundary to read the current clipboard. The returned typed snapshot carries the request ID and generation. It is accepted only for the still-pending request and captured destination. Clipboard-change input immediately invalidates future use of an acknowledged private transfer and any pending read or preflight for an older generation; it does not alter an operation already created. A destination that becomes unavailable or unwritable after capture is reported by the worker and is never replaced with the newly active tab's folder.

Private content resolves through `TransferId` to state from this launch and must match the acknowledged generation. It never falls back to paths embedded in an unknown or stale private payload. Finder content arrives as typed, byte-exact file references and can request `OperationKind::Copy` only. Its generation belongs to the read result, so current Finder content remains eligible after relaunch without persisted application clipboard state.

A Finder snapshot first receives structural validation and then produces a tagged worker request to preflight every referenced root without following links. The result returns roots in clipboard order with their no-follow kinds. Regular files, directories including packages, and symbolic links are supported; malformed or duplicate references, unavailable roots, and other native kinds reject the entire Paste before an operation ID exists. No valid subset is silently copied. A result is accepted only when its request, clipboard generation, and destination-frozen Paste are still current. The operation scanner then rechecks no-follow kind and native identity to close the preflight-to-execution race; a later source change follows the ordinary scan recovery workflow. The outer layer translates native facts but never decides transfer policy.

## Cut lifecycle and terminal outcomes

A Cut is a pending Move, never an eager mutation. A rejection before an operation ID exists leaves it pending. Once its Paste creates an operation, mark that transfer reserved so another Paste cannot start a second Move from it. Keep a transfer-to-operation association separate from clipboard availability: clipboard replacement may invalidate future Paste while the running job still needs terminal cleanup. When that operation first reaches any terminal outcome—success, failure, cancellation, partial completion, or uncertain cleanup—consume the Cut exactly once.

Copy remains reusable until clipboard replacement. Each accepted Paste creates a new immutable operation intent, including while an earlier operation from the same Copy is still running. A later clipboard, selection, filter, tab, or Browser change cannot alter any pending or running operation.

## Availability, rejection, and presentation boundary

M4 emits typed availability and rejection reasons for: no visible selection, stale observed revision, source listing unavailable, publication failure, clipboard replaced, unknown private transfer, unsupported or malformed external content, duplicate roots, source unavailable or changed, destination unavailable, same-folder transfer, and destination within a source. M5 maps those reasons to menus, disabled states, Notices, and accessible wording; M4 does not embed widget or pasteboard concepts in domain or application types.

Existing `OperationRejected` may be extended or a clipboard-specific output may be introduced if that keeps command availability and command acceptance on one validation path. Do not overload `OperationCommand::Copy` to mean clipboard capture: direct Copy to Other Browser and clipboard Copy have different destinations and availability.

## Implementation sequence

1. Generalize transfer roots in the domain intent, scanner, application coordinator checks, affected-folder refresh, and presenters. Preserve existing command constructors and tests before adding clipboard state.
2. Add generation-, transfer-, and request-tagged clipboard inputs, outputs, state, and pure transition tests. Add observed Copy/Cut capture and ready-destination Paste handling without native clipboard access.
3. Add the tagged, all-or-nothing Finder-source preflight request and result, including rejection of stale results and every unsupported root set.
4. Connect accepted private and Finder snapshots to the generalized operation workflow, including concurrent Copy reuse and Cut reservation, retry after pre-start rejection, and terminal consumption.
5. Add integration tests with temporary directories for preflight/scan races, mixed-parent roots, conflicts, cancellation, partial results, refresh, and verified-before-removal behavior. M5 later supplies the native pasteboard adapter and Finder acceptance checks.

## Verification focus

State-machine tests must permute publication acknowledgements, clipboard changes, Paste reads, tab changes, and operation completion rather than covering only the happy path. Domain and application tests prove stable root order, exact byte identity, mixed parents, same-name conflicts, descendant rejection, relaunch/private-ID rejection, and immutable running intents. Desktop operation tests use only temporary directories and prove that source identity is checked again before mutation, symbolic links are copied or moved as links, Copy never removes a source, and Move removal follows verified destination work.
