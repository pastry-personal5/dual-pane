# P4-M7 architecture

Status: Active

Drag and drop is a new gesture boundary, not a new file-operation path. It supplies a validated, frozen transfer intent to the application-owned workflow already generalized by P4-M4. The [overview checklist](milestone-07-overview.md#completion-checklist) defines acceptance; P4-M1 fixes internal-only directions, target classes, and Move/Option-Copy, and the [M7 owner decisions](milestone-07-overview.md#owner-decisions) fix gesture, feedback, target, and acceptance behavior.

## Existing seams and constraints

The completed clipboard milestones already provide the operation representation M7 needs:

- `OperationRoot` carries one byte-exact absolute source and its no-follow `EntryKind`.
- `OperationIntent::transfer` accepts ordered roots with one destination and rejects empty, duplicate or overlapping roots, a same-folder transfer, and a destination within a directory root.
- The scanner rechecks kind and native identity before execution. The operation coordinator, two-worker lane, safety journal, conflict decisions, verified-before-removal rule, progress, cancellation, cleanup, Notices, and refresh remain authoritative.
- The application already captures visible selection from a Browser, tab, and visible revision for observed commands. M7 must reuse that reconciliation rather than exporting paths from Qt.

After P4-M11, the native gesture surfaces are `FolderItemsList` and `FolderPane` in `cpp/include/dual_pane_desktop/folder_pane.hpp`, plus `BrowserTabButton` and `BrowserTabsStrip` in `tabs_strip.hpp`. The CXX-Qt surface belongs in `folder_items_list_model.rs` with feature logic in a new `folder_items_list_model/drag_drop.rs` child. Application policy stays in Qt-free types under `dual-pane-application`; C++ owns only pointer mechanics, painting, and translation of a live widget target into typed identity.

Qt's platform drag API runs its own drag session. Treat callbacks during that session as reentrant: they may observe listing, tab, operation, or window changes. No raw widget pointer, model index, row number, label, or path captured before that session may be trusted at drop time without resolving it again through stable identity and revision.

## Application-owned drag session

Add a UI-neutral session identified by a fresh `DragId`. Exact names may adjust during implementation, but keep these contracts:

- `BeginDrag` names the observed source Browser, `TabId`, and `visible_revision`. The application captures the selected visible roots in listing order with the same ready-listing and stale-selection checks used by clipboard capture. It returns a prepared session with `DragId`, root count, and allowed Copy/Move actions, but never returns paths for serialization into native MIME data.
- `DropTarget` is typed as `BrowserFolder`, `FolderRow`, or `BrowserTab`. It carries stable Browser and `TabId`; row targets also carry the target tab's visible revision and a row identity that the application resolves to a real destination under the accepted M7 policy. A tab label and the currently active tab are never target identity.
- `PreviewDrop` is a pure, no-I/O query over the current workspace and drag session. It returns an eligible frozen destination plus action/count presentation, or a typed rejection. Repeated hover must not allocate an operation, change selection, activate a tab, navigate, or emit Notices.
- `AcceptDrop` repeats the preview validation, consumes the one-use drag session, and starts an asynchronous physical-destination preflight. Its completion revalidates the captured source and target, freezes the canonical destination, calls `OperationIntent::transfer`, performs the ordinary journal availability check, and starts the existing operation. Preview eligibility and a queued preflight are never authority for a later start.
- `CancelDrag` and invalidation remove the session. An accepted, cancelled, or ended `DragId` is one-use; a late preview or drop is rejected. Starting a replacement drag invalidates the previous one.

Keep drag rejection separate from `ClipboardRejection`, because clipboard generation, publication, and external preflight do not apply. Reuse `OperationRejection` where it precisely describes intent construction; add drag-specific reasons for stale session, unsupported source, unsupported target kind, stale source view, stale target view, external payload, and unsupported action. Presentation maps these typed reasons to the accepted Status Bar and accessibility wording.

The application owns the current session even though C++ drives the platform loop. This closes three unsafe alternatives: trusting `QMimeData` paths, reconstructing the source from current selection at drop, or letting a drop resolve “the active tab” after focus changed.

## Private payload and provenance

Use one versioned application-private MIME type containing only the format version and `DragId`. Do not publish `text/uri-list`, file URLs, display names, or byte paths. That ensures a drag leaving Dual Pane does not accidentally become the out-of-scope app-to-Finder transfer implemented by another process.

Incoming acceptance requires all of the following:

1. The event advertises the supported private version and decodes exactly one `DragId` with no trailing data.
2. `QDropEvent::source()` identifies a live `FolderItemsList` owned by this workspace, not merely a foreign payload using the same MIME name.
3. The application has the matching current drag session and the source widget corresponds to its Browser.
4. The typed target resolves and the repeated Qt-free validation accepts the requested action.

The native payload is therefore only correlation, never source authority. A copied, malformed, unknown, completed, or replayed token starts no operation. Ending the native drag session always sends idempotent cancellation; it is harmless after a successful accept.

Do not reuse the macOS clipboard private type or `TransferId`. Clipboard transfers have generation and relaunch semantics; a drag is a short-lived in-process gesture with widget provenance. Sharing the identifier would blur those lifetimes and make external payload fallback possible.

## Source capture and invalidation

The Folder Items List retains the press position and the platform drag threshold. It applies the accepted selection gesture once on press. When an eligible pointer movement crosses `QApplication::startDragDistance()`, it asks the application to begin a drag from the rendered `TabId` and `visible_revision`. A rejected begin never constructs `QDrag` and reports the typed reason under the accepted feedback policy.

The application captures roots, not row indexes. Each root is `shown.location.join(entry.name())` with the listed no-follow kind. Selection reconciliation must prove every selected name still appears exactly once in the visible projection. Filter-hidden or otherwise stale selections reject rather than expanding the drag to invisible Items.

The drag keeps those roots frozen but remains eligible only while the source tab remains active with the same confirmed location, ready listing, and visible revision. A model reset, navigation, tab switch, filter/sort projection change, tab close, or replacement drag invalidates it. A later selection change does not retarget or invalidate the frozen roots.

The operation scanner remains the filesystem race boundary after acceptance. It verifies roots without following links and applies the existing changed-source behavior. Do not add synchronous metadata I/O to mouse movement or drop delivery.

## Target resolution

Target resolution is application-owned and deterministic:

- **Browser folder:** the target names that Browser's exact active `TabId` as observed for the drag event. It resolves only if the same tab still owns a ready confirmed folder. Dropping on list empty space or non-row Folder Pane space uses this target.
- **Folder row:** the target names Browser, active `TabId`, visible revision, and row identity. The application resolves the row from its current visible entries and accepts an ordinary non-package directory or a symbolic link known to point to a directory. A package, file, other Item, or non-folder link rejects. The candidate is the listing location joined with the entry's exact name; C++ never sends a path.
- **Browser Tab:** the target names Browser and the button's stable `TabId`. It resolves that tab's confirmed ready folder even when inactive. Hover and drop do not call `activateTab`.

Every target captures a candidate only after current-state validation. A target that is loading, failed, missing, closed, relabelled onto another ID, or revision-stale rejects. There is no fallback to the Browser's new active tab, the other Browser, a parent folder, or the target shown when hover began.

Every drop, including a Browser folder or tab whose confirmed path may contain a link component, goes to a bounded Browser read lane before an operation starts. That preflight resolves the candidate to a canonical physical folder, rejects packages and links to packages, and compares physical source-parent and directory identities with the destination and its ancestry. This catches same-folder aliases and a destination within a dragged directory before an operation ID exists. The canonical folder, rather than a mutable link spelling, becomes the intent destination, so retargeting a link after preflight cannot redirect the operation. A missing, unreadable, changed, or no-longer-folder target rejects; completion also repeats the exact application source and target revision checks.

`OperationIntent::transfer` remains the final Qt-free policy check after physical preflight. The relevant relation is **destination within a dragged directory source**, not “source inside destination”: the former would recurse into the new copy or move. An ancestor destination remains ordinary unless physical preflight proves it is also the source parent.

## Native gesture and feedback

`FolderItemsList` enables drag initiation but keeps Qt's automatic item-model drag/drop disabled; the custom gesture prevents Qt from serializing model data or reordering rows. The list owns press/threshold/cancel mechanics, while a small workspace-level drag coordinator owns the one native `QDrag`, current hover target, and cleanup across both Browsers and their tab strips.

Each supported widget accepts drag-enter, drag-move, drag-leave, and drop only for a proven internal payload. Folder-row hit testing is repeated on every move. Target widgets ask for a pure preview and render the returned action and eligibility. The action is derived from the current user modifiers under the accepted M7 rule, then translated to `Qt::CopyAction` or `Qt::MoveAction`; `possibleActions`, `proposedAction`, or a platform cursor badge never overrides application policy.

Keep feedback transient and centralized so only one target is highlighted. Leaving a widget, receiving a model/tab revision, losing the window, cancelling, dropping, or finishing the native session clears it idempotently. A drop shows and announces “Checking destination…” while the asynchronous preflight runs, then starts the operation or replaces that cue with a specific rejection. The coordinator exposes hover action and eligibility to the accessibility seam without writing them into persistent application state.

Hovering a directory or folder-link row is preview only. It draws the target feedback but never opens the row, navigates into it, changes the active tab, or starts spring-loaded navigation. Directory entry on hover and every other spring-loaded folder or tab behavior are explicitly outside P4-M7.

Browser Tab file drops must coexist with the existing tab-reorder gesture. A drag whose source is a `BrowserTabButton` remains tab reorder; a drag whose source is a `FolderItemsList` may target a tab. The Close Tab Button and New Tab Button are not file targets. Folder-row drops must not trigger double-click activation, inline editing, context menus, Quick Look, type-to-filter, or ordinary selection handling.

## Operation handoff and refresh

An accepted drop creates the same `OperationKind::Copy` or `OperationKind::Move` intent used by clipboard Paste. Record the source Browser and tab as the operation origin for rejection and panel presentation, while affected-folder refresh continues to derive all source parents and the destination from the intent's roots.

Acceptance itself does not report success: it reports that the operation started. Later conflict, progress, cancellation, failure, partial completion, cleanup uncertainty, and Notice behavior come only from the existing coordinator. Copy never removes a source. Move removal happens only after verified destination work, including cross-volume moves. Links are roots and are never traversed; packages are directory roots for transfer even if the owner declines them as drop targets.

Do not invent a drag-specific refresh or post-operation selection path. The existing operation terminal transition refreshes every affected open location. If the accepted owner decision requires additional selection after refresh, that would be new application behavior and must be designed explicitly before implementation.

## Verification and sequencing

Implement in this order so each boundary can be reviewed independently:

1. Record the owner decisions and update UX GUI. Add Qt-free drag identifiers, target and rejection types, source capture, pure preview, one-use acceptance, cancellation, invalidation, and transition tests.
2. Add asynchronous physical-destination preflight on the bounded Browser read lanes, then connect its revalidated completion to `OperationIntent::transfer` and the existing operation coordinator. Add temporary-directory integration coverage for direct and link targets, aliases, packages, Copy, Move, conflicts, cancellation, partial results, and affected-folder refresh.
3. Add the CXX-Qt bridge and private payload codec. Prove that the codec contains no paths and rejects malformed, unknown, replayed, external-source, and wrong-workspace payloads.
4. Add the custom Folder Items threshold gesture, workspace hover coordinator, Browser-folder/folder-row/tab targets, live Option action, highlight cleanup, and accessibility/status presentation. Preserve tab reorder and every existing Folder Items gesture.
5. Add the isolated native check and guided macOS acceptance described by the overview. Then run the direct Cargo commands, `make check`, and all affected native scripts.

Qt-free tests should exercise event permutations, not only successful drags: preview then source revision; preview then target revision; modifier change between preview and drop; tab activation without ID change; tab close and ID replacement; cancel then late drop; accept then replay; external payload with a guessed token; and operation completion after the UI changes.

The native check uses a disposable profile and temporary folders. It should expose a deterministic seam that can prepare, move, drop, cancel, and inspect hover state without replacing the system clipboard or depending on Finder. The guided macOS run supplies evidence for behavior an offscreen plugin cannot prove: the real drag threshold, platform cursor/action badge, Option transitions during the drag session, pointer cancellation outside the window, and Finder drag rejection.
