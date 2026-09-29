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

## 2. Dependency rule and logical rings

Clean Architecture is defined by **source-code dependency direction**, not by runtime call direction or directory nesting. Code in an inner ring must not name a type, API, storage format, or framework from an outer ring. Dependencies between Dual Pane's logical modules point inward:

```
                  source-code dependencies point inward

  ┌──────────────────────────────────────────────────────────────┐
  │ Frameworks and drivers                                        │
  │ Qt Widgets/C++, CXX-Qt bridge, macOS APIs, worker runtime,    │
  │ file-system and settings implementations                      │
  │   ┌──────────────────────────────────────────────────────┐   │
  │   │ Interface adapters                                   │   │
  │   │ controllers, presenters, gateway translations        │   │
  │   │   ┌──────────────────────────────────────────────┐   │   │
  │   │   │ Application                                   │   │   │
  │   │   │ use cases, workspace state, ports, outputs    │   │   │
  │   │   │   ┌──────────────────────────────────────┐   │   │   │
  │   │   │   │ Domain                               │   │   │   │
  │   │   │   │ file-manager rules and value types   │   │   │   │
  │   │   │   └──────────────────────────────────────┘   │   │   │
  │   │   └──────────────────────────────────────────────┘   │   │
  │   └──────────────────────────────────────────────────────┘   │
  └──────────────────────────────────────────────────────────────┘

                 composition root lives at the outer edge
```

Runtime control often travels in the other direction: a Qt action invokes an application input, an application use case invokes an injected output port, and a macOS implementation later returns a result as another application input. That is valid because the application owns the port contract and never imports the implementation.

The outermost implementations necessarily import Qt or macOS SDKs. Those third-party imports are confined to their outer components; they are not permission for application or domain code to do so.

### Allowed dependencies

| Logical ring | May depend on | Must not depend on |
|---|---|---|
| Domain | Rust standard library and small platform-neutral value utilities | Application, Qt, CXX-Qt, macOS APIs, I/O, threads, storage, logging backends |
| Application | Domain | Qt, CXX-Qt, macOS types, widgets, platform I/O, concrete schedulers, storage formats |
| Interface adapters | Application and Domain | Widget ownership, raw macOS file APIs, application-policy decisions |
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

The domain ring contains rules that remain meaningful if the UI, operating system, and execution model change. It has no ports because a port represents a use case's need of the outside world, not an enterprise rule.

| Area | Responsibility |
|---|---|
| Value types | Pane side, tab ID, operation ID, request/version token, location, entry name, file kind, sort specification, and capability values. Their representations are platform-neutral. |
| Invariants | Valid pane/tab ownership, selection and cursor consistency, operation intent validity, and the conditions under which a choice is required. |
| Policies | Conflict and destructive-operation decision requirements, safe-operation semantics, stale-result identity rules, and domain-level validation that does not require I/O. |
| Errors | Stable, user-relevant categories with operation and path context, never native error objects. |

`Location` and entry identity are application values, not leaked native paths or file handles. A platform adapter may supply an opaque stable identity when one is available; the domain must not assume that an identity survives a rename, a volume boundary, or every provider-backed location.

### 3.2 Application — use cases and application state

The application ring coordinates the product's use cases. It owns the state previously described as a separate `state` layer: separating it outside the use cases would reverse the dependency direction. The state is an implementation detail of the application's input boundary, not a UI model and not a Qt object.

| Area | Responsibility |
|---|---|
| Input boundary | Commands and externally observed facts: navigate, select, tab operations, file-operation requests, decisions, cancellation, listing results, watcher invalidations, and job progress/completion. |
| Workspace use cases | Validate inputs, evolve the workspace state, issue work requests, reject stale results, and produce application output. |
| Workspace state | Two panes; tabs; histories; locations; selection; loading state; jobs; pending decisions; and preferences required to carry out use cases. It has one serialized owner. |
| Output models | Immutable, framework-neutral snapshots or deltas of panes, jobs, decisions, and recoverable errors. They state *what changed*, not how a widget redraws it. |
| Output ports | Narrow contracts the use cases need from the outside: directory reading, file-operation execution, location watching, session persistence, desktop opening, task dispatch, and application-output delivery. |

The application ring owns every port it calls. Ports are purpose-specific; there is no universal `FileSystem` façade whose broad API encourages business logic to drift into adapters. For example, directory reading exposes the listing data required by navigation, and operation execution accepts an already validated operation intent and reports typed progress, conflict, failure, or completion.

A useful internal shape is a reducer-like transition, but its outputs are application concepts:

```
Application input
  → validate and transition workspace state
  → application output + zero or more work requests
```

It must not return a Qt `ViewChange`, manipulate a model, choose dialog text, own a GUI-thread callback, or start a thread. The scheduler or gateway implements an output port and later submits a typed result through the same serialized application input boundary.

### 3.3 Interface adapters — translate, do not decide

Interface adapters turn delivery and platform-specific forms into the application boundary's forms. They may reshape data for a screen or a gateway, but they do not contain a second set of file-manager policy.

