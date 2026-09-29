# Architecture

Status: Active

This document defines Dual Pane's Clean Architecture boundaries, technical stack, physical workspace layout, and safety constraints. [mvp.md](mvp.md) owns product scope and user-visible behavior.

## 1. Goals and constraints

The architecture prioritizes the following, in order:

1. **Data safety.** An operation must never silently overwrite or destroy user data. A conflict or destructive operation reaches an explicit policy and user-decision boundary.
2. **Responsiveness.** A person can continue navigating, selecting, changing tabs, and cancelling work while listings, watches, persistence, or file operations are in progress. The GUI thread never enumerates directories, watches locations, copies data, or performs other potentially blocking I/O.
3. **Correctness.** The workspace, pending decisions, and jobs have a single logical owner and change only through ordered application inputs.
4. **Testability.** Domain rules and use cases run without Qt, macOS, a real file system, or a scheduler.
5. **Replaceable details.** Qt, CXX-Qt, macOS APIs, storage formats, and concurrency mechanisms are outer details, not sources of application policy.
6. **Simplicity.** Boundaries exist to protect a dependency or test seam. The design does not require a crate, DTO, trait, or presenter for every noun.

The fixed technical constraints are Rust 2024; Qt 6.11.2+ Widgets; CXX-Qt (`cxx-qt`, `cxx-qt-lib`, and `cxx-qt-build`); Cargo-only builds; C++17 or newer; and macOS 26.7+ only. Qt is linked dynamically under LGPLv3. QML/Qt Quick, CMake, Corrosion, qmake project files, Windows or Linux support, a GUI-dependent core, and blocking work on the GUI thread are outside this design.

Dual Pane runs as a single application window containing one workspace; there is no multi-window mode. It is built and run locally without notarization and without the App Sandbox, so file access is governed by ordinary permissions and macOS privacy (TCC) grants. Those grants are tied to the code signature and can be lost when a locally signed build changes, so a privacy denial is a routine, recoverable error rather than an edge case.

## 2. Dependency rule and logical rings

Clean Architecture is defined by **source-code dependency direction**, not by runtime call direction or directory nesting. Code in an inner ring must not name a type, API, storage format, or framework from an outer ring. Dependencies between Dual Pane's logical modules point inward:

```
                  source-code dependencies point inward

  ┌──────────────────────────────────────────────────────────────┐
  │ Frameworks and drivers                                        │
  │ Qt Widgets/C++, CXX-Qt bridge, macOS APIs, runtime, gateways, │
  │ file-system and settings implementations                      │
  │   ┌──────────────────────────────────────────────────────┐   │
  │   │ Interface adapters                                   │   │
  │   │ controllers and presenters (Qt-free)                 │   │
  │   │   ┌──────────────────────────────────────────────┐   │   │
  │   │   │ Application                                   │   │   │
  │   │   │ use cases, workspace state, requests, outputs │   │   │
  │   │   │   ┌──────────────────────────────────────┐   │   │   │
  │   │   │   │ Domain                               │   │   │   │
  │   │   │   │ file-manager rules and value types   │   │   │   │
  │   │   │   └──────────────────────────────────────┘   │   │   │
  │   │   └──────────────────────────────────────────────┘   │   │
  │   └──────────────────────────────────────────────────────┘   │
  └──────────────────────────────────────────────────────────────┘

                 composition root lives at the outer edge
```

Runtime control often travels in the other direction: a Qt action becomes an application input, the application returns a work request, and an outer runtime carries it out with macOS facilities and later returns a result as another application input. That is valid because the application owns the request and result types and never imports the implementation.

The outermost implementations necessarily import Qt or macOS SDKs. Those third-party imports are confined to their outer components; they are not permission for application or domain code to do so.

### Allowed dependencies

| Logical ring | May depend on | Must not depend on |
|---|---|---|
| Domain | Rust standard library and small platform-neutral value utilities | Application, Qt, CXX-Qt, macOS APIs, I/O, threads, storage, logging backends |
| Application | Domain | Qt, CXX-Qt, macOS types, widgets, platform I/O, concrete schedulers, storage formats |
| Interface adapters | Application and Domain | Qt, CXX-Qt, widget ownership, raw macOS file APIs, application-policy decisions |
| Frameworks and drivers | Inner public contracts and the necessary platform/framework APIs | Inward implementation details or policy of their own |
| Composition root | Every public concrete type needed for wiring | Reusable business policy |

