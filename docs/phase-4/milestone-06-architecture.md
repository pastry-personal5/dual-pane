# P4-M6 architecture

Status: Done

Path entry becomes a submission addressed to one tab: the application parses it without I/O, a per-Browser read lane resolves it to the folder's stored spelling, and the ordinary navigation workflow commits it. The [overview checklist](milestone-06-overview.md#completion-checklist) defines acceptance. This plan implements the [owner decisions](milestone-06-overview.md#owner-decisions), cited below as decisions 1 to 14.

## Current state

- **The control.** The Path Edit Control is a read-only `QLineEdit` built in `Browser` in `crates/dual-pane-desktop/cpp/include/dual_pane_desktop/folder_pane.hpp`. It has `Qt::NoFocus` and ignores mouse events. Every `pathTextChanged` signal calls `setText`, which would overwrite a draft.
- **Displayed text.** `format::location_text` renders a `Location` lossily: undecodable bytes appear as U+FFFD, so the text cannot round-trip.
- **Navigation.** `Command::Navigate { browser, location }` always targets the Browser's active tab. `start_read` clears the tab's error and records the requested location. A failed read sets `TabState::error`, and checks for that error disable clipboard capture, Paste, and operation commands until a read succeeds.
- **Read lanes.** Listing reads run on each Browser's supervised directory-read lanes. These lanes provide bounded admission, which reports `Busy`, and token cancellation, as described in [architecture 5.2](../architecture.md#52-planned-desktop-execution-model).
- **Probe lane.** One serial lane runs Favorite and Screenshots probes, link resolution (`ResolveItem`), and Finder Paste preflight.
- **Path conversion.** `native_location::location_from_path` refuses `.` and `..` components. `launch_target.rs` resolves command-line paths with `fs::canonicalize`, which follows links to a physical path.
- **Shortcuts.** Since P4-M5, `ShortcutBinder` disables every workspace shortcut except Quit while any `QLineEdit` has focus.
- **Older profiles.** New action IDs reach them through `complete_bindings`, as `FocusFolderItemsFilter` and the clipboard actions did. No schema migration is needed.

## Affected surfaces

| Surface | M6 responsibility |
|---|---|
| `crates/dual-pane-application/src/path_entry.rs` (new) and `tests/path_entry.rs` | Pure parsing and lexical normalization into a candidate `Location`, with typed rejection reasons. |
| `crates/dual-pane-application/src/input.rs`, `output.rs`, `work_request.rs`, `workspace.rs` | Submission command, per-tab pending check, resolve request and result, stale guards, and handoff to `start_read`. |
| `crates/dual-pane-application/src/settings.rs` | Append `EditPath` with Window scope and the `Command+L` default. |
| `crates/dual-pane-adapters/src/browser_presenter.rs` | Path-editor view state and reason wording. |
| `crates/dual-pane-desktop/src/native_calls.rs` and a resolver module beside `location_probe.rs` | Walk components without following the final link, read stored names, and classify the target. |
| `crates/dual-pane-desktop/src/runtime.rs` | Run resolve jobs on the submitting Browser's read lanes, with admission, cancellation, and supervision. |
| `crates/dual-pane-desktop/src/folder_items_list_model.rs` and `browser_session.rs` | Path-editor properties and a dedicated submit method, not a copyable `UiEvent` payload. Submissions from a tab that is no longer shown are rejected. |
| `crates/dual-pane-desktop/src/settings_storage.rs` and `workspace_bridge.rs` | Persist and present `EditPath`. |
| `crates/dual-pane-desktop/cpp/include/dual_pane_desktop/folder_pane.hpp` | Display and edit modes, focus, guards, error style, accessibility, and the `EditPath` handler. |
| `scripts/check-text-editor-focus.sh` and `scripts/check-settings-window.sh` | Native focus and catalogue evidence. |

M6 adds no dependency or build tool.

## Parsing without I/O

`path_entry.rs` takes three inputs: the submitted text, the submitting tab's confirmed folder captured at submission (`Option<Location>`), and the workspace's `home` location. That home is the disposable profile's home in debug test runs. Parsing applies these rules:

- Text that starts with `/` is absolute.
- `~` alone, or text that starts with `~/`, is home-relative.
- Any other text, including `~name`, is relative to the captured folder.
- A relative path with no confirmed folder is refused.
- Empty components from repeated or trailing slashes are dropped, `.` is removed, and `..` removes the previous component (decision 7). The removal is lexical and never checks that the component exists, so `/a/missing/..` is `/a`. `..` at root stays at root.
- Every remaining component must form a valid `EntryName`.
- Line breaks and other control characters are refused as malformed.
- Text containing U+FFFD is refused, because it shows undecodable bytes and does not name them.
- Spaces are literal and are never trimmed, so whitespace-only text is a relative name. Only zero-length text acts as Escape, and the desktop handles that before calling the parser (decision 9).

Parsing joins components onto the captured base by their exact bytes, so a relative path typed in a non-UTF-8 folder keeps that folder's bytes.

A submission is either `Typed(text)` or `Unchanged(location)`. When editing starts, the desktop captures the observed tab and the exact `Location` whose text fills the editor. It sends `Unchanged` with that `Location` when the person presses Return without editing. The application rereads that location and never reparses the lossy display text. The captured location matters because the text guard keeps the prefilled text on screen even if the confirmed folder changes during editing, for example when a Favorite's read finishes. Relative `Typed` text still resolves against the folder confirmed at submission, as M1 requires. The desktop converts `QString` to `String` with a strict UTF-16 decode. An unpaired surrogate is reported as malformed, never replaced.

## Path identity: stored spelling

APFS looks up names without regard to Unicode normalization, and default APFS volumes also ignore letter case. Each name is still stored and listed with one spelling. A typed `/USERS` or a decomposed `é` opens the same folder that Folder Items reaches through its rows. A `Location` built from typed bytes would still be a different value. Each of these consumers compares `Location` values byte for byte, so each would split or miss the folder:

- Per-folder sort memory (`SettingsState::sort_for`).
- `TabHistory::arrive` and Back or Forward state restoration.
- Operation refresh matching (`refresh_location`).
- Same-folder Paste rejection and Copy/Move to Other Browser disablement.
- Pending-Cut cue matching.
- Launch-target tab reuse.
- Location watchers and Favorite Items created from the tab.

The resolver therefore rebuilds the `Location` from stored names. Starting at root, it does the following for each component:

1. Look the component up relative to the already-resolved parent without following a final link.
2. Read the component's stored name, for example with `getattrlist` and `ATTR_CMN_NAME` using `FSOPT_NOFOLLOW`, and substitute those bytes.
3. If the component is a link, keep the link's own name in the `Location` and follow the link only to continue the walk. The walk follows at most the kernel's link limit. A link loop (`ELOOP`) reports the same reason as a dangling link, missing.
4. Check the job's cancellation token between components, so a walk that stalls on a slow volume stops promptly.

The `getattrlist` name call is an assumption until step 3 of the sequencing proves it on the temporary volume: it must return the stored spelling for a case variant and for a decomposed or precomposed variant. If it does not, the resolver reads the parent's entries instead, and this section changes before the resolver is built.

The resolver then classifies the target as folder, package, regular file, or a link to one of these, or as missing, not a folder, permission denied, or privacy restricted. It reuses the listing reader's error mapping. Libc calls stay in `native_calls.rs`. The resolver never uses `fs::canonicalize`, because that would replace link names with physical targets and break the logical model behind Up and Back.

## Worker placement

Architecture 5.2 places navigation reads on the foreground blocking-I/O lane. The resolve job therefore runs on the submitting Browser's directory-read lanes and shares their admission capacity, `Busy` result, token cancellation, and supervision.

The shared probe lane was rejected. It is serial, so a submission that stalls on an unresponsive volume would also block link activation, Favorite probes, and Finder Paste preflight.

Making the navigation read itself the check was also rejected. The stored-spelling `Location` must exist before the read starts. Folding editor-local failure into `PendingRead` would also tangle the editor with its restore, dirty-retry, and visibility-retry logic.

The two-step design costs one metadata walk before the read. If the folder disappears between resolution and the read, the read fails as an ordinary failed navigation.