| Adapter | Responsibility |
|---|---|
| Input controller | Maps Qt actions, menus, drag-and-drop, and eventual approved shortcuts into application commands. It does not decide whether a command is valid. |
| Presenter | Maps application output models into screen-oriented models and dialog requests. It owns display concerns such as row roles and formatting, not policy or wording choices that change available user actions. |
| Gateway translator | Maps the application ports' values and errors to an outer driver's values and maps results back to typed application events. |
| Event ingress | Delivers every external result through the serialized application input boundary; a worker, watcher, or dialog never mutates workspace state directly. |

The presenter may keep rendering caches that can be reconstructed from application output. Cursor, selection, active pane, pending conflict decisions, and other behaviorally significant state remain in the application ring.

### 3.4 Frameworks and drivers — replaceable mechanics

This outer ring owns concrete technology and resource lifetime. It includes the C++ Qt Widgets view, CXX-Qt bridge objects, Qt item-model machinery, macOS file-system and desktop-service calls, FSEvents (or a replacement), settings serialization, worker implementation, and native error conversion.

| Driver | Responsibility |
|---|---|
| Qt delivery | Constructs widgets, owns Qt object lifetime, connects signals to the input controller, and renders presenter output. CXX-Qt types stay here because they name a framework. |
| File-system gateway | Implements directory reading and file-operation execution using macOS facilities. It enforces syscall-level protections such as no-overwrite behavior and preserves sufficient native detail to classify errors. |
| Watcher gateway | Watches a requested location and reports invalidation; it never refreshes a pane or changes state itself. |
| Settings gateway | Stores application-defined settings values at an application-support location using an adapter-chosen format and atomic replacement writes. |
| Worker/runtime driver | Runs blocking port work, owns cancellation primitives and native handles, queues typed results, and requests an event-loop wake-up. |

Infrastructure code may contain FFI and `unsafe`, but it is isolated here. Each unsafe boundary is minimal and documented with a `SAFETY:` explanation. Native types, file descriptors, Qt objects, and platform error objects never escape this ring.

### 3.5 Composition root — wiring only

The executable entry point creates concrete drivers, binds them to application-owned ports, creates controllers and presenters, and starts the Qt event loop. It is allowed to know every concrete type. It must not introduce business rules, recover errors, or decide operation outcomes.

## 4. Data and control flow

### 4.1 User command and rendering

```
Qt signal / drag-and-drop / menu action
  → input controller
  → application input boundary
  → workspace use case and state transition
  → framework-neutral output model
  → presenter
  → Qt model or dialog view
```

Menus, toolbars, drag-and-drop, tests, and future shortcuts all use the same command boundary. The interface adapter may report invalid input, but it does not replace application validation with widget-specific rules.

### 4.2 Blocking work and external events

```
application work request
  → application-owned output port
  → outer gateway and worker runtime
  → macOS / storage / watcher
  → typed application event
  → serialized application input boundary
  → new output model
```

The runtime, not the application use case, chooses threads, queues, cancellation handles, and wake-up mechanics. The application requires only that inputs are processed in order by one owner. The Qt delivery layer binds that owner to the GUI thread so that output reaches models there, while all blocking work stays outside it.

Every asynchronous request has a request/version token. The application records the token when issuing work and ignores a result that no longer describes the current tab, location, or operation generation. A blocked network or provider-backed location may delay a worker but cannot overwrite newer state or freeze the UI.

### 4.3 File operations and decisions

File operations cross two boundaries: policy decides whether the operation may proceed and which decision is needed; the outer executor performs the actual file-system work safely.

1. An application command creates a validated operation intent and a new operation ID.
2. The application issues execution work and records its pending/running state.
3. The executor reports typed progress, conflict, failure, completion, or cancellation. It never chooses a conflict resolution on its own.
4. A conflict or destructive-operation decision becomes application output. The presenter displays the permitted choices and returns the selected choice as an application command.
5. The application validates that the choice still belongs to the pending operation, resumes or finishes it through the executor, then requests affected listings to refresh.

Product-level delete behavior and confirmation policy are defined in [mvp.md](mvp.md); no driver may infer them from a menu label or platform default.

## 5. Ownership, concurrency, and safety

| Resource | Owner | Rule |
|---|---|---|
| Widgets, `QObject`s, and Qt item models | Qt delivery driver on the GUI thread | A worker never reads or writes them. |
| Workspace state | Serialized application input boundary | Changes only while processing an application command or event. |
| Work request lifecycle | Application use case | Identified by request/operation tokens; only matching events affect state. |
| Worker handles, queues, file descriptors, native buffers | Outer runtime and gateway drivers | Never escape as domain or application types. |
| Rendering caches | Presenter/Qt driver | Rebuildable from application output and never authoritative for policy. |

### 5.1 Threading and responsiveness

The desktop runtime uses a small, bounded set of execution contexts. This is a threading policy, not a commitment to a particular executor or synchronization crate:

| Execution context | Owns and may do | Must not do |
|---|---|---|
| GUI/application thread | Qt event loop; widgets and Qt models; the serialized application input boundary; bounded state transitions; output delivery and rendering. | Synchronous file, provider, watch, persistence, process, or other unbounded work; waiting for a worker; holding a lock while invoking Qt or a port. |
| Worker runtime | Blocking directory reads, file-operation steps, settings I/O, native calls, and CPU work that could delay an event-loop turn. It emits typed events only. | Read or write Qt objects, presenter state, or workspace state; decide product policy; synchronously wait for GUI processing. |
| Native callback sources | Minimal watcher or platform callbacks that enqueue an invalidation or typed result. | Refresh a listing, perform I/O, update a model, or call application code re-entrantly. |

The worker runtime is bounded and demand-driven: it uses a configured finite capacity or equivalent controlled execution resource, never an unbounded thread-per-request design. Capacity and scheduling may evolve with measurement, but foreground interaction always has a free GUI event-loop turn; pending work waits in a cancellable queue rather than creating more threads. The runtime may run independent reads or operations concurrently when their resources and safety constraints permit it. It must serialize steps that depend on one another and must prevent conflicting writes to the same operation destination from racing.

Every GUI-thread action is short and non-waiting. Application transitions process one input at a time and return to Qt promptly; a large batch of worker results is drained in bounded slices over successive event-loop turns. Directory-listing results are published as complete, token-identified snapshots or as bounded batches with a final completion event. Progress updates are rate-limited or coalesced, and repeated watcher invalidations for the same location are coalesced into one refresh request. These rules prevent a fast worker, a large directory, or an event storm from starving input, painting, dialogs, or accessibility processing.

Cancellation is cooperative and observable. Cancelling a queued request removes it before execution when possible; cancelling running work signals the worker without blocking the GUI thread. Workers check cancellation at safe boundaries, report a typed terminal event, and release native resources before their handle is discarded. A worker that is slow, blocked, or cannot be interrupted may finish later, but its token and cancellation state ensure that it cannot change newer workspace state.

Locks, if the chosen runtime needs them, protect only driver-local mutable resources and have a narrow lifetime. Workspace state is not shared behind a mutex: its serialized input owner is its synchronization mechanism. Qt thread affinity and application event ordering therefore remain visible architectural rules rather than accidental properties of locking.

The architecture requires these data-safety invariants:

- A conflict is established by the write primitive or an equivalent atomic reservation, never only by a check-then-write race.
- A replacement must not destroy an existing destination until the replacement data is complete and the final switch is safe for the relevant object and volume.
- A cross-volume move completes each destination copy successfully before removal of its corresponding source.
- Recursive work does not unexpectedly follow symlinks and remains inside its intended source and destination trees.
- Cancellation leaves completed work intact and removes only known partial artifacts created by the cancelled operation.
- Errors retain operation and location context. Drivers classify native failures; application chooses recovery choices; presentation chooses wording and accessibility treatment.

## 6. Error, settings, and observability boundaries

Application errors distinguish conditions that affect available choices: item missing, destination exists, permission denied, privacy restriction, no space, read-only location, cross-device behavior, cancellation, and unknown failure. The macOS gateway maps native errors into those categories and may retain driver-only diagnostics for logging.

Settings and session values are application-owned value types. Their file format, storage location, atomic-write mechanism, and corrupt-data handling belong to the settings driver. A failed or corrupt load becomes a recoverable application event; it never makes a serialization format part of domain policy.

Logging is an outer concern. Structured operation context may be passed outward through an application-owned diagnostic port or emitted by a driver, but domain and application decisions must not depend on a logging backend succeeding.

## 7. Verification strategy

| Boundary | Primary tests | Evidence |
|---|---|---|
| Domain | Unit and property tests | Invariants and policy choices hold without I/O. |
| Application | Unit tests with deterministic output-port fakes | Commands/events yield the correct state, outputs, work requests, token rejection, and decision lifecycle. |
| Adapters | Focused controller/presenter tests | Native input maps to the right application input; output is rendered without duplicating policy. |
| Drivers | Temporary-directory and Qt smoke tests | Error mapping, exclusive writes, metadata behavior, symlink safety, settings atomicity, model notifications, GUI-thread confinement, cancellation, and bounded event delivery. |
| End-to-end | Small opt-in macOS scenarios | Volume, privacy, File Provider, watcher, and desktop-service behavior that fakes cannot reproduce. |

Tests that touch a file system stay in a temporary directory. No test uses a real user path, and the application/domain test suites require neither Qt nor macOS facilities.

## 8. Deliberate non-decisions

This architecture does not decide exact macOS API choices, concurrency primitive, worker-capacity value, storage format, sort collation rules, performance thresholds, or distribution tooling. Product choices and remaining key-binding work are defined in [mvp.md](mvp.md).

Those choices may vary, but each must preserve the dependency rule, ownership model, and data-safety invariants in this document.