An outer ring may translate its native representation at the boundary. Its translated value, rather than a `QString`, Objective-C object, file descriptor, `NSError`, or Qt model index, crosses inward.

## 3. Boundary ownership

The workspace uses four crates to enforce these boundaries:

| Crate | Ring | Dependencies |
|---|---|---|
| `dual-pane-domain` | Domain | No Dual Pane crate dependencies. |
| `dual-pane-application` | Application | `dual-pane-domain`. |
| `dual-pane-adapters` | Interface adapters | `dual-pane-application` and `dual-pane-domain`. |
| `dual-pane-desktop` | Frameworks, drivers, and composition root | The three inner crates plus Qt, CXX-Qt, and macOS implementation dependencies. |

The C++ Qt Widgets shim and `build.rs` live in `dual-pane-desktop`. Modules within a crate retain the ring rules above; a Cargo dependency may never point outward.

### 3.1 Domain — stable file-manager policy

The domain ring contains rules that remain meaningful if the UI, operating system, and execution model change. It performs no I/O and knows nothing about asynchronous work.

| Area | Responsibility |
|---|---|
| Value types | Pane side, tab ID, operation ID, request/version token, location, entry name, file kind, sort specification, and capability values. Their representations are platform-neutral. |
| Workspace structure | `Pane`, `Tab`, tab history, `Selection`, and `Cursor` types that enforce their own invariants: each pane has at least one tab and exactly one active tab, history positions stay in range, and cursor and selection refer only to entries in the current listing. |
| Policies | Operation intent validity; conflict, kind-mismatch, and destructive-operation decision requirements; directory-merge semantics; safe-operation semantics; stale-result identity rules; sort ordering; and other validation that does not require I/O. |
| Errors | Stable, user-relevant categories with operation and path context, never native error objects. |

`Location` and entry identity are platform-neutral domain values, not leaked native paths or file handles. A platform adapter may supply an opaque stable identity when one is available; the domain must not assume that an identity survives a rename, a volume boundary, or every provider-backed location.

### 3.2 Application — use cases and application state

The application ring coordinates the product's use cases. It owns the single `Workspace` aggregate, built from the domain's pane and tab types plus the asynchronous state the domain does not model. The state is an implementation detail of the application's input boundary, not a UI model and not a Qt object.

| Area | Responsibility |
|---|---|
| Input boundary | Commands and externally observed facts: navigate, select, tab operations, file-operation requests, decisions, cancellation, listing results, watcher invalidations, operation step results, and settings load/save results. |
| Workspace use cases | Validate inputs, evolve the workspace state, issue work requests, reject stale results, and produce application output. |
| Workspace state | Two panes of domain tabs; outstanding request tokens and loading state; file operations with their plans, progress, and pending decisions; and preferences required to carry out use cases. It has one serialized owner. |
| Output models | Immutable, framework-neutral values describing panes, jobs, decisions, and recoverable errors. Listing changes are row deltas (insert, remove, update) against the previous listing so that views keep scroll position, cursor, and selection. They state *what changed*, not how a widget redraws it. |
| Work requests | Typed, purpose-specific descriptions of outside work the use cases need: read a directory, execute an operation step, start or stop watching a location, load or save settings, and open a file with its default application. |

Use cases are a pure reducer. The application never calls outward, holds a callback, or waits:

```
Workspace::handle(input)
  → validate and transition workspace state
  → (application outputs, zero or more work requests)
```

A runtime in the frameworks ring carries out each work request and later submits its typed result as a new input. The application owns the request and result types; there are no application-owned I/O traits to implement or fake. Work requests are purpose-specific; there is no universal `FileSystem` façade whose broad API encourages business logic to drift outward. For example, a directory-read request carries the location, sort and filter specification, and the previous listing snapshot needed to compute row deltas, and an operation-step request carries an already validated intent and the decisions that apply to it.

`handle` must not return a Qt `ViewChange`, manipulate a model, choose dialog text, or start a thread. A multi-step flow is modelled as explicit state identified by tokens, not as a suspended call.

### 3.3 Interface adapters — translate, do not decide