## Application state and handoff

Exact names may change during implementation, but these contracts hold:

1. **Submission.** `Command::SubmitPath { browser, tab, submission }` names the tab the editor observed. The workspace rejects the submission if that tab no longer exists or is not the Browser's active tab. It captures the tab's confirmed folder from its current Folder Items and parses the text. A parse failure produces `Output::PathEntryRejected` at once. Otherwise the workspace allocates a `RequestToken`, stores it as the tab's pending path check, and emits `WorkRequest::ResolvePath { browser, tab, token, candidate }`.
2. **Superseding.** These clear the pending check and ask the runtime to cancel its job:
   - A newer submission for the tab.
   - A navigation the person starts in the tab: a row, Back, Forward, Up, `Command+R`, a Favorite, or a launch target.
   - Closing the tab.
   - `Command::CancelPathEntry` (Escape).

   Watcher, operation, and visibility rereads of the current folder also call `start_read`, but they do not cancel the check. Otherwise a busy folder could keep rejecting submissions. A tab or Browser switch leaves the check running (decision 6). Its eventual failure is dropped because the draft was discarded.

   Superseding must also end the editor's pending state, because a toolbar, Sidebar, or Browser Tabs click can start a navigation while the read-only control still shows the submitted text. A navigation the person starts and `CancelPathEntry` (Escape) therefore emit `Output::PathEntryCancelled { browser, tab }`, and the desktop restores display mode with the confirmed path. A newer submission replaces the check and keeps the editor pending with the new text, and closing the tab removes the editor, so neither emits it.
3. **Result.** `Event::PathResolved { browser, tab, token, result }` is accepted only while its token is still the tab's pending check.
   - A folder, a link to a folder, or a package (decision 8) calls `start_read` with the resolved `Location`. History, filter clearing, sort memory, watching, and session saving then follow the ordinary path.
   - A file, or a link to one (decision 8), reads the file's folder. The workspace sets `pending_selection` before it calls `start_read`, as the launch target does. That slot is one workspace-wide value tagged with a Browser and tab, so a newer writer replaces an older one, and a background result may replace the slot of another tab's unfinished selection. Accept that, because a pending selection is only a convenience, and cover it with a test. `start_read` already clears a selection whose folder differs, and the selection applies only if the read succeeds. If the file is gone by then, or Item Visibility hides it, the folder opens with nothing selected (decision 14). When the target is in the tab's current folder and the Folder Items Filter hides it, the workspace clears that tab's filter first (decision 13), because a same-folder reread would otherwise keep it.
   - A rejection emits `Output::PathEntryRejected { browser, tab, reason }`. The tab's `folder_items`, `error`, `requested`, history, and selection are left untouched, so command availability does not change.
4. **Acceptance.** `Output::PathEntryAccepted { browser, tab }` ends editing. The desktop ignores `PathEntryAccepted`, `PathEntryRejected`, and `PathEntryCancelled` unless that tab's editor is the one showing, so a background result never ends or marks another tab's editor. From then on the ordinary loading, success, and failure outputs apply.

`PathEntryPending` and the reason enum reuse `ListingErrorKind` categories where they match, and add `Malformed`, `NoConfirmedFolder`, and `UndecodableText` as needed. The resolver maps `ELOOP`, `ENOENT`, and a dangling link to `ItemMissing`, a non-folder final component to `NotADirectory`, `EACCES` to `PermissionDenied`, a privacy denial to `PrivacyRestricted`, admission overflow to `Busy`, and every other errno to `Internal`. `VisibilityClassificationFailed` and `Unknown` never come from the resolver, because it does not classify visibility. The presenter keeps a per-Browser `PathEditorView` with the pending flag, reason text, and a revision for the shown tab. Reasons reuse the wording of the existing listing-error messages, with the entered path as the subject.

## Desktop composition

