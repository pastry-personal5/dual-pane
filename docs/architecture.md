# Architecture

Status: Active

This document defines Dual Pane's architectural styles (Clean Architecture with MVVM presentation and command-based input), technical stack, crate boundaries, and safety constraints. [planned-repository-architecture.md](planned-repository-architecture.md) owns the physical repository layout, [product-behavior.md](product-behavior.md) owns product scope, and [ux-gui.md](ux-gui.md) owns current user-visible behavior.

## 1. Goals and constraints

The architecture prioritizes the following, in order:

1. **Data safety.** An operation must never silently overwrite or destroy user data. A conflict or destructive operation reaches an explicit policy and user-decision boundary.
2. **Responsiveness.** A person can continue navigating, selecting, changing tabs, and cancelling work while listings, watches, persistence, or file operations are in progress. The GUI thread never enumerates directories, watches locations, copies data, or performs other potentially blocking I/O.
3. **Correctness.** The workspace, pending decisions, and jobs have a single logical owner and change only through ordered application inputs.
4. **Testability.** Domain rules and use cases run without Qt, macOS, a real file system, or a scheduler.
5. **Replaceable details.** Qt, CXX-Qt, macOS APIs, storage formats, and concurrency mechanisms are outer details, not sources of application policy.
6. **Simplicity.** Boundaries exist to protect a dependency or test seam. The design does not require a crate, DTO, trait, or presenter for every noun.

The fixed technical constraints are Rust 2024; Qt 6.11.2+ Widgets; CXX-Qt (`cxx-qt`, `cxx-qt-lib`, and `cxx-qt-build`); Cargo-only builds; C++17 or newer; and macOS 26.7+ only. Qt is linked dynamically under LGPLv3. QML/Qt Quick, CMake, Corrosion, qmake project files, Windows or Linux support, a GUI-dependent core, and blocking work on the GUI thread are outside this design.

Dual Pane has one workspace window; Notices and operation UI may occupy auxiliary windows, but there is no second workspace. It is built and run locally without notarization and without the App Sandbox, so file access is governed by ordinary permissions and macOS privacy (TCC) grants. Those grants are tied to the code signature and can be lost when a locally signed build changes, so a privacy denial is a routine, recoverable error rather than an edge case.

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

### Architectural styles

Dual Pane combines three architectural styles. Clean Architecture sets the direction of source-code dependencies and is the outer frame. Inside it, presentation follows **Model–View–ViewModel (MVVM)** with one-way binding, and input follows a **command-based** style built on typed command messages. Neither style may break the dependency rule. Each MVVM and command role lives in the ring that owns its policy or technology:

| Role | Style | Ring and code | Responsibility |
|---|---|---|---|
| Model | MVVM | Application `Workspace` and the domain values and policies it uses | The single source of truth for state and rules. It changes only by handling an input. |
| ViewModel | MVVM | Interface-adapter presenters, such as `BrowserPresenter` producing `BrowserViewModel` | A Qt-free projection of application output that holds display text, formatting, and row-change instructions. It holds no authoritative state and can be rebuilt from application output. |
| Command surface | MVVM and command-based | Interface-adapter `InputController` | Turns a `UiEvent` and the current ViewModel into an application `Command`. This is how a gesture requests a change. |
| View | MVVM | C++ Qt Widgets, with CXX-Qt objects such as `FolderItemsListModel` as binders | Renders ViewModel state and reports gestures as `UiEvent` values. It makes no file-manager decision. |
| Command | Command-based | Application `Command` | A typed, UI-neutral request. It names its target by the stable identity observed when the gesture happened, such as Browser, tab ID, or exact entry name and row. |
| Event | Command-based | Application `Event` | An externally observed fact, such as a work result or a location invalidation. It carries a request token wherever its relevance can expire. |
| Handler | Command-based | Application `Workspace::handle` | The one handler for every command and event. It validates the input, changes state, and returns outputs and work requests. |
| Outbound command | Command-based | Application `WorkRequest`, carried out by drivers | A request for outside work. Its result returns as an `Event`, never through a callback. |

Qt calls its item-view adapters "models". A Qt item model is a View-side binder that exposes ViewModel rows to Qt's item views. It is not the MVVM Model and holds no policy.

#### MVVM rules

- Binding is one-way. ViewModel state reaches the View through Qt property change notifications and item-model notifications (row insertion, removal, `dataChanged`, or reset). The View never writes ViewModel or application state; it requests change only by sending a `UiEvent` that becomes a `Command`. There is no two-way binding.
- After each handled input, the GUI owner gives the outputs to every presenter. Each binder then applies only the difference from what its View shows. A programmatic update of Qt selection, current index, or scroll position never emits a new command.
- View-local state stays in the View: focus, hover, drag feedback, the live scroll offset, and measurements such as the number of fully visible rows. When such state affects behavior, the View reports it as a command, such as `Command::UpdateScrollHint`, or as a `UiEvent` parameter, such as a page size. The decision stays outside the View.
- A ViewModel formats and words. It does not decide validity or availability; enablement, such as whether Back is available, comes from application state and reaches the View through the ViewModel.
- ViewModels and the command surface are tested without Qt in the adapters crate. View tests cover only binding and native behavior.