Interface adapters translate between the user interface and the application boundary. They are plain Rust with no Qt or CXX-Qt dependency, so they are tested without the Qt/C++ build. They reshape data for a screen, but they do not contain a second set of file-manager policy.

| Adapter | Responsibility |
|---|---|
| Input controller | Maps Qt-free UI events (for example `UiEvent::DropOnPane { pane, row }` or a menu command identifier) plus current view context into application commands. It does not decide whether a command is valid. |
| Presenter | Maps application output into plain-Rust view-models: row view-models with display text and icon kind, row-delta instructions, operation status, and dialog view-models listing the permitted choices. It owns formatting and wording, not policy or the set of available user actions. |

The Qt delivery driver performs only the mechanical translation from Qt signals, actions, and model indexes to `UiEvent` values and from view-models to Qt model notifications and widgets.

The presenter may keep rendering caches that can be reconstructed from application output. Cursor, selection, active pane, pending conflict decisions, and other behaviorally significant state remain in the application ring.

### 3.4 Frameworks and drivers — replaceable mechanics

This outer ring owns concrete technology and resource lifetime. It includes the C++ Qt Widgets view, CXX-Qt bridge objects, Qt item-model machinery, macOS file-system and desktop-service calls, FSEvents (or a replacement), settings serialization, worker implementation, and native error conversion.

| Driver | Responsibility |
|---|---|
| Qt delivery | Constructs the single window and its widgets, owns Qt object lifetime, translates Qt signals into `UiEvent` values for the input controller, and applies presenter view-models to Qt models and dialogs. CXX-Qt types stay here because they name a framework. |
| Runtime | Receives work requests from the reducer, dispatches them to gateways on the worker pool, owns cancellation primitives and native handles, and delivers every typed result as an application input on the GUI thread. A worker, watcher, or dialog never mutates workspace state directly. |
| File-system gateway | Reads directories, then sorts, filters, and computes row deltas against the previous listing snapshot on a worker. Scans operation sources and executes operation steps using macOS facilities. Enforces syscall-level protections such as no-overwrite behavior and classifies native errors into application error categories. |
| Watcher gateway | Watches a requested location and reports invalidation; it never refreshes a pane or changes state itself. |
| Settings gateway | Stores application-defined settings values at an application-support location using a driver-chosen format and atomic replacement writes. |

Infrastructure code may contain FFI and `unsafe`, but it is isolated here. Each unsafe boundary is minimal and documented with a `SAFETY:` explanation. Native types, file descriptors, Qt objects, and platform error objects never escape this ring.

### 3.5 Composition root — wiring only

The executable entry point creates concrete drivers and the runtime, creates the workspace, controller, and presenter, and starts the Qt event loop. It is allowed to know every concrete type. It must not introduce business rules, recover errors, or decide operation outcomes.

## 4. Data and control flow

### 4.1 User command and rendering

```
Qt signal / drag-and-drop / menu action
  → Qt delivery driver: UiEvent
  → input controller: application command
  → Workspace::handle
  → application outputs (+ work requests, see 4.2)
  → presenter: view-models
  → Qt delivery driver: Qt model notifications or dialog
```

Menus, toolbars, drag-and-drop, tests, and future shortcuts all use the same command boundary. The interface adapter may report invalid input, but it does not replace application validation with widget-specific rules.

### 4.2 Blocking work and external events

```
work request returned by Workspace::handle
  → runtime dispatches to a gateway on a worker
  → macOS / storage / watcher
  → typed result
  → runtime delivers it on the GUI thread as an application input
  → Workspace::handle
  → new outputs and work requests
```

The runtime, not the application, chooses threads, queues, cancellation handles, and wake-up mechanics. The application requires only that inputs are processed in order by one owner. The Qt delivery layer binds that owner to the GUI thread so that output reaches models there, while all blocking work stays outside it.

Every asynchronous request has a request/version token. The application records the token when issuing work and ignores a result that no longer describes the current tab, location, or operation generation. A blocked network or provider-backed location may delay a worker but cannot overwrite newer state or freeze the UI.

### 4.3 File operations and decisions

File operations cross two boundaries: policy decides whether the operation may proceed and which decision is needed; the outer executor performs the actual file-system work safely. The application drives the operation step by step, so no worker ever waits for a person.