- **Modes.** The Path Edit Control has a display mode and an edit mode. Focus always enters edit mode, so a control that is not editing never holds focus. That way the text-field shortcut rule never suppresses workspace shortcuts for a control in display mode.
- **Entering edit mode.** `Command+L` through the `EditPath` handler, a click, or Full Keyboard Access traversal enters edit mode for the active Browser and selects all text. The existing focus-enter rule activates an inactive Browser first.
- **Keys.** Return submits `Typed` or `Unchanged`, captured with the observed tab. While editing, the rename shortcut is not active because Folder Items List shortcuts are widget-scoped. Escape cancels a pending check or discards the draft or failed text, then focuses the Folder Items List. Focus loss and tab switches discard failed text (decisions 5 and 6). After a successful submission, focus moves to the Folder Items List (decision 4).
- **Text sync guard.** Only display mode applies `pathText`, with signals blocked. Edit mode, pending checks, and failed text keep their own text until they end, so Back, a watcher refresh, a launch target, or Reset Settings cannot overwrite them.
- **Pending and error states.** While pending, the control is read-only with a busy cue, and `PathEntryCancelled` returns it to display mode. A failed submission keeps its text, which stays until it is edited, Escape is pressed, or focus leaves (decision 5), and sets a `pathError` dynamic property for the stylesheet. The reason is exposed through the accessible description, the Browser Status Bar, and an accessibility announcement. The accessible name stays “Path Edit Control”.
- **Shortcut gating.** No new `ShortcutBinder` rule is needed (decision 12). The P4-M5 text-field rule covers edit mode, and `scripts/check-text-editor-focus.sh` gains the control as evidence. `Command+L` does nothing while any text field has focus, including the control itself.

## Compatibility with P4-M8

Keep the edit mode and the `SubmitPath` contract independent of how display mode renders. P4-M8 can then replace display mode with breadcrumb components and an Edit control without changing submission, stale guards, or feedback. Breadcrumb and column navigation submit exact `Location` values through `Navigate` rather than text, so they never pass through parsing or U+FFFD rejection.

## Verification and sequencing

Implement the milestone in this order:

1. Parser and its Qt-free tests.
2. Submission state machine and stale, cancel, and tab-switch tests.
3. Native resolver and its temporary-directory tests. The first test proves the stored-name call described in "Path identity".
4. Runtime lane wiring and `Busy` and cancel tests.
5. Presenter and model state.
6. Desktop modes and guards.
7. `EditPath` catalogue and persistence.
8. Focus and Settings scripts, then documentation and the full gate.

Each native step thus builds on a tested application contract.

Required scenarios:

- A delayed resolve result after a newer Back, Favorite, or submission changes nothing.
- A result after its tab closed is ignored.
- A navigation started by a toolbar or Sidebar click during a pending check restores the editor's display mode with the confirmed path.
- A background file result replaces the single `pending_selection` slot, and the tab it belongs to selects the file once its read succeeds.
- A result for an inactive tab navigates that tab in the background, and a failure is dropped. Neither touches the shown tab's editor (decision 6).
- A relative path submitted while another read loads resolves against the confirmed folder, and the later submission wins.
- A watcher refresh during a check does not cancel it.
- Typed normalization variants produce the row-reached `Location`, the same sort memory and history entry, and operation refresh. Case variants do too, where the temporary volume ignores case. On a case-sensitive volume, the test checks that a case variant reports missing.
- Press `Command+L` while a Favorite's read is loading, let the read finish, then press Return unedited. The submission rereads the location shown in the editor, not the newly confirmed folder.
- `..` after a link returns to the link's parent folder.
- A dangling link reports missing.
- A folder with permissions `000` reports permission denied without changing the tab.
- An unedited submission in a synthetic non-UTF-8 folder rereads the exact bytes, and edited U+FFFD text is refused.
- A `Busy` admission leaves the tab unchanged.
- A tab in an error state or showing the Missing Folder Overlay accepts an absolute path, and a relative path is refused with `NoConfirmedFolder`.
- A link loop reports missing, and a cancelled walk stops between components.
- Escape during a pending check restores the confirmed path and cancels the job.
- `/a/missing/..` parses to `/a`.
- Whitespace-only text is parsed as a relative name, and empty text never reaches the parser.
- A file path opens its folder with the file selected, and a package path opens its contents.
- A file hidden by the filter clears the filter and is selected. A file hidden by Item Visibility opens its folder with nothing selected, and a hidden folder opens.