#### Command rules

- Every way to invoke an action produces the same `Command`: pointer, keyboard, menu, toolbar, drag-and-drop, or test. A shortcut-capable action has a stable `ActionId` whose effective binding comes from settings ([P3-M2 actions](phase-3/milestone-02-architecture.md#actions-and-shortcut-settings)). P3-M6 [wires the delivered actions](phase-3/milestone-06-architecture.md#widgets-focus-and-visible-state) to that catalogue.
- Commands are data, not objects with behavior. They carry no callback, perform no I/O, and are interpreted only by `Workspace::handle`.
- The serialized owner handles inputs one at a time and in order. An input that arrives during handling, such as a terminal event returned directly by work dispatch, waits in a queue and is never handled re-entrantly.
- The handler validates every command against current state and rejects one that no longer applies without changing state, such as a stale tab, a row whose entry changed, or an unavailable action. A disabled control is a convenience, not the validation.
- Commands do not have to be reversible. Undo is outside product scope, and adding it requires a product decision.
- Because inputs are plain values, tests drive the application by replaying command and event sequences ([§7](#7-verification-strategy)).

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
| Value types | Browser side, tab ID, operation ID, request/version token, location, entry name, file kind, sort specification, and capability values. Their representations are platform-neutral. |
| Workspace structure | `BrowserTabs` keeps each Browser's ordered tab IDs nonempty, with exactly one active tab. `TabHistory` holds a tab's ordered `Visit`s; its current position always names a visit, and a new visit after Back discards the Forward visits. Each visit's `VisitState` holds the `Selection`, cursor, range anchor, and `ScrollAnchor`. It applies selection gestures and reconciles after a reload against the current Folder Items, so cursor, anchor, scroll, and selection refer only to entries that are present. The application keeps request tokens, pending reads, and the rule that a pending Back or Forward counts as taken. |
| Policies | The one-level Favorites hierarchy and its naming rules; operation intent validity; conflict, kind-mismatch, and destructive-operation decision requirements; directory-merge semantics; safe-operation semantics; stale-result identity rules; sort ordering; and other validation that does not require I/O. |
| Errors | Stable, user-relevant categories with operation and path context, never native error objects. |

`Location` and entry identity are platform-neutral domain values, not leaked native paths or file handles. A platform adapter may supply an opaque stable identity when one is available; the domain must not assume that an identity survives a rename, a volume boundary, or every provider-backed location.

### 3.2 Application — use cases and application state

The application ring coordinates the product's use cases. It owns the single `Workspace` aggregate, built from the domain's Browser and tab types plus the asynchronous state the domain does not model. The state is an implementation detail of the application's input boundary, not a UI model and not a Qt object.

| Area | Responsibility |
|---|---|
| Input boundary | Commands and externally observed facts: navigate, select, tab operations, file-operation requests, decisions, cancellation, Folder Items results, watcher invalidations, operation step results, and settings load/save results. |
| Workspace use cases | Validate inputs, evolve the workspace state, issue work requests, reject stale results, and produce application output. |
| Workspace state | Two Browsers of domain tabs; outstanding request tokens and loading state; file operations with their plans, progress, and pending decisions; and preferences required to carry out use cases. It has one serialized owner. |
| Output models | Immutable, framework-neutral values describing Browsers, jobs, decisions, and recoverable errors. Folder Items changes are row deltas (insert, remove, update) against the previous Folder Items so that views keep scroll position, cursor, and selection; a listing with no applicable delta is marked as replacing every row. They state *what changed*, not how a widget redraws it. |
| Work requests | Typed, purpose-specific descriptions of outside work the use cases need: read a directory, execute an operation step, start or stop watching a location, load or save settings, and open a file with its default application. |

Use cases are a pure reducer. The application never calls outward, holds a callback, or waits:

```
Workspace::handle(input)
  → validate and transition workspace state
  → (application outputs, zero or more work requests)
```

A runtime in the frameworks ring carries out each work request and later submits its typed result as a new input. The application owns the request and result types; there are no application-owned I/O traits to implement or fake. Work requests are purpose-specific; there is no universal `FileSystem` façade whose broad API encourages business logic to drift outward. For example, a directory-read request carries the location, sort specification, and the previous Folder Items snapshot needed to compute row deltas, and an operation-step request carries an already validated intent and the decisions that apply to it.

`handle` must not return a Qt `ViewChange`, manipulate a model, choose dialog text, or start a thread. A multi-step flow is modelled as explicit state identified by tokens, not as a suspended call.

### 3.3 Interface adapters — translate, do not decide

Interface adapters translate between the user interface and the application boundary. They are plain Rust with no Qt or CXX-Qt dependency, so they are tested without the Qt/C++ build. They reshape data for a screen, but they do not contain a second set of file-manager policy.

| Adapter | Responsibility |
|---|---|
| Input controller | The ViewModel's command surface. It maps Qt-free UI events, such as `UiEvent::DropOnBrowser { browser, row }` or a menu command identifier, plus the current view context into application commands. It does not decide whether a command is valid. |
| Presenter | Maps application output into the plain-Rust MVVM ViewModels: row view-models with display text and icon kind, row-delta instructions, Operation Panel status, Operation Decision Card choices, and Notices. It owns formatting and wording, not policy or the set of available user actions. |

The Qt delivery driver is the MVVM View. It performs only the mechanical translation from Qt signals, actions, and model indexes to `UiEvent` values and from view-models to Qt model notifications and widgets, following the [MVVM and command rules](#architectural-styles).

The presenter may keep rendering caches that can be reconstructed from application output. Cursor, selection, active Browser, pending conflict decisions, and other behaviorally significant state remain in the domain and application rings.

### 3.4 Frameworks and drivers — replaceable mechanics

This outer ring owns concrete technology and resource lifetime. It includes the C++ Qt Widgets view, CXX-Qt bridge objects, Qt item-model machinery, macOS file-system and desktop-service calls, FSEvents (or a replacement), settings serialization, worker implementation, and native error conversion.

| Driver | Responsibility |
|---|---|
| Qt delivery | Constructs the workspace window and auxiliary operation and Notices windows, owns Qt object lifetime, translates Qt signals into `UiEvent` values for the input controller, and applies presenter view-models to Qt models and widgets. CXX-Qt types stay here because they name a framework. |
| Runtime | Receives work requests from the reducer, dispatches them to gateways on the worker pool, owns cancellation primitives and native handles, and delivers every typed result as an application input on the GUI thread. A worker, watcher, or dialog never mutates workspace state directly. |
| File-system gateway | Reads directories, then sorts and computes row deltas against the previous Folder Items snapshot on a worker. Scans operation sources and executes operation steps using macOS facilities. Probes mounted-volume capabilities outside the GUI thread, enforces only the syscall-level protections the mounted file system actually supports, and classifies native errors into application error categories. |
| Watcher gateway | Watches a requested location and reports invalidation; it never refreshes a Browser or changes state itself. |
| Settings gateway | Stores application-defined settings, Favorites, and session values at an application-support location using driver-owned encoding and transactional durable writes. It runs on its own serialized worker; the desktop session routes settings work requests there rather than through the listing runtime. |

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
  → Qt delivery driver: Qt model notifications, Operation Panels, Notices, or confirmation UI
```

This loop is the command-based input and one-way MVVM binding described in [Architectural styles](#architectural-styles). The interface adapter may report invalid input, but it does not replace application validation with widget-specific rules.

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
5. A required conflict or recoverable-error decision becomes application output for that job's Operation Decision Card. Several jobs may wait for separate choices concurrently. The presenter displays the permitted choices and returns the selected choice as an application command.
6. The application validates that the choice still belongs to the pending operation and item, records it (including any "apply to all" choice, scoped to that operation), and issues the next step from that item. Cancelling simply issues no further step and requests cleanup of the operation's known partial artifacts.
7. When the operation finishes, the application requests affected listings to refresh.

Product-level delete behavior, conflict choices, and confirmation policy are defined in [product-behavior.md](product-behavior.md); no driver may infer them from a menu label or platform default.

### 4.4 Data-safety invariants

The architecture requires these data-safety invariants:

- A conflict is established by the write primitive or an equivalent atomic reservation, never only by a check-then-write race. Name equality is therefore whatever the destination file system decides (including APFS case and Unicode rules); the application never pre-compares names to decide safety.
- A replacement must not destroy an existing destination until the replacement data is complete and the final switch is safe for the relevant object and volume.
- Replacement applies only to a file replacing a file. A directory arriving at an existing directory is merged; a file/folder kind mismatch is never resolved by replacing either side.
- A cross-volume move completes each destination copy successfully before removal of its corresponding source.
- Recursive work never follows symlinks: a symlink is copied or moved as a link, and work remains inside its intended source and destination trees. Navigating into a symlinked directory is ordinary navigation, not recursive work.
- Cancellation leaves completed work intact and removes only known partial artifacts created by the cancelled operation.
- Errors retain operation and location context. Drivers classify native failures; application chooses recovery choices; presentation chooses wording and accessibility treatment.

## 5. Ownership, concurrency, and safety

| Resource | Owner | Rule |
|---|---|---|
| Widgets, GUI-facing `QObject`s, and Qt item models | Qt delivery driver on the GUI thread | They are created, used, and destroyed on their affinity thread. A worker never reads or writes them. |
| Workspace state | Serialized application input boundary | Changes only while processing an application command or event. |
| Work request lifecycle | Application use case | Identified by request/operation tokens; only matching events affect state. |
| Operation plan, progress, and pending decisions | Application use case | Held in workspace state between steps; no worker holds a pending decision. |
| Worker handles, queues, file descriptors, native buffers | Outer runtime and gateway drivers | Never escape as domain or application types. |
| Rendering caches | Presenter/Qt driver | Rebuildable from application output and never authoritative for policy. |

### 5.1 Threading and responsiveness

The desktop runtime uses a small, bounded set of execution contexts. These are architectural roles, not a commitment to a particular executor, synchronization crate, or one operating-system thread per row:

| Execution context | Owns and may do | Must not do |
|---|---|---|
| GUI/application thread | Qt event loop; widgets and Qt models; the serialized application input boundary; bounded state transitions; output delivery and rendering. | Synchronous file, provider, watch, persistence, process, or other unbounded work; waiting for a worker; blocking queue submission; a blocking queued connection; nested event-loop pumping; or holding a lock while invoking Qt or application code. |
| Runtime coordinator | Non-blocking work admission; bounded queues; prioritization and fairness; request cancellation state; worker supervision; and terminal-result accounting. | Perform file I/O or expensive computation; call application or Qt code; wait for a user decision; or make product-policy decisions. |
| Worker execution | Blocking directory reads; Folder Items sort and delta computation; operation scans and steps; settings I/O; native calls; and other work that could delay an event-loop turn. It emits typed events only. | Read or write Qt objects, presenter state, or workspace state; decide product policy; synchronously wait for GUI processing; or hold a destination lease while waiting for a person. |
| Native callback sources | Minimal watcher or platform callbacks that enqueue an invalidation or typed result. | Refresh Folder Items, perform I/O, update a model, or call application code re-entrantly. |

Nothing may simulate responsiveness by calling `QCoreApplication::processEvents`, entering a nested event loop, or using a synchronous cross-thread callback. Those techniques permit re-entrancy and make state ordering implicit. Operation Decision Cards and other user decisions are asynchronous: opening the UI returns to Qt, and the eventual response arrives as a new application command. Cross-thread Qt delivery always uses a queued invocation; a direct or blocking queued connection is not used across thread boundaries.

Every GUI-thread action is short and non-waiting. Application transitions process one input at a time and return to Qt promptly. Worker results are drained in slices bounded by both item count and elapsed time, over successive event-loop turns; a count limit alone is insufficient because individual results may have very different costs. The GUI thread applies only already-computed model changes. It never enumerates, sorts, diffs, formats complete large Folder Items, resolves icons through blocking I/O, or eagerly converts every row to a Qt value. A full replacement swaps immutable backing data and issues the required Qt notification in bounded work; incremental results use bounded row-delta batches. All `QAbstractItemModel` API and begin/end notification pairs remain on the model's GUI thread.

The event-loop budget covers controller mapping, one `Workspace::handle` transition, presentation, model notification, and scheduling the next drain. Progress updates are rate-limited or coalesced, repeated watcher invalidations for the same location become one refresh request, and paint is requested with Qt's coalescing update path rather than forced synchronously. Exact slice sizes and time budgets are measured and tuned later, but an event storm, large directory, or fast worker must always leave turns for input, painting, window management, dialogs, and accessibility.

Locks, if the chosen runtime needs them, protect only driver-local mutable resources and have a narrow lifetime. Workspace state is not shared behind a mutex: its serialized input owner is its synchronization mechanism. A GUI-thread-only holder that lets several Qt objects reach that owner is not shared state; it is released before Qt is notified. Qt thread affinity and application event ordering therefore remain visible architectural rules rather than accidental properties of locking.

### 5.2 Planned desktop execution model

**Status: Planned.** This is the broader target beyond the current runtime, which serves each Browser's bounded directory-read admission queue with a fixed set of supervised reader lanes ([2026-10-02 changelog](phase-3/changelog.md#2026-10-02--pre-m6-review-fixes)). A cancelled queued read never runs, and a blocked read occupies only its own lane, so the Browser's other tabs keep loading while a lane is free; further reads stay queued and cancellable. The lanes also sort each listing and diff a same-folder reload against the request's previous Folder Items. A separate serialized lane runs short location probes. It refines the required behavior above without selecting an executor, synchronization crate, pool-size value, or platform cancellation API.

The runtime has a GUI-thread endpoint, a coordinator, finite worker capacity, and a GUI-bound result endpoint. The GUI endpoint is the only component that may call the controller, `Workspace::handle`, presenter, CXX-Qt bridge, Qt models, or widgets. Dispatch is an O(1), non-blocking handoff. Workers own only a job's copied request data, cancellation handle, gateway-local resources, and result publisher; they have no reference to workspace or presenter state, a GUI `QObject`, or a callable that can re-enter the GUI thread.

#### Workload isolation and scheduling

The runtime exposes logical execution lanes. An implementation may use separate pools or a shared physical pool only if it preserves the isolation, capacity, and scheduling guarantees in this table:

| Lane | Work | Scheduling rule |
|---|---|---|
| Foreground blocking I/O | Directory enumeration, metadata needed for visible rows, navigation, and other latency-sensitive reads. | Superseded Folder Items requests are cancelled before admission when possible. It has reserved service capacity so bulk operations and CPU work cannot occupy every slot. Provider and remote-volume calls have separate bounded allowances, so one blocked mount cannot consume all foreground capacity. |
| File-operation I/O | Copy, move, rename, create, delete, source scans, and cancellation cleanup. | Fair progress with bounded parallelism. Dependent steps and writes with overlapping destination scopes are serialized by runtime-owned destination leases. A step releases its worker and lease before asking for a decision. |
| CPU transformation | Sort, diff, checksum, and other computation over data already read. | Bounded independently from blocking I/O so CPU saturation cannot prevent a read or cancellation cleanup from starting. Long computations split into cancellable units. |
| Serialized services | Settings persistence and native facilities that require one owner or a particular callback context. | Each service has one ordered owner and coalesces replaceable work such as session snapshots. It receives a dedicated thread only when the native API requires one. |

Every lane and queue has a configured finite bound; there is no unbounded thread-per-request fallback. Capacity is based on measurement, workload type, and memory limits rather than equating every task with a CPU core. Fair scheduling prevents sustained navigation from starving file-operation cleanup and prevents background refreshes from delaying foreground navigation. Slow or uninterruptible I/O is never compensated for by unlimited replacement threads. If a lane has exhausted its bounded capacity, further work remains cancellable in its queue or admission fails visibly; the GUI still does not block.

#### Admission, backpressure, and delivery

The GUI thread never waits for queue space. At admission the coordinator either accepts the request immediately or returns a typed retryable saturation result. Each accepted job reserves the capacity needed for its one terminal event, so a success, failure, or cancellation is never dropped merely because the result queue is full. Replaceable events use keyed coalescing: only the newest pending Folder Items refresh per tab/version, watcher invalidation per location, progress snapshot per operation, and settings snapshot need remain queued. Product commands and terminal events are not coalesced.

Large immutable payloads cross threads by shared ownership rather than repeated copying. Publishing a result sets one coalesced wake flag and posts at most one queued GUI wake while a drain is pending. The GUI endpoint consumes terminal/control events before replaceable progress, drains within its count-and-time budget, clears or rearms the wake without a lost-wakeup race, returns to Qt, and schedules another queued turn only if work remains. Closing the endpoint makes later publications harmless no-ops; a failed CXX-Qt queue operation after object destruction is teardown, not an application error.

Request/version tokens remain the authority for relevance. Queue removal and worker cancellation reduce wasted work, but the application independently rejects every stale result before mutation. Runtime priority never changes application event ordering or lets a later step of one operation overtake an earlier dependent step.

#### Cancellation and shutdown

Cancellation is a runtime-owned, idempotent state machine associated with an application token. The application requests cancellation; it does not manipulate thread handles or native resources.

1. A per-job terminal claim makes success, failure, and cancellation mutually exclusive. Before a job starts, cancellation removes or marks it in the queue and publishes exactly one typed terminal cancellation, unless the delivery endpoint has already closed during teardown.
2. For running work, cancellation sets a cheap shared flag and invokes a native cancellation mechanism when one is available. The gateway checks before a side effect, between bounded units, and after an interruptible blocking call returns. Cancellation submission itself never waits for the worker or a mutex held by it.
3. A worker that loses the terminal claim discards its late outcome. Application token validation remains a separate defense, so even a broken or uninterruptible provider cannot overwrite newer state.
4. A file operation finishes its current atomic safe boundary. Completed items remain completed. The executor removes only partial artifacts it can positively identify as its own. If cleanup cannot establish a safe result, it reports a typed failure with operation and artifact context rather than claiming cancellation completed cleanly.
5. During window teardown, the GUI owner first closes delivery, then rejects new work and requests cancellation for queued and running jobs. It does not call `waitForDone`, join a worker, or otherwise wait inside the Qt event loop. Workers hold no UI references, so late completion cannot touch a destroyed object. After the event loop exits, the composition root may perform only a bounded graceful join; an uninterruptible worker is detached for process teardown rather than forcing termination or delaying exit without bound.

Cancellation is therefore cooperative, not a promise that arbitrary I/O stops immediately. A token is invalidated as soon as cancellation is accepted, so late work cannot change newer Folder Items, an operation, or a view even when the operating system cannot interrupt it.

#### Error and panic containment

Expected gateway failures are data, not worker failures. A gateway maps native failures at the edge into the application-owned error category plus operation, location, and item context; it keeps raw OS errors and diagnostics driver-local for logging. The worker remains available after permission, privacy, missing-item, no-space, busy, read-only, and other routine outcomes. The application decides whether the typed error exposes a retry, a conflict decision, or a terminal operation state; presentation supplies the wording.

Each worker executes one job behind a Rust panic boundary. A panic is never allowed to cross a CXX-Qt, Qt, Objective-C, C++, or other FFI boundary, and its payload is never presented to the person. The runtime wins or observes the job's terminal claim, publishes one `Internal` failure for an affected non-cancelled token, discards that worker and its gateway-local state, and replaces capacity under bounded backoff. It does not continue using possibly poisoned worker-local state.

The replacement never replays the panicked job automatically: a file-operation step may already have changed the file system. Queued independent jobs remain queued and cancellable while capacity is restored. Repeated worker loss opens a circuit for that lane: new admissions receive a typed unavailable result while periodic bounded recovery attempts continue. A panic, failed spawn, closed coordinator, or runtime-initialization failure is logged with sanitized diagnostics and becomes a typed runtime failure; it must not terminate the GUI process or silently leave an accepted request loading forever.

This boundary does not make process-abort panics, memory corruption, or a crashed native library recoverable. Those failures are outside Rust panic recovery. The runtime minimizes their blast radius by keeping native calls and `unsafe` blocks narrow, avoiding poisoned shared application state, and ensuring that no worker owns a Qt object or authoritative workspace data.

#### Responsiveness evidence and observability

The runtime records queue depth, admission rejection and coalescing counts, queue wait, execution time, cancellation latency, result-drain time, and worker replacement without making application behavior depend on logging. A GUI heartbeat may measure event-loop delay, but it only reports; it never mutates Qt from a monitor thread or attempts recovery by pumping events.

The planned verification evidence includes: GUI-affinity assertions for every model and widget mutation; tests that dispatch and cancellation return without waiting when queues or workers are blocked; bounded-queue, fair-scheduling, reserved-capacity, and coalesced-wake stress tests; count-and-time-bounded drain tests; exactly-once terminal-event tests for queued, running, stale, saturated, panicking, and teardown paths; tests that a slow cancelled job cannot publish late state; native-error mapping tests; panic containment tests proving no payload reaches output, no unsafe replay occurs, and capacity or the lane circuit is recovered; and shutdown tests proving late publications cannot use a destroyed delivery endpoint or delay the GUI event loop.

### 5.3 Planned mounted remote-volume model

**Status: Planned.** This section plans safe future handling of a location already mounted by macOS, including an SMB share presented in the local file-system namespace. It does not add remote file systems to the current product scope in [product-behavior.md](product-behavior.md#out-of-scope), implement an SMB client, mount shares, prompt for network credentials, or store them.

A mounted remote volume looks path-like but is not local-storage-equivalent. Any directory enumeration, metadata lookup, path resolution, open, close, flush, rename, deletion, free-space query, or watch setup may perform network I/O, pause during reconnect, or return after the server has changed independently. No such call, including a preliminary `stat`, capability query, mount check, or icon lookup, runs on the GUI thread.

#### Mount identity and capabilities

The macOS gateway builds a `MountedVolumeProfile` on a worker when a location first crosses a mount boundary. The profile is driver-owned and contains an opaque mount-instance identity, mount generation, connectivity generation, local/remote and read-only state, file-system type for diagnostics, and the capabilities relevant to the requested operation: case sensitivity and preservation, persistent IDs, exclusive rename, ordinary rename, Trash behavior, cloning, permissions, extended attributes, maximum file size, and watcher availability. Capability discovery itself can block or fail, so an unknown value is not treated as supported.

The gateway chooses behavior by observed capability, not by assuming that every `smbfs` mount or every server behaves alike. It may use protocol or file-system type to select conservative scheduling and diagnostics, but never to manufacture a guarantee. A profile is cached only for the lifetime of its observed mount and connectivity generations. Unmount, remount, mount-path rename, wake from sleep, transport loss, reconnect, or an error that indicates a stale mount advances one of those generations and requires revalidation. A path reused by a later mount is a new volume until reprobed, even if the server and share names look identical.

Entry identity on a remote volume is best-effort. The domain receives an opaque identity only when the mounted volume reports persistent IDs; otherwise path components plus the Folder Items generation identify the observed entry. Inode numbers, file IDs, timestamps, case-folded names, and mount paths are never assumed stable across reconnect, failover, server upgrade, or remount. After uncertainty, the application preserves selection or cursor only for entries it can match safely and treats the rest as new observations.

Mount triggers and nested mount points are boundaries, not ordinary directories for recursive work. Merely reading a parent must not activate a dormant mount trigger. Explicit navigation may ask macOS to enter an already supported mounted location, but recursive scans do not silently cross into a different mounted volume. Encountering such a boundary becomes a typed application fact; behavior beyond that boundary remains a future product decision.

#### Responsiveness, isolation, and backpressure

Remote calls use a global bounded remote-I/O allowance and a smaller per-mount allowance layered over the execution lanes in §5.2. Local navigation, cancellation cleanup, settings, and another remote mount retain service capacity when one share stalls. A blocked call keeps its existing allowance; the runtime never creates an unlimited replacement thread to hide a hung server. Requests waiting behind that mount remain cancellable without starting.

Deadlines are observations, not claims that a mounted-file-system syscall can be forcibly stopped. Crossing a soft deadline publishes a coalesced slow-location state and keeps the GUI interactive; cancellation immediately invalidates the application token and invokes native cancellation when available, but the worker may remain blocked. Repeated timeouts, disconnects, or transport failures open a per-mount circuit. While open, background refreshes are suppressed, new work fails fast with a typed unavailable state, and at most one bounded probe runs after backoff or an explicit retry. Success closes the circuit only after the mount identity and capabilities are revalidated.

Directory reads request only metadata required for the current view and operation, preferring one enumerator result over per-entry round trips. Expensive metadata, previews, and icons are lazy, separately cancellable, and prioritized for visible rows. Partial Folder Items batches are published only when their ordering and provisional status are explicit; otherwise the worker completes enumeration, sorting, and diffing off the GUI thread and delivers immutable backing data. Folder Items and metadata caches are memory-bounded, keyed by mount generation, and never presented as fresh after disconnect. The last successful Folder Items may remain visible as unavailable/stale so the other Browser stays useful, but actions revalidate the target and mount before execution.

Watch notifications on remote volumes are advisory. The watcher uses native notification support only when available and still treats each event as an invalidation requiring a worker rescan. Missing, dropped, or unsupported notifications fall back to visibility-aware polling with bounded exponential backoff and jitter; hidden tabs do not poll continuously. Reconnect, remount, watcher overflow, or a changed mount generation requires a full rescan. Watch activity from one share is coalesced and cannot flood the GUI result queue.

#### Safe operations and uncertain outcomes

Remote mutation uses the same [data-safety invariants](#44-data-safety-invariants) as local mutation, with additional distrust of cached metadata and acknowledgements. Immediately before each namespace or destructive step, the gateway revalidates the mount and connectivity generations, source, destination parent, relevant identities, and required capabilities. It uses the mounted file system's atomic no-overwrite or exclusive-create primitive rather than a check-then-write sequence. Server locks and advisory locks are not correctness boundaries because another client or reconnect may invalidate them. If exclusive publication, required durability, or macOS Trash semantics are unavailable or unknown, the affected replace, move, or Trash operation reports a typed unsupported-safety failure; Trash never degrades to permanent deletion and no operation silently substitutes a weaker algorithm. Free-space and quota values are hints only; the executor still handles failure from every write and close.

Copies write to an operation-owned, uniquely named partial artifact in the destination directory or volume. Data is streamed in cancellation-sized chunks, write and close/flush errors are checked, and the final destination switch occurs only after the completed artifact is verified to the degree supported by that volume. Replacement does not remove the old destination until that point. Cross-volume moves do not remove the source until the destination is complete and verified.

A timeout or disconnect during a mutating call creates an **uncertain outcome** unless postcondition evidence proves what happened. Rename, replace, delete, create-directory, final destination publication, and source removal are not retried automatically after an uncertain outcome. Before issuing such a call, the executor atomically persists a driver-local recovery record containing its operation ID, mount and connectivity generations, step, known source and destination observations, and owned partial-artifact name; it clears the record only after a confirmed terminal outcome. The record contains no credential or mount-from secret. After an uncertain return, the executor releases the worker and reports uncertainty to the application. After connectivity returns—or after a later launch—a separate read-only reconciliation step observes both sides and classifies the step as completed, not completed, or still ambiguous. Recovery never resumes a mutation automatically. Only the application may then offer a safe retry or decision. It never infers success solely from a missing source or matching name, and it never repeats source deletion merely because the first reply was lost.

Read-only operations may retry automatically with bounded exponential backoff and jitter while their token is current. A data-transfer step may resume only when the runtime can prove it is reopening the same operation-owned partial artifact at the verified offset; otherwise it restarts into a new owned artifact or asks for recovery. Automatic retry budgets are per request and per mount so a disconnected server cannot generate permanent traffic.

Unmount and remount notifications are delivered as application inputs. An unmount invalidates the mount generation, cancels queued work, marks affected tabs unavailable without discarding their last successful view, and moves any in-flight mutation to cancellation or uncertain-outcome reconciliation according to its last safe boundary. Remount never automatically resumes a mutating operation. Credentials and mount-from locations are never written to logs; diagnostics use a redacted mount identifier and operation token.

#### Remote-volume verification

Most tests use a deterministic fault-injecting gateway, not a real user share. It simulates delayed and permanently blocked calls, disconnect before and after a server commit, stale replies, remount at the same path with a new generation, missing persistent IDs, case-insensitive names, unavailable exclusive rename, watcher loss, queue saturation, and cancellation at every operation boundary. Properties prove that local work remains serviceable, worker counts remain bounded, stale results never commit, an uncertain mutation is never replayed automatically, and no source is removed before verified destination completion.

Opt-in macOS integration tests may use only a disposable, test-owned share backed by a temporary directory and provisioned for that test run. They never mutate an existing mounted share or a real user path. The milestone that introduces such a harness must separately approve its server or mounting tool, credentials handling, teardown, and licensing.

## 6. Error, settings, and observability boundaries

Application errors distinguish conditions that affect available choices: item missing, destination exists, file/folder kind mismatch, permission denied, privacy restriction, no space, item busy, read-only location, cross-device behavior, cancellation, and unknown failure. The macOS gateway maps native errors into those categories and may retain driver-only diagnostics for logging. The application decides which categories are recoverable and therefore offer the choices defined in [product-behavior.md](product-behavior.md#file-operation-safeguards).

Planned mounted remote-volume support also distinguishes slow, timed out, disconnected, authentication required, server unavailable, stale mount generation, unsupported safety capability, and uncertain mutation outcome. These are application-visible states rather than protocol error numbers. A driver may collapse native errors only when doing so preserves the recovery choices and never converts an uncertain mutation into an ordinary retryable failure.

Settings and session values are application-owned value types. The application requests a save of a coalesced session snapshot shortly after any session-relevant change and once more on quit, so a crash loses at most the most recent changes. The file format, storage location, atomic-write mechanism, and corrupt-data handling belong to the settings driver. A failed or corrupt load becomes a recoverable application event; it never makes a serialization format part of domain policy.

An operation recovery record is separate from session settings. The runtime persists it locally and atomically before an ambiguity-prone remote mutation, restricts it to the context needed for reconciliation, and never stores credentials. Loading a record may request read-only reconciliation but never executes or resumes a mutation by itself.

Logging is an outer concern. Structured operation context may be carried in application outputs and emitted by a driver, but domain and application decisions must not depend on a logging backend succeeding.

## 7. Verification strategy

| Boundary | Primary tests | Evidence |
|---|---|---|
| Domain | Unit and property tests | Tab order and active-tab, tab history, selection, cursor, range anchor, scroll, Favorites hierarchy, and sort invariants and policy choices hold without I/O. |
| Application | Unit and property tests of `Workspace::handle` | Commands/events yield the correct state, outputs, work requests, token rejection, row deltas, and operation/decision lifecycle, with no fakes. |
| Adapters | Focused controller/presenter tests without Qt | UI events map to the right application commands; outputs become view-models without duplicating policy. |
| Drivers | Temporary-directory, fault-injection, and Qt smoke tests | Error mapping, exclusive writes, metadata behavior, symlink safety, settings atomicity, model notifications, GUI-thread confinement, cancellation, bounded event delivery, mount-generation invalidation, and uncertain-outcome reconciliation. |
| End-to-end | Small opt-in macOS scenarios | Local and disposable mounted-volume capabilities, disconnects, privacy, File Provider, watcher, and desktop-service behavior that fakes cannot reproduce. |

Tests that touch a file system stay in a temporary directory. No test uses a real user path, and the domain, application, and adapter test suites require neither Qt nor macOS facilities.

Property tests generate input sequences and check that invariants hold, for example: the cursor and selection always refer to the current Folder Items; only the latest token changes a tab's Folder Items; applying emitted row deltas to the old Folder Items yields the new Folder Items; an operation terminates exactly once and rejects decisions that are not pending; and a restored session equals the saved one apart from unrestorable tabs. The domain and application crates use the approved `proptest` dev-dependency for them.

## 8. Deliberate non-decisions

This architecture does not decide exact macOS API choices, concurrency primitive, worker-capacity value, operation step size, storage format, sort collation rules, session-save delay, or performance thresholds. Distribution is limited to local, unnotarized, unsandboxed builds (see [§1](#1-goals-and-constraints)); packaging tooling is not decided. Product choices are defined in [product-behavior.md](product-behavior.md) and key bindings in [ux-gui.md](ux-gui.md).

Section 5.3 plans invariants for future support of locations already mounted by macOS; it does not change the current exclusion of remote file systems or decide mounting, discovery, authentication, credential storage, direct SMB access, or user-visible offline behavior. Those product and technology choices require a future phase decision.

Those choices may vary, but each must preserve the dependency rule, ownership model, and data-safety invariants in this document.