1. An application command creates a validated operation intent and a new operation ID. A destructive command that needs confirmation first becomes a pending decision.
2. The application requests a source scan. A worker returns the operation plan: the ordered items to process, with symlinks recorded as links and never traversed.
3. The application issues an execution step for the plan starting at a given item, together with the decisions that apply to it, and records the operation as running.
4. The executor processes items until the step finishes, the operation is cancelled, or it reaches an item that needs a decision: a destination conflict, a file/folder kind mismatch, or a recoverable error. It then stops, reports typed progress and the item's outcome, and releases its worker. It never chooses a resolution on its own. A directory whose destination is an existing directory is merged without a decision; only the items inside it can conflict.
5. A required decision becomes application output. The presenter displays the permitted choices and returns the selected choice as an application command.
6. The application validates that the choice still belongs to the pending operation and item, records it (including any "apply to all" choice, scoped to that operation), and issues the next step from that item. Cancelling simply issues no further step and requests cleanup of the operation's known partial artifacts.
7. When the operation finishes, the application requests affected listings to refresh.

Product-level delete behavior, conflict choices, and confirmation policy are defined in [mvp.md](mvp.md); no driver may infer them from a menu label or platform default.

## 5. Ownership, concurrency, and safety

| Resource | Owner | Rule |
|---|---|---|
| Widgets, `QObject`s, and Qt item models | Qt delivery driver on the GUI thread | A worker never reads or writes them. |
| Workspace state | Serialized application input boundary | Changes only while processing an application command or event. |
| Work request lifecycle | Application use case | Identified by request/operation tokens; only matching events affect state. |
| Operation plan, progress, and pending decisions | Application use case | Held in workspace state between steps; no worker holds a pending decision. |
| Worker handles, queues, file descriptors, native buffers | Outer runtime and gateway drivers | Never escape as domain or application types. |
| Rendering caches | Presenter/Qt driver | Rebuildable from application output and never authoritative for policy. |

### 5.1 Threading and responsiveness

The desktop runtime uses a small, bounded set of execution contexts. This is a threading policy, not a commitment to a particular executor or synchronization crate:

| Execution context | Owns and may do | Must not do |
|---|---|---|
| GUI/application thread | Qt event loop; widgets and Qt models; the serialized application input boundary; bounded state transitions; output delivery and rendering. | Synchronous file, provider, watch, persistence, process, or other unbounded work; waiting for a worker; holding a lock while invoking Qt or the application. |
| Worker runtime | Blocking directory reads; listing sort, filter, and delta computation; operation scans and steps; settings I/O; native calls; and other CPU work that could delay an event-loop turn. It emits typed events only. | Read or write Qt objects, presenter state, or workspace state; decide product policy; synchronously wait for GUI processing. |
| Native callback sources | Minimal watcher or platform callbacks that enqueue an invalidation or typed result. | Refresh a listing, perform I/O, update a model, or call application code re-entrantly. |

The worker runtime is bounded and demand-driven: it uses a configured finite capacity or equivalent controlled execution resource, never an unbounded thread-per-request design. Capacity and scheduling may evolve with measurement, but foreground interaction always has a free GUI event-loop turn; pending work waits in a cancellable queue rather than creating more threads. The runtime may run independent reads or operations concurrently when their resources and safety constraints permit it. It must serialize steps that depend on one another and must prevent conflicting writes to the same operation destination from racing.

Every GUI-thread action is short and non-waiting. Application transitions process one input at a time and return to Qt promptly; a large batch of worker results is drained in bounded slices over successive event-loop turns. Directory-listing results arrive already sorted and filtered, as token-identified row deltas against the previous listing (or bounded batches with a final completion event); the GUI thread never sorts or diffs a listing. Progress updates are rate-limited or coalesced, and repeated watcher invalidations for the same location are coalesced into one refresh request. These rules prevent a fast worker, a large directory, or an event storm from starving input, painting, dialogs, or accessibility processing.

Cancellation is cooperative and observable. Cancelling a queued request removes it before execution when possible; cancelling running work signals the worker without blocking the GUI thread. Workers check cancellation at safe boundaries, report a typed terminal event, and release native resources before their handle is discarded. A worker that is slow, blocked, or cannot be interrupted may finish later, but its token and cancellation state ensure that it cannot change newer workspace state.

