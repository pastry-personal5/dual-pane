# Phase 3 changelog

Status: Active

Chronological record of decisions and plan changes for Phase 3, newest entry first.

## 2026-10-03 — P3-M9 saved session integrity

- **Owner decision:** a saved session with tabs but no window-layout row is damaged as a whole. It opens the Home fallback and records Session Not Restored; it does not restore the tabs at the default layout. A database with no session rows remains a valid no-session case.

## 2026-10-03 — P3-M9 plan review follow-up

- Made restored-folder read priority deterministic: the saved active Browser's active tab, the other Browser's active tab, then the remaining tabs in Left-then-Right strip order.
- Required one read transaction for the settings snapshot and session rows, plus Qt-representability checks for persisted layout values, so a concurrent write or malformed numeric value cannot yield a torn or unsafe launch layout.
- Made the timeout boundary concrete: Reset remains unavailable while the sole settings worker is still loading, then can clear session-write protection only after a late successful load and a successful reset. Added explicit quit-while-Waiting behavior and verification.

## 2026-10-03 — P3-M9 planning

- Planned P3-M9 as one milestone with an ordered checklist ([overview](milestone-09-overview.md), [architecture](milestone-09-architecture.md)).
- **Plan review:** distinguished malformed session rows from SQLite read failures, so a transient storage fault cannot trigger a writable Home fallback; made Reset an ordered barrier for both save kinds; required a final flush when only the session changed; specified an initial save after valid load or damaged-session fallback; and made startup signal ordering and late-load behavior testable. Corrected the P3-M2 and P3-M4 handoffs that still named stored filter state.
- **Owner decisions** from the planning questions and a follow-up interview, recorded in the [included product scope](../product-behavior.md#included-product-scope), the [session recovery policy](../product-behavior.md#session-recovery), and [ux-gui](../ux-gui.md#notices-and-restoration-feedback):
  - **Discard rule:** a restored tab is discarded when its first read finds the location missing, not a folder, or unreadable for permission, privacy, or unknown reasons, as the Favorites launch probe does. Reader failures, a full read queue, and cancelled reads never discard, and a read that never answers keeps the tab loading.
  - **Restore display:** saved tabs appear at once and leave the strip when their read fails, with the active tab moving to the nearest surviving tab on the right, then the left. Each discarded tab gets its own Notice.
  - **Fallback:** every fallback opens a clean Home tab, including when no session exists. The launch working directory, `/` for a Finder or Dock launch, is no longer used at startup, which replaces the 2.0.0 launch behavior.
  - **Damaged session:** invalid session rows drop the whole session with a routine Notice; settings, Favorites, and saving keep working.
  - **Startup Notices:** a startup period that ends when launch work has answered, or about five seconds after the window appears. With the box unchecked, the first startup notice opens Notices without focus. With it checked, only notices that offer an action open it, so temporaries-sweep failures are recorded silently; this narrows the P3-M8 rule for that case. Notices about the person's own actions keep their usual rules.
  - **Focus:** keyboard focus follows the restored active Browser.
  - **Window layout:** relaunch also restores the window's frame, its zoomed or full-screen state, and the Sidebar Splitter and Browser Divider positions. A frame that no longer fits moves onto a connected display. This widens the product scope and a [Phase 3 exit criterion](phase-3.md#exit-criteria).
  - **Reset Settings:** a successful reset also closes every tab, leaving each Browser one clean Home tab, makes the Left Browser active, and returns the window and splitters to their defaults.
- **Follow-up owner decisions:**
  - An invalid saved layout row damages the whole session.
  - A successful Reset Settings leaves full screen or zoom and centers the default size on the current display.
  - A slow settings load shows a separate animated Waiting Window. After ten seconds it opens Home tabs at the default layout and reports a storage timeout. A later load supplies settings and Favorites but never replaces the visible workspace or saves this launch's session changes; the old session remains available on the next launch unless the person deliberately resets settings.
- **Architecture choices:** schema version 4 session tables in the settings database, written by a separately coalesced session save in its own transaction. The layout is stored as typed values in logical points rather than Qt's opaque geometry bytes, and fitting it to displays is a Qt-free function. The plan treats a missing layout row beside saved tabs as damaged, pending owner confirmation; the first layout report gates the initial save so every written session has its row. Only a read the person asks for ends a tab's restoration; the workspace's own re-reads, such as after an invalidation, keep its discard rule. A tab that never confirmed a location keeps its requested one for its label, the session, and Refresh.
- **Filter compatibility:** schemas 1–3 have no session or filter table, so nothing stored needs migration. The unused `TabSnapshot.filter` field is removed, closing the P3-M4 and P3-M5 handoff.

## 2026-10-02 — P3-M8 human check

- The deferred native [human check](milestone-08-overview.md#human-check) passed. The twelve items that waited for it are now checked, so every P3-M8 checklist item is checked and the milestone meets its definition of done without the exception recorded below. The open item that tracked the check is resolved and removed. `make check` passed.

## 2026-10-02 — P3-M8 Done

- The owner marked P3-M8 Done without the native human check, as an explicit exception to the definition of done. Every automated item is checked, and `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed. The twelve items that depend on the native interface stay unchecked, and the native behavior of the checked Trash and default-application opening items is part of the same check. Their verification, and any fixes or enhancements it leads to, moves to a later phase, tracked in open items until the [human check](#2026-10-02--p3-m8-human-check) resolved it.

## 2026-10-02 — P3-M8 Trash on a worker thread

- With the owner's approval, the ignored `moving_to_the_trash_works_on_a_worker_thread` test ran once on Qt 6.11.2. `QFile::moveToTrash` moved its temporary file into the real Trash from a worker thread and returned the path in the Trash. Trash therefore stays on the file-operation workers; the GUI-thread queue is not needed.

## 2026-10-02 — P3-M8 shortcut scope and runtime tests

- **Shortcut scope:** the application catalogue now records where each action's shortcut acts (`ActionId::scope`): the five file commands and Show Package Contents need a focused Folder Items List, Quit works in every window, and every other action is window-wide. The workspace bridge reports it as `bindingScope`, and the Qt binder installs shortcuts from it instead of its own list of action names, so the focus scope has a Qt-free test.
- **Runtime and bridge tests:** an attached file-operation lane delivers its events through the runtime; a Replace or Skip decision round-trips through the lane, and a replayed pre-decision result changes nothing; lane shutdown returns within its bound while a step ignores cancellation; Decision Card choice codes and file-command codes match their Qt constants, and an unknown choice code chooses nothing.
- **Action terms:** [ux-terms](../ux-terms.md) now lists New Folder and the six P3-M8 action IDs as catalogued current commands.

## 2026-10-02 — Relative Date hue range

- The owner narrowed the Relative Date strip hue from 0°–270° to 10°–260°; saturation and lightness are unchanged. A Qt 6.11.2 color probe measured active endpoints `#E8B9B0` and `#C3B0E8` with at least 10.22:1 black-text contrast, and dimmed endpoints `#C2A099` and `#A799C2` with at least 7.69:1 contrast. The dimmed color had lower luminance at each of 10,001 sampled positions. The [GUI rule](../ux-gui.md#folder-items-fields-and-display) records the new range.

## 2026-10-02 — P3-M8 presentation

- **Name editors:** `Command::RequestNameEditor` opens the Rename or New Folder editor only when the command is available, and otherwise reports `OperationRejected`. A refusal therefore reaches the Status Bar the same way a refused shortcut does.
- **Retries:** `Command::RetryName` retries a refused name from the refused job's frozen folder and target, so a selection changed in the meantime cannot retarget it. An invalid retry keeps the refused job. Closing an editor closes its refused job.
- **Decision Cards:** a card click carries the token of the card that was shown, so a click on a replaced card is ignored.
- **Availability:** menu enablement queries availability when the Folder Items Context Menu opens, not on every input, which keeps large selections off the GUI thread's per-input path.
- **Status Bar:** each refusal reason carries a token, and only the timer started for that reason clears it.
- **Dismissal:** presentation dismisses every finished job: after its panel hides or closes, or at once when it finished unseen. Dismissal also releases what the runtime kept for the job.
- **Panels:** a floating panel shows a close button only once its job has finished.
  - **Quit routing:** a close request that reaches a running panel during a quit goes to the workspace window's quit decision instead.
  - **Docking:** dragging a panel's title onto the Operation Panel Strip docks it, and dropping a docked panel elsewhere floats it there.
- **Confirmations:** the Permanent Delete Confirmation Window is non-modal, so it survives tab and selection changes. The quit prompt is a window-modal sheet, and ux-terms gains its name, Quit Confirmation Window.

## 2026-10-02 — P3-M8 native executor

- Added `libc = "=0.2.189"`, the version already in `Cargo.lock`, as a direct desktop dependency. Its license is MIT OR Apache-2.0, which is compatible with Apache-2.0. All of its `unsafe` calls live in `native_calls.rs`, and [AGENTS.md](../../AGENTS.md) names that module.
- **Interrupted steps:** a step that a cancel interrupts after completing entries reports them. While cancelling, the application accepts that one result for progress only and silently, so the outcome is Partial and affected tabs refresh. A late result still cannot reopen or retarget the job.
- **Progress:** an `Execute` effect carries the job's accepted progress, so the executor extends exactly what the application holds.
- **New error kind:** a Copy or Move folder root whose destination folder lies inside it through a link or a case variant raises a recoverable destination-within-source error. The check compares device and inode ancestry.
- **Leases:** scans read only and hold no write lease. Steps, finalization, and cleanup lease the operation's destination roots, and for a move, Trash, or deletion its source roots too.
- **Packages:** a listing asks Qt about package status only for folders with an extension, which keeps large listings fast. A link is resolved to its target before that check, because `QFileInfo::isBundle` does not follow it.
- **Exclusive rename:** a volume without it reports a recoverable error and never falls back to a plain rename. Volumes without `RENAME_EXCL`, such as some exFAT and network volumes, need the human check.

## 2026-10-02 — P3-M8 application contract

- Implemented the Qt-free contract extensions with their own test suite; the P3-M7 suite is unchanged.
- **Name problems:** `NameCollision` keeps its P3-M7 shape, and sibling `NameRejected` variants carry too-long and rejected-character problems. This departs from the planned single typed rejection so that the P3-M7 tests compile unchanged.
- **Refresh:** only a terminal outcome with completed entries rereads affected tabs. Uncertain cleanup alone changes no listing, because this launch's temporaries are never listed.
- **Links:** every link activation resolves on a worker first, because a link to a folder may point to a package.
- **Notices:** failed, partial, and uncertain-cleanup summaries open Notices; successes and clean cancellations are only recorded.

## 2026-10-02 — P3-M8 planning

- Planned P3-M8 as one milestone with an ordered checklist. Owner decisions:
  - **Native layer:** pinned `libc` for native calls; Qt for Trash; clone/copy with full metadata.
  - **Copies:** every copied file goes through a temporary and `RENAME_EXCL`. Replace uses plain `rename()`. A cross-volume source-removal failure is a recoverable error.
  - **Runtime:** a two-worker file-operation lane; a 500 ms panel reveal; floating panels plus the drag-to-dock strip.
  - **Feedback:** refresh at terminal outcome only; item progress plus current-file bytes; a confirmed quit while jobs run.
- Safety journal: a separate SQLite file, which satisfies the P3-M2 separation rule. It writes one record per operation and destination directory. Temporary names carry a random launch ID. The launch sweep deletes only exact-pattern files or links from earlier launches. If the journal cannot open, Copy is disabled until Try Again in its Notice reopens it, and a cross-volume move item raises a recoverable error.
- P3-M7 contract extensions, recorded here and in the [P3-M8 architecture](milestone-08-architecture.md). P3-M7's checklist and tests are unchanged.
  - **Folder finalization:** a post-order phase for permanent deletion and moves, because a folder-removal failure could not be expressed. [architecture.md](../architecture.md#43-file-operations-and-decisions) step 7 is updated.
  - **Rename and New Folder:** typed name rejections for the editors, plus a name-less availability query.
  - **Jobs:** job dismissal, byte-progress events, and new error kinds. A worker panic reuses the executor-unavailable terminal event instead of a new internal kind.
  - **Opening and quit:** a package flag on listed entries, set by the directory reader; an open-item request for links; the quit confirmation state; and a journal-status event.
- Interaction decisions:
  - **Packages and scope:** activating a macOS package opens it, and Show Package Contents navigates in. This widens the included scope and removes the package out-of-scope line in [product-behavior.md](../product-behavior.md).
  - **Shortcuts:** file-command shortcuts act only while the Folder Items List has focus; New Folder stays window-wide.
  - **Editors:** they commit asynchronously and reopen on a name problem after focus loss. The New Folder Name Editor is an empty row pinned at the top.
  - **Decision Cards:** they never take focus and have no default button. Floating panels join macOS window cycling, and docked panels are in the Tab order.
  - **Missing folders:** the tab keeps its path and rows and shows the Missing Folder Overlay.
  - **Trash:** a volume without Trash is a recoverable error, never permanent deletion.
  - **Temporaries:** only current-launch temporaries are hidden, so leftovers stay visible. The owner confirmed this narrowing of the earlier hide-all answer.
- Verification is automated first, then a human check for drag-docking, real Trash, default-application and package opening, and card focus.

## 2026-10-02 — P3-M7 follow-up review

- A Rename or New Folder name collision no longer leaves an uncancellable job. Cancelling it, like cancelling a pending permanent-delete confirmation, closes the job as cancelled without a cleanup request.
- New Qt-free tests cover the domain intent and decision policy (`operation_intent.rs`) and plan validation for root order, missing or duplicate roots, and Rename destination mapping.

## 2026-10-02 — P3-M7 safety review

- The owner chose **Skip** on a recursive scan error to omit the unread subtree and continue scanning other targets. A containing directory with skipped content remains during permanent deletion.
- Review fixes require contiguous depth-first scan plans, typed scan-skip placeholders, destination-kind evidence for conflicts, and progress that accounts only for chosen skips and completed entries. A recoverable execution error retains its native error kind for the pending decision. Clean cancellation with no completed changes remains clean even after skips.
- The Qt-free regression suite now covers scan retry and skip, subtree and conflict-result validation, skip accounting, cancellation outcomes, and stale results. The review keeps native execution and visible controls with P3-M8.

## 2026-10-02 — P3-M7 complete

- The workspace-owned Qt-free operation workflow and fail-closed desktop request path are implemented. All P3-M7 completion items are checked; direct Cargo format, Clippy, and test commands and `make check` passed.

## 2026-10-02 — P3-M7 operation workflow decisions

- Commands use each addressed active tab's settled, successful listing. Selected targets follow listing order and retain exact name bytes and entry kinds. Copy and Move also require a settled destination and reject a real directory sent into its logical subtree; a link remains a link.
- Jobs belong to `Workspace`, with workspace-local operation IDs, generation-checked scan and step results, unique decision tokens, and frozen roots. Permanent Delete waits for confirmation of its frozen target count before work starts.
- Only regular-file conflicts permit Skip, Replace, or Cancel and conflict-only apply to all. Link collisions, kind mismatches, and recoverable errors permit Try Again, Skip, or Cancel. Rename and New Folder collisions are typed inline-editor results with no overwrite choice.
- Cancellation stays pending through cleanup, preserving completed work and reporting clean, partial, failed, or uncertain outcomes. The desktop returns executor-unavailable for any M7 operation request; M8 supplies the native executor and interface, including shortcut rejection text in the source Browser Status Bar.

## 2026-10-02 — P3-M6 complete

- The owner accepted the native macOS verification, including the Favorites group-menu refinement that removes the detached downward caret and keeps the three-dot menu and add button.
- Every P3-M6 checklist item is complete. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed.

## 2026-10-02 — P3-M6 Relative Date strip refinement

- The owner set the active strip to HSL hue 0° through 270°, 55% saturation, and 80% lightness. The dimmed strip keeps the previous 15-point saturation and six-point lightness reductions, so it uses 40% saturation and 74% lightness. Qt measured active endpoints `#E8B0B0` and `#CCB0E8`, dimmed endpoints `#D7A2A2` and `#BDA2D7`, and minimum black-text contrast above 8.68:1 in both states.
- The owner deepened the inactive strip to 25% saturation and 68% lightness. Its endpoints are `#C29999` and `#AD99C2`; Qt measured at least 7.68:1 contrast with black text and lower luminance than the active strip at every sampled position.

## 2026-10-02 — P3-M6 Relative Date color implementation

- The adapter now maps each Item's age from its listing-time snapshot to the planned strip position. It retains that snapshot per tab and updates Relative Date cells on a reload even if the rows did not change.
- The desktop model supplies the position to the Qt delegate, which paints black Relative Date text over the planned color or a neutral background for missing metadata. Qt-free mapping and reload tests pass, and a Qt color probe confirms the endpoints and black-text contrast. Native visual verification remains open, so the [completion item](milestone-06-overview.md#completion-checklist) stays unchecked.
- The owner reduced the strip's saturation by 25 percentage points to 75%. A Qt color probe measured `#EC7979` and `#B379EC` at the endpoints and at least 5.76:1 contrast with black text.
- The Relative Date background now dims with its Browser or the window, using a darker, muted strip and neutral missing-date background. The list repaints on active-state changes, and a Qt color probe measured at least 4.79:1 black-text contrast for the dimmed strip.

## 2026-10-02 — P3-M6 Relative Date color plan

- The owner added a black foreground and an age-colored background behind Relative Date text. The virtual strip runs from bright red at one hour or less through positions 0.25 at one day, 0.50 at seven days, and 0.75 at 30 days to bright violet at 365 days or more. The [GUI rule](../ux-gui.md#folder-items-fields-and-display) defines the interpolation, missing-date treatment, and selection visibility; the [M6 architecture plan](milestone-06-architecture.md#relative-date-color-plan) identifies the adapter and Qt work.
- This is an unchecked addition to the active [M6 checklist](milestone-06-overview.md#completion-checklist). The earlier implementation and verification evidence does not cover it.

## 2026-10-02 — P3-M6 interpretation review

- The owner reviewed the implementation interpretations. The [open item](open-items.md) is removed.
- Changed:
  - **Folder Pane summary:** Row #2 shows the Item count at the left, followed by `N selected` in a muted dark blue when Items are selected. The right-aligned size shows `selected / total`, such as `1.2 MB / 4.5 MB`, or only the total with no selection. A folder with no non-folder Items totals `0 B`.
  - **Links to folders:** they count as folders. Size is blank and they are excluded from totals; Type stays `[LNK]`.
  - **Sort buttons:** tooltip and accessible name read `Sort items by type (A-Z)` and `(Z-A)`. The agent chose `name (A-Z)`, `date (oldest first)`, `date (newest first)`, and `size (smallest first)` for the other fields, pending owner confirmation.
  - **Type column:** its minimum width drops from 52 to 40 pixels.
- Confirmed unchanged: relative date units, Notice wording and behavior, keyboard traversal, and focus on activation.

## 2026-10-02 — P3-M6 implementation

- The agent implemented all four [P3-M6](milestone-06-overview.md) steps, and the automated gate passes. The milestone stays Active. The launch-probe checklist item is checked. The other items wait for the owner's native-verification decision; the [acceptance evidence](milestone-06-overview.md#acceptance-evidence) lists what automated tests cover and what they cannot.
- Design changes from the plan text:
  - **Tab-addressed gestures:** every gesture on a Browser's visible content (selection, open, Back, Forward, Up, Refresh, and scroll hint) names the tab it observed, and the application ignores it once that tab is no longer active.
  - **`SetSort`:** it names the Browser, tab, and confirmed location, and is ignored unless that tab still shows that location. In P3-M5, sorting a pending target location was also possible.
  - **Settings events:** `SettingsSaveFailed` carries the failure category. `ResetSettings` and `LoadSettings` are work requests that the session routes to the settings worker. A reset is accepted only after the current load has settled.
  - **Projections:** the application exposes `BrowserChrome` and `WorkspaceChrome`, typed `FavoriteEditRejected` outputs, and application-owned Notices. Scroll reports update the tab's scroll hint without activating its Browser.
  - **Bridge:** the desktop gains a second CXX-Qt object, `WorkspaceBridge`, for the shared session, Favorites, Notices, and shortcuts. The Folder Items model became a six-column table model.
- Dependencies and native evidence:
  - `jiff` 0.2.37 was added to `dual-pane-adapters` as approved, with `std`, `tz-system`, and `tzdb-zoneinfo` only. It brings in `jiff-core` 0.1.1, which has the same Unlicense OR MIT license.
  - A local Qt 6.11.2 spike confirmed that the Cocoa platform supports pixmaps off the GUI thread. Native icons therefore render on a worker thread through `QAbstractFileIconProvider` without Objective-C++ or a build change.
- The [interpretation review](#2026-10-02--p3-m6-interpretation-review) settled the implementation interpretations.

## 2026-10-02 — P3-M6 third plan review

- The owner settled the open cursor question. Movement keys start from the cursor even when no Item is selected, so after a Command-click deselects the last selected Item, Down selects the next row. Return and Right activate only when exactly one Item is selected. [Selection rules](../ux-gui.md#selection-and-file-commands) record both.
- [Sidebar Favorites](../ux-gui.md#sidebar-favorites):
  - A single click opens a Favorite Item and focuses the active Browser's Folder Items List.
  - New Group starts as an empty draft row that exists only after a valid name is committed.
  - Inline editors commit on Return and cancel on Escape. Focus loss commits valid text and silently cancels invalid or duplicate text.
- While an inline editor has focus, only `Command+Q` remains active, and quitting discards the draft. `CloseWindow` loses its default shortcut when `Command+W` moves to `CloseTab`. The Folder Items List has no column header row and sizes its columns automatically.
- Plan fixes:
  - All five delivered action IDs join the catalogue. Every delivered shortcut binds from the effective settings.
  - Schema version 3 unbinds the stored default `CloseWindow` `Command+W` before preload, so `CloseTab` is not left unbound. Tests check the effective bindings, including a v1-to-v3 chained upgrade.
  - The tab limit becomes a domain `BrowserTabs` invariant. Favorites edits gain a not-ready rejection, and Sidebar editing stays disabled until Favorites are known.
  - The root volume name is read off the GUI thread. Tab labels use the confirmed location.
  - Reset Settings requires a settings worker, warns that current-session changes will be replaced, and reports where the failed database was preserved.
  - Icons are requested lazily into a bounded cache, and column hiding follows minimum widths.

## 2026-10-02 — Tab history and visit state in the domain

- The owner moved tab history, cursor, range anchor, and scroll position into the domain, as the [P3-M5 plan](milestone-05-architecture.md) intended. This supersedes the "application state" wording in the architecture review entry below. [Architecture §3.1](../architecture.md#31-domain--stable-file-manager-policy) now names the domain types:
  - `TabHistory` holds a tab's ordered `Visit`s and a current position that always stays in range. It handles new visits, Forward truncation, and Back/Forward arrival.
  - Each visit's `VisitState` holds the `Selection`, cursor, range anchor, and `ScrollAnchor`. It applies plain, Command, Shift, right-click, Select All, and clear gestures, checks each named entry against the current Folder Items, and reconciles after a reload in linear time.
  - `ScrollAnchor` replaces the `(EntryName, i32)` scroll hints in `Command::UpdateScrollHint` and the listing outputs.
- Clearing the selection also clears the cursor and keeps the range anchor, so a later reload or tab switch no longer reports the cleared row as the cursor row again.
- `Workspace` keeps request tokens, pending reads, the rule that a pending Back or Forward counts as taken, and output emission; selection gestures delegate to the domain. A domain property test checks that random gestures, reloads, and arrivals keep every name present and the history position in range. The existing application tests pass unchanged apart from the new scroll-hint type.

## 2026-10-02 — MVVM and command-based architecture styles

- The owner made MVVM and a command-based style part of the decided architecture. The new [Architectural styles](../architecture.md#architectural-styles) section names Clean Architecture as the dependency frame, MVVM with one-way binding for presentation, and typed command messages handled by `Workspace::handle` for input. It maps each role to its ring and code and states the binding and command rules. "Command-based" means command messages, not command objects with `execute` or `undo`; undo remains outside product scope.
- Conformance fixes:
  - The desktop no longer builds commands or formats display text itself.
  - Focus activation and clearing the selection now go through `UiEvent` and the `InputController`.
  - The controller chooses Home, End, Page Up, Page Down, and Return targets. The View supplies only the fully visible row count, and it keeps the selected row visible after every keyboard movement, including Up and Down.
  - `BrowserViewModel` formats the folder-name label.
- Shortcuts are still hard-coded Qt key sequences until P3-M6 wires delivered actions to the `ActionId` catalogue.

## 2026-10-02 — Architecture review fixes

- One `Output::FolderItemsLoaded` replaces the paired `FolderItemsRowsChanged` and `FolderItemsReplaced` outputs. It carries the listing with `changes: Option<Vec<RowChange>>`, and `None` means every row is replaced. The reducer alone decides between a row delta and a full replacement, so the presenter no longer stores a pending change or compares locations to decide it again.
- Location invalidation is an externally observed fact, so `Command::InvalidateLocation` became `Event::LocationInvalidated`. P3-M10's watcher submits it.
- The Qt-free `InputController` now maps one-row arrow movement and cursor activation (`UiEvent::MoveSelection`, `UiEvent::ActivateSelection`) to `Command::MoveSelection` and `Command::OpenEntry`. The desktop session no longer computes target rows.
- A Favorites edit that changes nothing, such as moving a Group or Item to its current position, no longer emits `FavoritesChanged` or queues a save.
- Docs: the [data-safety invariants](../architecture.md#44-data-safety-invariants) moved verbatim from the planned remote-volume section to their own section, and their references now link there. Architecture §3.1 now describes the domain types that exist (`BrowserTabs`, `Selection`, and the Favorites hierarchy); tab history, cursor, anchor, and scroll are application state. Two notes were added: settings work runs on its own worker, and the bridge's session mutex only holds the session for the GUI thread.

## 2026-10-02 — Row deltas on workers and background Screenshots probe

- The owner chose to move the Folder Items diff to the workers, as [architecture §3.4](../architecture.md#34-frameworks-and-drivers--replaceable-mechanics) requires. A same-folder `ReadDirectory` request carries the tab's shown Folder Items as `previous`; the reader lane sorts, diffs against them, and returns the row change in `FolderItemsLoaded`. The reducer no longer compares listings. It accepts the change only when it was computed against the still-shown Folder Items and fits their row count; otherwise it replaces every row.
- The owner chose to move the launch check of `~/Documents/Screenshots` off the main thread. `main.rs` no longer probes it. After a load finds uninitialized Favorites, the application requests `ProbeScreenshotsFolder`, and the runtime's new serialized probe lane answers with `ScreenshotsFolderProbed`. Favorites are seeded and saved when the result arrives, so only fresh profiles probe at all. Favorites edits wait for the probe as they wait for a load. A failed probe, such as a privacy denial, omits Screenshots, and the lane's bounded queue answers `Failed` at once when it is full or stopped, so seeding never waits indefinitely. P3-M6 Favorite target probes reuse the lane.

## 2026-10-02 — Pre-M6 codebase and docs review

- Hardening for paths P3-M6 and P3-M10 make frequent: a reload reconciles selection, cursor, anchor, and scroll hint against a set of present names, so refreshing a large selected folder is linear instead of quadratic. The settings worker writes a save queued before a `Load` first, so a load reports what storage holds; a confirmed `Reset` still discards saves queued before it. Neither path is reachable from the 2.0.0 window.
- Reordering a tab to its current position is a no-op and emits no `TabsChanged`. Mapping a Folder Items gesture no longer clones the Browser view model. The C++ format and lint scripts also check `settings_glyph.hpp`.
- Docs: the [architecture §5.2](../architecture.md#52-planned-desktop-execution-model) status note now describes the current reader lanes, its lane table renders, and the property-testing note names the approved `proptest`. Completed milestone architecture docs are marked Done, the [repository tree](../planned-repository-architecture.md#repository-layout-and-future-examples) is redrawn with `settings_storage.rs`, and the P3-M6 native-verification step matches the automated-first rule.

## 2026-10-02 — Item Icon Column

- The owner added an [Item Icon Column](../ux-gui.md#folder-items-fields-and-display) as the first Folder Items column. It shows macOS native icons loaded off the GUI thread, has no header or sort buttons, and is the last column hidden as a Browser narrows. P3-M6 delivers it with the other columns.

## 2026-10-02 — Borderless Folder Pane commands

- The owner removed the border from the Up Button and the eight sort buttons in Folder Pane Toolbar Row #3; [visual layout](../ux-gui.md#visual-layout-and-accessibility) records the rule.

## 2026-10-02 — Pre-M6 review fixes

The owner asked for every finding of the Claude Code review of the P3-M2/P3-M5 code to be fixed before P3-M6 starts.

- Settings safety: the application tracks whether stored settings are loading, loaded, or failed, and saves only after a successful load. A sort chosen before the load result is replayed over the loaded snapshot; Favorites edits wait for the load. After a failed load, edits stay in memory and are never saved, so a damaged database is preserved until an explicit reset. Load failures arrive as a typed `SettingsLoadFailed` event, and a session without a settings worker settles as failed instead of loading forever.
- Quitting flushes unsaved settings with a bounded wait after the Qt event loop exits, as [architecture §5.2](../architecture.md#cancellation-and-shutdown) allows. The settings worker stays available after a failed open, and a reset that cannot move every file restores the moved ones and never falls back to an in-memory database.
- Back and Forward count a pending history step as taken, so repeated presses keep moving; `can_go_back` and `can_go_forward` expose the same rule for P3-M6. The application now uses the domain `BrowserTabs` for tab order and activation instead of a duplicate.
- Each Browser reads folders on four lanes, so a slow folder in one tab no longer delays the Browser's other tabs. Correction to the P3-M5 entry: admission was bounded per Browser, not per tab; it is now bounded per Browser at the pending capacity plus the lane count. Four lanes is provisional until measured.
- Same-folder reloads report one minimal row change, and the Qt list model applies it as row insertions, removals, and `dataChanged` notifications instead of a reset. Navigation and tab switches still replace every row.
- Settings schema version 2 gives `NavigateParent` its documented `Command+Up` default, replacing a stored plain `L` that version 1 used because shortcuts could not name arrow keys. A NULL key now means an unbound action; the migration restores defaults for NULL rows written by version 1, and unknown key text drops only that binding. P3-M6's `CloseTab` migration becomes schema version 3.
- Favorite target probes report `FavoriteProbeOutcome`, and only `Unavailable` removes an Item; P3-M6 still adds the probe runner. Listing events no longer have a panicking address path, settings tests clean up their temporary directories, `main.rs` reuses the settings path helper and runs without persistence when `HOME` is unset, and the application has a property test for tab, selection, and stale-result invariants.
- `scripts/lint-cpp.sh` passes Qt Core's header directory to clang-tidy, matching the compiler, because a CXX-Qt list header includes Qt headers without the `QtCore/` prefix.

## 2026-10-02 — P3-M6 interview decisions

- Development continues with Claude Code. The owner commits changes; the agent does not commit or push unless asked. Codex's existing code is reviewed before P3-M6 implementation starts, and P3-M6 stays one milestone delivered in its four planned steps.
- Verification uses automated tests first; the owner is notified when an automated check fails or cannot establish a checklist item. This replaces the earlier no-display exception question.
- [Folder Items fields](../ux-gui.md#folder-items-fields-and-display): Type shows `[DIR]` for folders, `[LNK]` for every symbolic link, and otherwise the lowercase text after the last `.` (blank without one). Type sort uses that text, replacing P3-M5's kind-only key. Folders have a blank Size cell and are excluded from totals. Relative dates show `now` under one minute or in the future and `N min` under one hour; sizes below 1 KB show whole bytes.
- The owner approved adding the `jiff` crate (MIT/Unlicense) to `dual-pane-adapters` for local-time Exact Date formatting.
- [Browser Tabs](../ux-gui.md#browser-tabs-strip-and-navigation) shrink and elide without scrolling, with at most eight tabs per Browser; New Tab is disabled at the limit. The root folder is labeled with its volume name, falling back to `/`.
- [Add Favorite Item](../ux-gui.md#sidebar-favorites) with an alias already in the group adds nothing and shows a brief inline “Already in this group” cue instead of a Notice.

## 2026-10-01 — P3-M6 second plan review

- The owner kept the Path Edit Control read-only and outside keyboard traversal. Favorite name validation stays in the inline editor without adding a Notice; automatic removal of an unavailable Favorite Item adds a Notice naming it.
- The P3-M6 plan now includes the settings worker's failed-open recovery path, typed storage failures, post-reset reload and stale-save invalidation, and multi-column row updates. Its native-verification item stays unchecked when no screen is available. The owner requested a note that a separate owner decision is needed on whether a documented no-display blocker may satisfy that item.

## 2026-10-01 — P3-M6 plan review and owner decisions

- The owner chose to show M6 storage and Favorites errors in Notices, with actionable storage errors opening Notices even at startup. M6 also delivers the P3-M2 explicit Reset Settings recovery UI; reset requires confirmation, preserves the failed database, and reports its outcome. Operation and restoration messages remain with P3-M8/P3-M9.
- The owner chose to migrate the workspace `Command+W` binding from `CloseWindow` to the new `CloseTab` action while retaining both stable IDs and the `CloseWindow` meaning. A focused Notices window handles `Command+W` locally.
- The [revised M6 plan](milestone-06-overview.md) adds typed Favorite edit rejections, stable-tab validation for delayed gestures, multi-column model work, launch probe sequencing, and direct native verification evidence.

## 2026-10-01 — P3-M5 review fixes

- Pending navigation now receives refreshed sort and settings choices without losing its target; settings load refreshes each open tab once. Right-click preserves a missing range anchor, movement at a row boundary still collapses a multiple selection, and tab/history outputs expose the retained scroll hint.
- Favorite IDs stay unique after deletion within a session, so a late target probe cannot remove a recreated Item. Settings storage rejects an invalid Favorites hierarchy, Folder Items sort keys are cached once per entry, and undelivered read results count against per-Browser runtime admission.

## 2026-10-01 — P3-M5 complete

- Added Qt-free Browser tab structure, tab-local history and view state, Favorites hierarchy validation and edits, location-shared sorting with direct metadata, tokenized refresh routing, and bounded per-tab reader admission.
- The desktop session now loads and saves settings on its serialized worker, seeds fresh Favorites once, and preserves the current two-Browser window while P3-M6 owns the interactive tab and Sidebar controls. The full automated gate passed.

## 2026-10-01 — P3-M5 Favorite name validation decision

- Favorite Group names and Favorite Item Aliases reject empty or whitespace-only text. The corresponding inline editor remains open and shows an error.

## 2026-10-01 — P3-M5 tab, Favorites, and sort decisions

- Closing a Browser's final tab opens a clean tab at the user's home directory. A newly created Favorite Item uses the active folder's current name as its alias.
- Type, Date, and Size sorts use natural Name ascending as their tie-breaker for both directions. Folder-first ordering, link metadata, and unknown values last remain as already decided.

## 2026-10-01 — P3-M5 planning

- Added the next milestone's implementation plan and unchecked completion checklist. It migrates Browser-wide state and read routing to stable tab identity, connects the P3-M2 settings records to Qt-free Favorites and shared sort state, and reserves the interactive UI, session restore, and native watches for their assigned milestones.

## 2026-10-01 — P3-M4 complete

- The remaining focus and range-anchor decisions are documented with scenario evidence. Every P3-M4 checklist item is complete; active-doc links, `git diff --check`, `make check`, and the direct Cargo format, Clippy, and test commands passed. Implementation remains assigned to P3-M5 through P3-M10.

## 2026-10-01 — P3-M4 workspace interview decisions

- The owner settled the remaining [keyboard-focus and range-anchor rules](../ux-gui.md#planned-phase-3-gui-decisions): focus entering any control in a Browser activates that Browser before Browser-specific commands, and Shift-click or Shift-arrow does nothing until a plain selection establishes an anchor. The [scenario trace](milestone-04-overview.md#scenario-trace) records both cases.
- The owner completed the [selection and file-command contract](../ux-gui.md#selection-and-file-commands): fixed range anchor and cursor behavior, confirmed-folder command availability, context-menu entry points, frozen destructive targets, and a Cancel-default Permanent Delete confirmation. Rename and New Folder reject whitespace-only names.
- The owner corrected Permanent Delete Confirmation to show the frozen target count only, without names; Cancel remains the default and deletion requires explicit activation.
- The owner completed [operation result behavior](../ux-gui.md#operation-panels-and-decisions): reveal panels for long work, decisions, and partial or failed results; hide successful and cleanly cancelled panels after about three seconds; close completed panels with `Command+W`; open Notices for quick failures and default-app opening failures. Tab reordering, Favorite Item movement, and panel docking remain drag-only by owner choice.
- The owner completed [refresh behavior](../ux-gui.md#automatic-refresh): `Command+R` is the sole manual entry point and reserves the `RefreshFolder` action ID; unavailable watched folders keep their last successful view and path with an error; affected open tabs refresh after operations; disappeared Items leave selection and a disappeared cursor clears.
- For keyboard focus, Browser Tabs and Favorite Items use arrow keys and Return under Full Keyboard Access, and text editing suppresses file-command shortcuts. The Favorite Item Context Menu remains pointer-only by owner choice.
- The owner set [tab, history, and selection gestures](../ux-gui.md#browser-tabs-strip-and-navigation), [Favorite controls](../ux-gui.md#sidebar-favorites), [file-command entry points](../ux-gui.md#selection-and-file-commands), and [Operation Panels and Decision Cards](../ux-gui.md#operation-panels-and-decisions). The [product safeguards](../product-behavior.md#file-operation-safeguards) retain conflict choices and define the conflict-only apply-to-all option.
- The owner set [fallback restoration and Notices policy](../product-behavior.md#session-recovery), [Notices controls](../ux-gui.md#notices-and-restoration-feedback), and [inactive-tab refresh feedback](../ux-gui.md#automatic-refresh). [Planned containment](../ux-information-architecture.md#planned-phase-3-containment) places the new components while current 2.0.0 layout and gestures stay separate.
- Text filtering was removed from Phase 3 scope and restored session state; location-shared [sort memory and controls](../ux-gui.md#sort-controls-and-remembered-choice) remain. Completed P3-M2 filter references now carry a supersession note. Existing code and persisted action IDs are unchanged; P3-M5/P3-M9 own later compatibility work.

## 2026-10-01 — Canonical terminology before P3-M4

- Recorded shortcut-capable action names and planned component names in [UX terms](../ux-terms.md). Nine new action IDs are reserved in documentation only; the persisted catalogue and tab behavior are unchanged. Renamed the inert native Browser Tabs Strip widget and its object and accessibility names to match the canonical container term.

## 2026-10-01 — P3-M4 plan review

- Renamed the milestone to cover its full workspace-interaction scope and made its decision questions, owner review, scenario evidence, and downstream handoffs explicit. Added gesture collisions, multi-selection and command targeting, user-visible filter rules, hidden-tab refresh, and operation/session error paths to the plan; no behavior decision was made in this review.

## 2026-10-01 — P3-M4 planning

- Added the documentation-first plan for the next milestone. It will settle the user-facing contracts for tabs, file operations, recovery, sort/filter, refresh, and Sidebar Favorite editing before P3-M5 models the new state.

## 2026-10-01 — P3-M3 complete

- Refactored the native desktop shell into static Sidebar, Main Toolbar, Browser, Browser Tabs Strip, Folder Pane, summary-row, command-row, and coordination components without changing the existing model or command interfaces. The full automated gate passed.
- A native launch was attempted after verification, but visual inspection remains deferred because the environment reported `Cannot create window: no screens available`.

## 2026-10-01 — P3-M3 layout decisions

- The static Standard Layout now has inert Browser Tabs Strip bound only to each current folder name, a bottom-anchored Main Toolbar with disabled Settings, and a three-row Folder Pane. Static Sidebar labels remain disconnected from persisted Favorites.
- Folder Pane Toolbar Row #2 reserves summary metrics for P3-M5/P3-M6. Those milestones will show `N items`, optional `X selected`, and selected or folder totals as non-recursive sums of known direct visible Folder Item sizes; unknown totals render `—`, and metrics stay with visible last-successful items across pending or failed navigation. Row #3 has the functional Up Button followed by disabled Name, Type, Date, and Size ascending/descending carets until P3-M6 connects them.

## 2026-10-01 — P3-M2 complete

- Added Qt-free settings values, sort memory, action catalogue, Favorites records, and workspace-session handoff values; the desktop SQLite driver uses a serialized worker and lossless BLOB locations.
- Added temporary-directory coverage for migrations, unsupported schemas and I/O failure, reset preservation, action preload, non-UTF-8 locations, Favorites ordering, LRU eviction, and asynchronous delivery. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed.

## 2026-10-01 — P3-M2 persistent-storage decisions

- SQLite is the sole live configuration store. The application preloads persistent shortcut bindings for every canonical action, including future inactive actions, from the compiled action catalogue.
- Supported older database schemas upgrade automatically and transactionally. Corrupt, unsupported, or unwritable storage shows a recoverable error with an explicit Reset Settings option; reset is never automatic and preserves the failed database for diagnosis.
- Natural Name ascending is retained as an explicit folder-sort choice and occupies one of the 100 LRU entries. The owner approved `rusqlite` linked to system SQLite.

## 2026-10-01 — P3-M2 plan review

- Separated P3-M2's Qt-free persistence records and settings boundary from P3-M5's future Favorites domain state and edit rules, clarified that the SQLite driver itself has no Qt dependency, and added the remaining owner decisions that must be made before implementation.

## 2026-10-01 — Tabs and Favorites persistence decisions

- The owner added durable open tabs and an ordered, one-level Favorite Group → Favorite Item hierarchy. Tab back/forward history stays session-only. Phase 3 includes Sidebar editing. A saved Favorite Item whose target is missing or unavailable at launch is automatically removed after a completed target probe.
- Fresh-profile Favorites are Applications, Desktop, Documents, Screenshots, and Downloads; Screenshots targets `~/Documents/Screenshots` only when it exists.

## 2026-10-01 — Date-column terminology

- Added Relative Date Column and Exact Date Column to [ux-terms.md](../ux-terms.md#canonical-components) as planned GUI components and aligned the planned GUI description with those exact names.

## 2026-10-01 — Planned GUI owner decisions recorded

- Recorded the owner's planned field order and display, direct sort buttons, responsive field hiding and `Expand` overlay, and future shortcut editor in [ux-gui.md](../ux-gui.md#planned-phase-3-gui-decisions). Current GUI behavior remains described separately.

## 2026-10-01 — P3-M2 scope clarified

- Expanded P3-M2 from an initial shortcut-focused preference plan to Phase 3-wide configuration, defaults, settings, and durable storage. Added a shared, 100-location folder sort history and selected SQLite for settings; sorting controls and session restoration stay in their later milestones.

## 2026-10-01 — P3-M2 planning

- Added P3-M2's plan for application-owned preferences, compiled defaults, asynchronous atomic persistence, canonical UpperCamelCase action IDs, and the future shortcut-editor path. `NewFolder` is reserved with the default `Command+Shift+N`; its command and editor remain later work.

## 2026-10-01 — P3-M1 complete

- P3-M1 is complete with its documentation, terminology, link, and automated verification evidence. The native launch attempt is recorded in its overview; visual inspection remains pending a screen-enabled environment.

## 2026-10-01 — Documentation consistency review

- Separated committed product scope from version 2.0.0 availability, clarified current versus deferred Sidebar and Browser Tabs Strip behavior, and aligned active architecture and repository-layout terminology with Browser and Folder Items.

## 2026-10-01 — P3-M1 terminology baseline

- Product behavior moved out of the former active MVP document; UX vocabulary, information architecture, and GUI behavior now have dedicated active documents.
- Browser and Folder Items are the canonical source and bridge vocabulary. Deferred UI remains unrendered.
