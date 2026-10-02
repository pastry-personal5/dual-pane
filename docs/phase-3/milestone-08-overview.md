# P3-M8: Execute file operations and open files

Status: Active

This milestone connects the P3-M7 operation workflow to macOS. It delivers the native scanner and executor, the P3-M7 contract extensions that native work needs, file-command entry points, inline name editors, Operation Panels and Decision Cards, Notices summaries, affected-listing refresh, and default-application opening. [Architecture](milestone-08-architecture.md) describes the approach; this checklist defines completion.

## Scope and fixed inputs

Use the [file-operation safeguards](../product-behavior.md#file-operation-safeguards), [selection and file commands](../ux-gui.md#selection-and-file-commands), [operation panels and decisions](../ux-gui.md#operation-panels-and-decisions), [automatic refresh](../ux-gui.md#automatic-refresh), [file operations and decisions](../architecture.md#43-file-operations-and-decisions), [data-safety invariants](../architecture.md#44-data-safety-invariants), and the [P3-M7 contract](milestone-07-architecture.md). P3-M7's token, decision, and stale-result rules stay as they are. This milestone extends its state machine only as listed under "Application contract extensions"; P3-M7's checklist and tests stay unchanged and green.

In scope: native scan and execution of copy, move, rename, New Folder, Trash, and permanent deletion; no-clobber, replace, and merge semantics; cross-volume move; identity checks and pre-mutation revalidation; folder finalization; cancellation cleanup; a random launch ID and an on-disk safety journal with launch sweep; file-command menu items and focus-scoped shortcuts; the unavailable-shortcut Status Bar reason; the asynchronous Rename Item and New Folder Name editors; the Permanent Delete Confirmation Window; floating Operation Panels, the drag-to-dock Operation Panel Strip, and Operation Decision Cards; operation summaries in Notices; refresh of affected tabs; default-application opening of regular files and macOS packages, and Show Package Contents; the Missing Folder Overlay; and the quit confirmation.

Out of scope: session restoration and the Notices startup preference (P3-M9), native file-system watching and live refresh while a job runs (P3-M10), a shortcut editor, and undo.

## Decisions recorded 2026-10-02

| Topic | Decision |
|---|---|
| Slicing | One milestone with the ordered checklist below. |
| Dependencies | Add `libc` (pinned to the version already in `Cargo.lock`) to the desktop crate for `clonefile`, `copyfile`, `renamex_np`, `unlinkat`, `getentropy`, and related calls. Trash uses Qt `QFile::moveToTrash` through the existing C++ bridge. No other dependency or build tool. |
| Copy fidelity | `clonefile` on the same APFS volume, otherwise `copyfile` preserving data, permissions, timestamps, extended attributes, and resource forks. Links are copied as links. Sockets, devices, and FIFOs raise the standard recoverable error (Try Again/Skip/Cancel). |
| Crash safety | Every copied file or link, and every cross-volume move target, is created under a temporary name in its destination folder and then moved into place with `renamex_np(RENAME_EXCL)`. A crash never leaves a truncated real-looking file. Directories are created directly; an existing directory merges. |
| Replace | A copied replacement is written to a temporary, then moved over the target with plain `rename()`. A same-volume move replacement is a single `rename()` with no temporary. The destination is untouched until the data is complete; nothing is left over afterward. Links and kind mismatches are never replaced. |
| Cross-volume move | Each destination copy completes before its source is removed. A source-removal failure after a successful copy is a recoverable error. The destination copy is kept, and Skip leaves both copies and counts the item as partial. |
| Folder finalization | Permanent deletion and moves remove source folders in a separate post-order finalization phase driven by the application. It runs deepest first, leaves out ancestors of skipped or failed content, and offers Try Again/Skip/Cancel when a removal fails. |
| Journal | A separate SQLite file next to `settings.sqlite3`, owned by one serialized runtime writer. It shares no connection or transaction with settings, and Reset Settings never matches or moves it. It holds one record per operation and destination directory, written before the first temporary there. |
| Temporary names | `.dual-pane-<launch>-<operation>-<n>.partial`, where `<launch>` is a 128-bit random launch ID from `getentropy`. The name positively identifies its creator. Folder Items reads and operation scans hide names that carry the current launch ID. Leftovers from earlier launches stay visible so a person can see and remove them; the owner confirmed this narrowing. |
| Launch sweep | For each record from an earlier launch, delete only regular files or links in the recorded directory whose names match that record's exact launch and operation pattern, never following links or touching directories. Missing directories drop their record. Records on an unreachable volume are kept silently and retried at a later launch. Notices reports removals and failures. |
| Journal unavailable | Copy is disabled, with a refusal reason, and a Journal Unavailable Notice offers Try Again; a successful reopen re-enables Copy. A move that turns out to cross volumes raises a recoverable error at that item, and Try Again reopens the journal. Rename, same-volume move, Trash, deletion, and New Folder need no temporaries and continue. |
| Concurrency | File-operation lane of 2 workers. Runtime leases serialize overlapping write scopes: destinations, plus the source subtrees of move, Trash, and deletion. A step yields after a short time budget so progress flows and jobs share workers fairly. |
| Progress | Entries completed out of planned, plus bytes for the current file. Byte progress is a coalesced event through the application, checked against the step's generation, and stored only for display. The `copyfile` callback also observes cancellation. |
| Reveal delay | A panel appears after about 500 ms, or immediately on a decision, failure, or partial result. |
| Panels | Floating panels plus the drag-to-dock Operation Panel Strip. A finished job is dismissed from application state when its panel hides or closes, or, for a job that finished before its panel was revealed, once its Notice is recorded. Its Notices summary remains. |
| Decision Cards | A card never takes keyboard focus and has no default button. It is announced for accessibility; Replace can never fire by accident. For keyboard access, floating panels are regular windows that join window cycling (``Command+` ``), and docked panels are in the main window's Tab order. |
| Shortcut scope | Copy, Move, Rename, Move to Trash, and Delete Permanently shortcuts act only while the Folder Items List has focus. New Folder stays window-wide and targets the active tab. |
| Inline editors | Commit is asynchronous. A pending editor keeps its text until the job ends. Return or focus loss commits. If a collision or name problem arrives after focus loss, the editor reopens on its row with the typed name and the error. Name problems keep the editor open. Some are caught before a job starts: empty or whitespace-only text, and names the folder-name type rejects, such as a slash or NUL. Others come from the job: collision, too long, or characters the volume rejects. Other failures close the editor and show the job's Decision Card. Committing an unchanged name closes the editor without a job. |
| New Folder editor | A temporary row pinned at the top of the listing with an empty name. The created folder lands at its sorted position and is selected. |
| After edit | A successful New Folder or Rename selects the new Item and moves the cursor to it once the refreshed listing contains it; otherwise the normal refresh rule applies. |
| Trash unavailable | A recoverable error with Try Again/Skip/Cancel; never an offer of, or fallback to, permanent deletion. |
| Renamed or removed folder | No retargeting. The tab keeps its last rows and path under the existing refresh rule. The Missing Folder Overlay (“Command+R to Refresh”) appears for `ItemMissing` and `NotADirectory` until a refresh succeeds, and the narrow-Browser Expand overlay takes precedence. |
| Packages | The directory reader marks bundles on its worker using the reentrant `QFileInfo::isBundle`, so activation and the Context Menu know a package without another request. Activating a package, or a link to one, opens it with its default application; Show Package Contents navigates into it. This widens the decided product scope in [product-behavior.md](../product-behavior.md). |
| Refresh timing | At terminal outcome only. Live updates during a job come with P3-M10. |
| Quit | Command+Q, Dock Quit, and closing the window all arrive as the main window's close event. With running jobs, a confirmation appears. Quit cancels every job, closes delivery, and exits the event loop. The composition root then waits about five seconds for cleanup; later leftovers are swept at the next launch. With no running jobs there is no prompt. |
| Verification | Automated tests first, then a human check at the end for items automation cannot cover. Do not launch the GUI from an agent session. |

## Completion checklist

Ordered by dependency; each later group builds on the earlier ones.

### Application contract extensions

- [x] `OperationErrorKind` gains read-only, privacy-restricted, changed-since-scan, unsupported item, Trash unavailable, source-removal-failed, journal-unavailable, and folder-not-empty kinds, each tested for its permitted choices. A worker panic or lost worker reuses the `OperationExecutorUnavailable` terminal path, renamed if useful, so it never becomes a Skip decision on a root.
- [x] A post-order finalization phase for permanent deletion and moves has its own effect, step results, progress, decisions, cancellation, stale-result, and worker-loss rules (`unavailable()` covers the finalizing state). A directory already gone counts as finalized. Tests cover skipped and failed descendants, removal failure with each choice, and cancellation.
- [x] Name problems extend `NameCollision` to a typed name rejection (collision, too long, rejected characters), which keeps the editor open. The existing pre-job rejections (`InvalidName`, unrepresentable names) are shown the same way. Retrying cancels the rejected job, which closes it without cleanup, and starts a new one.
- [x] A name-less availability query serves menu enablement for each file command; `OperationRejected { reason }` still reports a refused shortcut.
- [x] Terminal outcomes with changes, including cancelled-with-completed-work, partial, and failed-after-progress, request a refresh of every open tab in either Browser whose location is the source or destination folder, or lies at or below a source or destination root.
- [x] A pending post-edit selection `(tab, folder, name)` is applied once when a listing containing the name arrives, then cleared; navigation, a selection change, or a listing without the name also clears it.
- [x] A dismiss command removes a finished job, including one that finished before its panel was revealed, and a test proves running or waiting jobs cannot be dismissed.
- [x] A coalesced byte-progress event is accepted only for the current step's generation and never changes decisions or outcomes.
- [x] Listed entries carry a package flag set by the directory reader on its worker. An open-item request resolves links to regular files, folders, or packages on a worker. The application then asks presentation to open the target on the GUI thread, and a refused open becomes an Open Failed Notice that opens Notices.
- [x] `NoticeKind` variants and presenter wording cover operation summaries, failures and partial results, journal unavailable, sweep reports, and open failures.
- [x] New catalogued actions `CopyToOtherBrowser`, `MoveToOtherBrowser`, `RenameItem`, `MoveToTrash`, `DeletePermanently`, and `ShowPackageContents` have their decided defaults. Settings saved before them still load and receive the defaults.
- [x] A quit request with running jobs yields a confirmation state and a cancel-all path, tested without Qt.
- [x] A journal-status event, sent at launch and after each reopen attempt, drives a new `OperationRejection` reason that disables Copy while the journal is unavailable; the Journal Unavailable Notice offers Try Again.

### Native executor

- [x] `libc` is added as a pinned direct dependency with its license check in the changelog and `AGENTS.md` updated.
- [x] The scanner returns contiguous depth-first plans that record links as links, never traverse them, and map unreadable folders to recoverable scan decisions. Trash scans roots only; Copy, Move, and permanent deletion scan fully. Current-launch temporaries never appear.
- [x] Copy and move preserve metadata as decided, merge directories, and never clobber. A destination `lstat` is only a fast path; `RENAME_EXCL` or `mkdir` is the authority.
- [x] Every file copy and Replace uses the temporary-then-move path. A forced failure mid-copy leaves the existing destination intact and removes only the operation's own temporary.
- [x] A same-volume move of a file, or of a directory whose destination is absent and whose subtree has no scan-skip placeholder, is one `RENAME_EXCL` reported as completed entries. Otherwise entries move one by one and their source folders go through finalization.
- [x] Cross-volume move copies first and removes each source only after its copy completes, with the decided source-removal error.
- [x] Rename and New Folder report collisions and name problems without overwriting. A case-only or normalization-only rename succeeds when `EEXIST` names the same device and inode and the parent directory holds no separate entry with the exact destination bytes. It is decided by file identity, never by comparing names. A hard link with the same inode under a different name is still a collision. Test both.
- [x] Trash moves each root with `QFile::moveToTrash`; the runtime then reports a `LocationInvalidated` for the Trash folder it used. Permanent deletion unlinks files and links during the main pass and leaves directories to finalization.
- [x] Revalidation rejects a target whose identity changed since the scan, and a destination whose device and inode ancestry reaches a source root, catching symlinked and case-variant paths.
- [x] Cancellation finishes the current safe boundary, keeps completed work, removes only this operation's temporaries, and reports uncertain cleanup as such. A cancel runs only after that operation's in-flight step has stopped.
- [x] Native errors map to the application error kinds with operation and item context; raw OS detail stays in logs.
- [x] The file-operation lane runs scan, execute, finalize, and cleanup requests on two workers. It leases write scopes, yields on a time budget, coalesces progress, and never waits for a decision. It contains a panic without replaying the step, then removes that operation's temporaries through its journal records; whatever remains is swept at the next launch.
- [x] The journal writes its per-directory record before the first temporary in that directory, clears it after terminal cleanup, and survives a simulated crash. The sweep follows the decided rules. A test proves Reset Settings never touches the journal file.
- [ ] A temp-directory test settles whether `QFile::moveToTrash` is safe on a worker thread. If it is not, Trash goes through a serialized GUI-thread queue, one root per turn, cancellable between roots.

### Commands and inline editors

- [ ] The Folder Items Context Menu offers Copy to Other Browser, Move to Other Browser, Rename Item, New Folder, Move to Trash, Delete Permanently, and, on a row flagged as a package, Show Package Contents. Enablement comes from the availability query, and disabled items stay disabled.
- [ ] File-command shortcuts act only while the Folder Items List has focus; New Folder is window-wide. A refused shortcut shows its reason in the source Browser Status Bar for about three seconds, then restores the loading, error, or path text.
- [ ] The Rename Item Editor and New Folder Name Editor commit asynchronously, follow the decided focus-loss, reopen, and name-problem rules, suppress workspace shortcuts except `Command+Q` while focused, and close on success. The New Folder Name Editor is an empty temporary row at the top.
- [ ] The Permanent Delete Confirmation Window shows only the frozen target count, defaults to Cancel, and survives tab or selection changes without retargeting.
- [x] Activating exactly one regular file, package, or link to either opens it with the default application; a failure appears in Notices, which opens.
- [ ] Every quit route with running jobs shows the confirmation. Quit cancels with cleanup and a bounded join after the event loop; Cancel keeps the jobs running.

### Operation Panels, decisions, and Notices

- [ ] A job past the reveal delay, or needing a decision, or ending partial or failed, shows its own panel with progress and a Cancel Operation Button. Closing its source tab does not cancel it.
- [ ] Each waiting job shows its own Operation Decision Card with exactly the permitted choices. Conflict-only apply-to-all starts unchecked, concurrent jobs keep independent cards, and a card neither takes focus nor has a default button.
- [ ] Panels can be dragged into the Operation Panel Strip and arrange left to right; docking has no keyboard command.
- [ ] Panel hiding and retention follow the ux-gui rules. `Command+W` closes a completed panel with focus, whether docked or floating, and does nothing on a running or waiting one; elsewhere it keeps its tab behavior.
- [x] Quick successes are recorded in Notices without opening it; quick failures, partial results, and sweep failures open Notices.
- [ ] The Missing Folder Overlay shows centered “Command+R to Refresh” text over dimmed last-good rows for the decided error kinds and disappears after a successful refresh.
- [ ] Panels, cards, editors, the overlay, and the confirmation windows use the [ux-terms](../ux-terms.md) accessible names. Cards are announced; floating panels join window cycling (``Command+` ``), and docked panels are in the main window's Tab order.

### Awaiting the human check

The items below are implemented. Their command, presenter, and bridge logic has automated tests; what remains needs the native interface, which an agent session does not launch. Each stays unchecked until a person verifies it:

- the Folder Items Context Menu and its disabled items
- file-command shortcuts acting only from a focused Folder Items List, and the Status Bar reason clearing after three seconds
- both inline name editors: pending, reopen after focus loss, error text, the pinned New Folder row, and shortcut suppression
- the Permanent Delete Confirmation Window and the Quit Confirmation Window from Command+Q, Dock Quit, and the close button
- floating panels, cards without focus or a default button, VoiceOver announcements, drag-to-dock, Command+W on docked and floating panels, and window cycling
- the Missing Folder Overlay
- real Trash, default-application and package opening, and a volume without `RENAME_EXCL` such as exFAT

The Trash thread-safety test is ignored by default because it moves a file into the real Trash. Run it with `cargo test -p dual-pane-desktop -- --ignored moving_to_the_trash`.

### Evidence and gate

- [x] Native tests run only in `tempfile` directories and cover each executor item above: conflict, merge, link, case-only rename, destination within source through a symlink, cross-volume behavior (injected where a second volume cannot be built), finalization, cancellation, forced failure, panic, and journal crash recovery.
- [x] Runtime, bridge, and presenter tests cover dispatch, stale results, decision round trips, panel and card state, reveal timing, Notices entries, Status Bar expiry, shortcut focus scope, the quit confirmation, and refresh targets.
- [ ] A human check of what automation cannot cover (drag-docking, real Trash, default-application and package opening, card focus and announcements, and visual panel behavior) is recorded after the automated gate, or the item stays unchecked with its blocker.
- [x] Directly affected docs and `AGENTS.md` reflect the executor, dependency, journal, and contract extensions; `docs/README.md` indexes these files.
- [x] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass.