Locks, if the chosen runtime needs them, protect only driver-local mutable resources and have a narrow lifetime. Workspace state is not shared behind a mutex: its serialized input owner is its synchronization mechanism. Qt thread affinity and application event ordering therefore remain visible architectural rules rather than accidental properties of locking.

The architecture requires these data-safety invariants:

- A conflict is established by the write primitive or an equivalent atomic reservation, never only by a check-then-write race. Name equality is therefore whatever the destination file system decides (including APFS case and Unicode rules); the application never pre-compares names to decide safety.
- A replacement must not destroy an existing destination until the replacement data is complete and the final switch is safe for the relevant object and volume.
- Replacement applies only to a file replacing a file. A directory arriving at an existing directory is merged; a file/folder kind mismatch is never resolved by replacing either side.
- A cross-volume move completes each destination copy successfully before removal of its corresponding source.
- Recursive work never follows symlinks: a symlink is copied or moved as a link, and work remains inside its intended source and destination trees. Navigating into a symlinked directory is ordinary navigation, not recursive work.
- Cancellation leaves completed work intact and removes only known partial artifacts created by the cancelled operation.
- Errors retain operation and location context. Drivers classify native failures; application chooses recovery choices; presentation chooses wording and accessibility treatment.

## 6. Error, settings, and observability boundaries

Application errors distinguish conditions that affect available choices: item missing, destination exists, file/folder kind mismatch, permission denied, privacy restriction, no space, item busy, read-only location, cross-device behavior, cancellation, and unknown failure. The macOS gateway maps native errors into those categories and may retain driver-only diagnostics for logging. The application decides which categories are recoverable and therefore offer the choices defined in [mvp.md](mvp.md#file-operation-safeguards).

Settings and session values are application-owned value types. The application requests a save of a coalesced session snapshot shortly after any session-relevant change and once more on quit, so a crash loses at most the most recent changes. The file format, storage location, atomic-write mechanism, and corrupt-data handling belong to the settings driver. A failed or corrupt load becomes a recoverable application event; it never makes a serialization format part of domain policy.

Logging is an outer concern. Structured operation context may be carried in application outputs and emitted by a driver, but domain and application decisions must not depend on a logging backend succeeding.

## 7. Verification strategy

| Boundary | Primary tests | Evidence |
|---|---|---|
| Domain | Unit and property tests | Pane, tab, history, selection, and cursor invariants and policy choices hold without I/O. |
| Application | Unit and property tests of `Workspace::handle` | Commands/events yield the correct state, outputs, work requests, token rejection, row deltas, and operation/decision lifecycle, with no fakes. |
| Adapters | Focused controller/presenter tests without Qt | UI events map to the right application commands; outputs become view-models without duplicating policy. |
| Drivers | Temporary-directory and Qt smoke tests | Error mapping, exclusive writes, metadata behavior, symlink safety, settings atomicity, model notifications, GUI-thread confinement, cancellation, and bounded event delivery. |
| End-to-end | Small opt-in macOS scenarios | Volume, privacy, File Provider, watcher, and desktop-service behavior that fakes cannot reproduce. |

Tests that touch a file system stay in a temporary directory. No test uses a real user path, and the domain, application, and adapter test suites require neither Qt nor macOS facilities.

Property tests generate input sequences and check that invariants hold, for example: the cursor and selection always refer to the current listing; only the latest token changes a tab's listing; applying emitted row deltas to the old listing yields the new listing; an operation terminates exactly once and rejects decisions that are not pending; and a restored session equals the saved one apart from unrestorable tabs. A property-testing dev-dependency requires approval under [AGENTS.md](../AGENTS.md) when the first such test is written.

## 8. Deliberate non-decisions

This architecture does not decide exact macOS API choices, concurrency primitive, worker-capacity value, operation step size, storage format, sort collation rules, session-save delay, or performance thresholds. Distribution is limited to local, unnotarized, unsandboxed builds (see [§1](#1-goals-and-constraints)); packaging tooling is not decided. Product choices and remaining key-binding work are defined in [mvp.md](mvp.md).

Those choices may vary, but each must preserve the dependency rule, ownership model, and data-safety invariants in this document.
