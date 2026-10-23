# P4-M7: Drag and drop file items

Status: Active

This milestone adds file drag and drop within Dual Pane. P4-M1 fixes the internal-only boundary, supported target classes, and Move/Option-Copy rule. The [architecture plan](milestone-07-architecture.md) defines the safe handoff; this checklist is the definition of done.

## Scope

In scope:

- Pointer drags that begin from selected Folder Items in either Browser.
- Either Browser's confirmed folder, folder rows, and Browser Tabs as drop targets, including inactive tabs.
- Move by default, Option-Copy, live target/action feedback, rejection feedback, and cancellation.
- Frozen operation handoff through the existing transfer and operation workflow.
- Symbolic-link, package, same-folder, descendant, stale-source, stale-target, and unavailable-target behavior.
- Keyboard and menu alternatives, accessibility feedback, automated native checks, and guided macOS acceptance.

Out of scope:

- Finder-to-Dual Pane and Dual Pane-to-Finder drag interchange.
- Dragging Favorites, Browser Tabs, path components, or operation panels as file-transfer sources.
- Reordering Folder Items or changing sort order by dragging.
- Entering or navigating into a folder or tab by hovering during a drag, including spring-loaded navigation. Hover over a directory only previews that row as the prospective drop target; it never opens the directory.
- Hover-to-activate tabs and dropping onto breadcrumb or expanded-column targets from P4-M8.
- A second file-operation executor or any weakening of existing operation safeguards.

## Fixed rules

P4-M1 already fixes these rules, and this milestone does not reopen them:

- A file drag is accepted only when its source and destination are inside Dual Pane. External payloads start no operation.
- The supported target classes are a Browser's confirmed folder, a folder row, and a Browser Tab's confirmed folder.
- An accepted drop is Move by default and Copy while Option is held.
- Hovering an inactive Browser Tab does not activate it.
- Hover is preview only. Only an accepted drop creates a frozen operation intent, and later UI changes cannot retarget it.
- The existing [file-operation safeguards](../product-behavior.md#file-operation-safeguards) and [data-safety invariants](../architecture.md#44-data-safety-invariants) govern the resulting Copy or Move.

## Owner decisions

The owner accepted these decisions on 2026-10-23. They refine the fixed P4-M1 rules without adding external drag interchange or spring-loaded navigation.

P4-M12 implements a later owner-directed change to decision 2: a modified primary press may become a file drag, preserving the pre-press selection when the pressed row was selected and dragging only the pressed row otherwise. Other modifiers at drop select Move, while Option with any combination selects Copy. An attempted drop reaches a decision and shows its rejection reason after release, even when hover found the target ineligible. The rules and earlier native evidence below remain the P4-M7 baseline; the [P4-M12 plan](milestone-12-overview.md) holds the updated rules and still requires guided physical-pointer acceptance.

| # | Topic | Accepted behavior |
|---|---|---|
| 1 | Dragged selection | Pressing an already selected row preserves the current multi-selection, and crossing the platform drag threshold drags that exact selection. Pressing an unselected row first selects only that row; if the pointer then crosses the threshold, only that row is dragged. Empty space never begins a drag. |
| 2 | Click and cancelled-drag selection | Preserve the ordinary click result established on press. Cancelling a drag, or releasing before the threshold, does not restore an older selection and never reorders rows. Command- and Shift-modified presses keep their existing selection gestures and do not begin a file drag. |
| 3 | Source lifetime | Starting the drag freezes exact byte-identity roots and kinds. A later selection change cannot retarget them, but navigation, tab closure, a different rendered revision, or loss of the source listing makes the drag stale and rejects the drop. Source-file races after acceptance remain the operation scanner's responsibility. |
| 4 | Same-folder and descendant targets | Reject the whole drop before an operation starts when the destination is the source folder or is inside any dragged directory. Do not silently remove only the offending source. Briefly explain the rejection in the target Browser Status Bar. |
| 5 | Folder-row targets | Accept a real directory row or a symbolic-link row known to point to a directory. A package is an Item, not a drop folder, and a link resolving to a package also rejects. Hovering a directory or folder link only previews it as the drop target; it never enters, opens, or navigates into it. Source symbolic links remain transferred as links. |
| 6 | Target readiness and physical destination | A Browser or tab target must have a confirmed, ready folder, and a row must still name the same visible eligible target in that tab revision. Every drop clears hover, shows and announces “Checking destination…”, and resolves the physical destination asynchronously before an operation ID exists. The canonical folder is frozen for the operation; package targets, broken or changed links, loading, failed, missing, closed, superseded, same-folder aliases, and destinations physically within a dragged directory reject without fallback. Source and target state are revalidated after the check. |
| 7 | Post-drop activation and focus | A successful or rejected drop does not activate an inactive target tab, switch the active Browser, navigate into a folder row, or move keyboard focus. The existing operation panel, Notices, refresh, and selection behavior show the outcome. |
| 8 | Action timing | The modifier at the drop event chooses the action. Entering a target shows Move; pressing Option changes the feedback to Copy and releasing it changes back to Move. The application validates the requested action again when accepting the drop rather than trusting the cursor badge. |
| 9 | Hover and rejection feedback | An eligible target gets a clear outline and an action label such as “Move 3 Items” or “Copy 3 Items.” An ineligible target uses the prohibited cursor and no eligible highlight. A dropped rejection clears hover immediately, announces the reason, and briefly shows it in the target Browser Status Bar. Merely passing over an ineligible target stays silent. |
| 10 | Drop completion and selection | Acceptance leaves selection and cursor unchanged in every tab. Ordinary operation refresh later reconciles rows: moved Items disappear when the source refresh succeeds, while copied Items remain selected under the existing refresh rules. Dropping does not add a special post-operation selection policy. |
| 11 | Multiple source roots | One drag may contain every selected visible file, folder, package, and symbolic link from the source tab, in listing order. Validation is all-or-nothing: an invalid, stale, duplicate, or overlapping root rejects the drag rather than transferring a subset. |
| 12 | Native acceptance | Add an isolated offscreen check for event ordering and target state, then require one guided macOS run for the real drag threshold, Option transition, cursor/action badge, both Browsers, inactive tabs, cancellation, and rejection of Finder drags. All fixtures stay in a disposable profile and temporary directories. |

## Completion checklist

- [x] Before implementation, the owner decides every row in [Owner decisions](#owner-decisions). Record the accepted behavior in this overview, [UX GUI](../ux-gui.md), and the [Phase 4 changelog](changelog.md); update product behavior only if product scope changes.
- [x] Add application-owned drag state and typed start, preview, accept, cancel, and invalidation transitions. A drag token resolves only to roots captured from the exact Browser, tab, selection, and visible revision that began it; no native payload path is trusted as source identity.
- [x] The Folder Items List begins a drag only under the accepted selection and threshold rules. A click or cancelled drag cannot reorder Folder Items, mutate files, or produce a second selection change on release.
- [x] The private in-process payload is versioned, contains no source paths, is accepted only from a live Dual Pane Folder Items List and current application drag token, and is invalidated after completion or cancellation. Finder and malformed or replayed private payloads cannot start an operation.
- [x] Browser-folder, folder-row, active-tab, and inactive-tab targets resolve through stable Browser/tab identity and observed revisions. No target is inferred from a label, lossy path, current focus, or whichever tab happens to be active when the event is delivered.
- [x] Hover feedback identifies the exact target and current Move or Copy action without filesystem I/O or application-state mutation. Modifier changes update the action, inactive-tab hover does not activate it, and leaving or invalidating a target clears every visual and accessible hover state.
- [x] Drop acceptance reuses the same Qt-free validation as hover, consumes the drag token once, and asynchronously resolves and freezes every target's canonical physical folder before constructing `OperationIntent::transfer`. Same-folder and destination-within-source checks include path aliases; duplicate or overlapping roots, target and resolved package kinds, journal availability, and stale source/target identity are checked before an operation ID exists.
- [x] An accepted drop uses the existing conflict, progress, cancellation, partial-result, verified-before-removal, cleanup, Operation Panel, Notice, and affected-folder refresh paths. It does not add a second executor or bypass the safety journal.
- [x] Rejection covers same-folder aliases, destinations within a dragged directory, file/package/non-folder-link/package-link rows, broken or retargeted links, unavailable destinations, source or target navigation, closed tabs, changed listings, external drags, malformed or replayed tokens, unsupported modifiers, and cancelled drags. Every rejection starts no operation and changes no file.
- [x] Pointer drag leaves keyboard and menu alternatives intact: Copy/Move to Other Browser and Copy/Cut/Paste remain available under their existing focus and availability rules. Accessible names, descriptions, announcements, action changes, and rejection text follow the accepted M7 feedback policy.
- [x] Qt-free tests permute source capture, selection changes, source and target revisions, tab activation/closure, modifier changes, preview/accept disagreement, token reuse, cancellation, and late events. Domain tests cover stable root order, duplicate/overlapping roots, same-folder rejection, and destination-within-source rejection.
- [x] Temporary-directory integration tests prove Copy keeps every source, Move removes a source only after verified destination work, links are transferred as links, packages are transferred as directory roots, conflicts use the existing decisions, cancellation cleans temporary artifacts, and partial/failure outcomes refresh all affected open tabs.
- [x] Add `scripts/check-file-drag-drop.sh` (or an equivalently focused native check) using `DUAL_PANE_TEST_PROFILE`. It covers both Browsers, Browser-folder and folder-row targets, active and inactive tabs, live Option changes, stale source and target revisions, cancellation, and external/malformed payload rejection without touching real user files.
- [ ] Record the guided macOS acceptance from decision 12, including any platform limitation that cannot be synthesized offscreen. Do not treat synthetic `QDropEvent` delivery alone as proof of the real platform drag gesture and action badge.
- [x] Update affected product, UX, architecture, documentation-index, command-list, and native-check documentation in the same change. Add no dependency or build tool without separate owner approval.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, and so do `make check` and the M7 native check. Record exact evidence here before marking P4-M7 Done. Phase 4 remains open until P4-M8 and every exit criterion are complete.

## Acceptance evidence

Owner decisions were recorded on 2026-10-23. On 2026-10-23, direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed. After implementation review fixes, `make check` passed the complete formatter, linter, and test gate; `scripts/check-path-entry.sh` passed; and `scripts/check-file-drag-drop.sh` passed with eight focused application tests and completed temporary-file transfers in both Browser directions. The offscreen check covers token-only payload encoding, Move/Option-Copy preview, Browser-folder and folder-row targets, an inactive tab, stale and cancelled sessions, replay, and completed Copy/Move semantics. The guided real-pointer macOS acceptance remains pending because the real platform drag threshold and action badge cannot be proved by synthetic `QDropEvent` delivery.

P4-M12 later expanded the offscreen check to deliver press, move, release, drag-enter, and drop events through the widgets and to complete more target classes. That evidence does not check this milestone's guided physical-pointer item; it stays unchecked until the [M12 guided run](milestone-12-overview.md#guided-real-pointer-acceptance) is recorded.
