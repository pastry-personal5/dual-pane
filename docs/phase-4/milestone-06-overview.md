# P4-M6: Navigate by entered path

Status: Planned

This milestone turns the read-only Path Edit Control into a navigation entry point for the active Browser Tab. The [architecture plan](milestone-06-architecture.md) defines parsing, path identity, worker placement, and state handoff. This checklist is the definition of done.

## Scope

In scope:

- The `EditPath` action and its `Command+L` default.
- Entering, editing, submitting, and discarding a path under the [M1 path rules](milestone-01-overview.md#owner-decisions).
- Resolving the entered text to the folder's stored name spelling.
- Validation and pending feedback, focus return, accessibility, and ordinary tab history.
- Behavior when a check is superseded or the person switches tabs.
- Files and packages named by a path (decision 8).
- Qt-free and native verification.

Out of scope:

- Path completion and suggestions.
- Shell syntax, quoting, escapes, environment variables, and `~name` user expansion. Such text is read literally as a path.
- Network mounting and remote file systems.
- `file://` URLs and other URL schemes. Such text is read literally as a relative path.
- Breadcrumb components and expanded columns, which belong to [P4-M8](milestone-08-overview.md).
- Changing command-line launch resolution. It keeps resolving links to a physical path (decision 7).

## Fixed rules

P4-M1 already fixes these rules, and this milestone does not restate or change them:

- Path syntax and draft lifetime: the M1 [owner decisions](milestone-01-overview.md#owner-decisions) under "Path syntax and draft".
- The `Command+L` default: the same table, under "Shortcut defaults".
- Safety results for path entry: the path-entry rows of the [policy scenario matrix](milestone-01-architecture.md#policy-scenario-matrix).

## Owner decisions

The owner answered all twelve questions in the 2026-10-19 interview and accepted every recommendation. Review of the plans the same day added rows 13 and 14, also accepted as recommended. Each row records the accepted behavior. These decisions extend the M1 [owner decisions](milestone-01-overview.md#owner-decisions) and widen its "folder path" wording to files and packages (row 8).

| # | Topic | Decision |
|---|---|---|
| 1 | Failed submission | The entered path is checked before the tab navigates. A rejected path changes nothing in the tab, and the error stays with the editor. A typo never disables Copy, Cut, Paste, or file commands. |
| 2 | Failure feedback | The Path Edit Control keeps the failed text with an error style. The Browser Status Bar shows the reason until the text changes or is discarded, and the reason is announced. |
| 3 | Starting to edit | `Command+L` or a click starts editing and selects all text. A click on an inactive Browser's control also activates that Browser. The control joins the Full Keyboard Access Tab order. P4-M8 later limits editing clicks to empty space and its Edit control. |
| 4 | After success | Editing ends, the control shows the new confirmed path, and focus moves to that Browser's Folder Items List, as Favorite Item activation does. |
| 5 | Escape and focus loss after failure | The failed text stays until it is edited, Escape is pressed, or focus leaves. Escape and focus loss then restore the confirmed path, and Escape also returns focus to the Folder Items List. Editing the failed text clears the error. |
| 6 | Tab or Browser switch | Failed text is discarded, as an unsubmitted draft is. A check still running finishes in its own tab: success navigates that tab in the background, and failure is dropped with the discarded draft. |
| 7 | `..`, links, and command line | `..` is lexical: it removes the previous typed or base component, so `/a/link/..` is `/a`, as Up does. It does not check that the removed component exists, so `/a/missing/..` is `/a`. A link to a folder is followed for reading, and the Location keeps the link's name. `dual-pane <path>` keeps resolving links to a physical path in M6, and the difference is recorded in UX GUI. |
| 8 | Files and packages | A path to a file, or to a link to a file, shows the file's folder with the file selected, as `dual-pane <file>` does. A path to a package such as `.app`, or to a link to one, navigates into its contents, as Show Package Contents does. A dangling link counts as missing. |
| 9 | Special text | Empty text (zero characters) on Return acts like Escape. Whitespace-only text is parsed as a literal relative name, because spaces are never trimmed and a folder may be named with spaces. A bare `~` means home. `~name` is an ordinary relative name. Line breaks and control characters are rejected as malformed. Unedited Return rereads the exact location the editor was filled with. |
| 10 | Non-UTF-8 paths | Unedited Return submits the exact Location the editor was filled with. A relative path typed in such a folder uses that folder's exact bytes. Edited text that contains U+FFFD is refused, with a hint to use a relative path, Up, or Back. |
| 11 | Pending check | While the path is checked, the control shows the submitted text read-only with a busy cue, and the Status Bar shows "Loading…". Escape cancels the check and restores the confirmed path. A newer navigation, submission, or tab close supersedes the check. Once the check accepts the folder, editing ends and the ordinary navigation read takes over. If that read fails, it is an ordinary failed navigation. |
| 12 | `Command+L` in a text field | The P4-M5 rule stands: every workspace shortcut except Quit stands aside while a text field has focus, including the Path Edit Control itself. The person presses Escape first. `ShortcutBinder` needs no exception. |
| 13 | Filter hiding a file target | A file path in the tab's current folder clears the tab's Folder Items Filter when the filter hides that file, so the file is visibly selected. This is an exception to the same-folder-reread rule that keeps a filter. A filter that does not hide the file stays. |
| 14 | Hidden file target | Item Visibility settings apply to the selection. A folder or package opens whatever its visibility, because the person named it. A file hidden by the Item Visibility settings opens in its folder with nothing selected, and the settings are not changed. |

The accepted behavior is recorded in [UX GUI](../ux-gui.md), [product behavior](../product-behavior.md), and the [changelog](changelog.md).

## Completion checklist

- [x] The owner decided the [owner decisions](#owner-decisions) on 2026-10-19. The record is in this overview, the architecture plan follows it, and the links to the renamed section are updated.
- [ ] Add `EditPath` to the end of `ActionId::ALL`. It is named **Edit Path**, has Window scope and the `Command+L` default, and appears in Keyboard Shortcuts Settings. Older profiles gain it through the existing missing-action completion, which never takes a shortcut an older profile already accepted. Conflict, rebinding, reset, and persistence tests pass. `scripts/check-settings-window.sh` shows the action.
- [ ] `EditPath` is available whenever the active Browser can navigate, including while its tab is loading, shows an error, or shows the Missing Folder Overlay. Typing an absolute path is the way out of such a state. It is unavailable wherever row activation and Back are, as the existing availability code decides.
- [ ] `Command+L`, a click on the Path Edit Control, and Full Keyboard Access traversal start editing the active Browser's Path Edit Control and select all its text, without changing Folder Items selection. A click on an inactive Browser's control also activates that Browser. `Command+L` does nothing while any text field, including the control itself, has focus (decision 12). When editing ends, the control shows only the confirmed path. A confirmed-path update during editing, a pending check, or a failed submission never overwrites the text. Such updates come from Back, a watcher refresh, a launch target, or Reset Settings.
- [ ] Qt-free parsing resolves the following and rejects malformed text with a reason, without I/O:
  - Absolute paths.
  - `~` and `~/` paths, using the workspace's home location rather than `$HOME`.
  - Paths relative to the submitting tab's captured confirmed folder.
  - Root, repeated and trailing slashes, `.`, and lexical `..` (`/a/missing/..` is `/a`; `..` at root stays at root).
  - Empty text, which the desktop treats as Escape and never submits. Whitespace-only text is a literal relative name.
  - A relative path is refused when the tab has no confirmed folder.
  - Text containing U+FFFD or an unpaired surrogate is refused, never replaced.
- [ ] A worker resolves each component to its stored name without replacing link names. The resolved Location therefore equals the Location reached through Folder Items rows, even when the typed text differs in letter case or Unicode normalization. Temporary-directory tests show the two routes share history, sort memory, and operation refresh.
- [ ] A file target hidden by the filter clears it, and a file target hidden by Item Visibility selects nothing (decisions 13 and 14). Tests cover both.
- [ ] An accepted folder navigates through the ordinary workflow: history, filter clearing, sort memory, watching, and session saving. An unedited or same-location submission is a same-folder reread. A file, or a link to one, opens its folder with the file selected. A package, or a link to one, opens its contents. A link to a folder keeps the link's name in the Location.
- [ ] The resolver maps its errors onto the existing listing error kinds as the [architecture plan](milestone-06-architecture.md#application-state-and-handoff) states. These failures receive distinct feedback in the control's error style, the Status Bar, and an announcement: malformed, missing, not a folder, permission denied, privacy restricted, busy, and internal. A failure leaves the tab's Folder Items, confirmed path, selection, error state, and command availability unchanged.
- [ ] Cancellation is covered. A check is cancelled or superseded by Escape, a newer submission, a navigation the person starts in that tab (a row, Back, Forward, Up, `Command+R`, a Favorite, or a launch target), or closing the tab. Watcher and operation rereads of the current folder do not cancel it. A cancelled or late result never navigates a tab after a newer request and never reaches a different tab. A tab or Browser switch discards failed text and lets a running check finish in its own tab (decision 6). A late success navigates that tab in the background, and neither it nor a late failure reaches another tab's editor. Busy read capacity reports busy without changing the tab.
- [ ] `scripts/check-text-editor-focus.sh` builds the Path Edit Control. It verifies that while editing, Command+L does nothing, Command+C, Command+X, Command+V, and Command+A edit the text, Folder Items and workspace shortcuts stand aside, Return and Escape reach the editor, and Quit stays bound.
- [ ] While a check runs, the control shows the submitted text read-only with a busy cue, and the Status Bar shows "Loading…" (decision 11). Escape, a newer submission, or a navigation the person starts in that tab ends the pending state and restores the confirmed path. A check that accepts the folder ends editing before the ordinary read takes over. Tests cover each exit.
- [ ] The accessible name stays “Path Edit Control”. Editing, pending, and error states are exposed to assistive technology, and errors are announced. Focus moves to the Folder Items List after a successful submission and after Escape (decisions 4 and 5).
- [ ] Qt-free tests cover parsing, home-relative and relative bases, root, history, stale and cancelled results, and tab switching. Non-UTF-8 coverage uses synthetic byte-exact Locations. Native tests use temporary directories and a disposable profile to cover the following:
  - Unicode normalization variants, and letter-case variants where the temporary volume ignores case.
  - Pressing Return unedited after the confirmed folder changed during editing.
  - Submitting an absolute path from a tab that shows an error or the Missing Folder Overlay, and a relative path from the same tab, which is refused as having no confirmed folder.
  - A link loop, which reports missing.
  - A link to a folder, `..` after a link, `..` after a missing component, a dangling link, and a link to a file.
  - Escape during a pending check, and a late result after a tab switch.
  - A package, a file, an unreadable folder, root, and a slow or cancelled check.
  - Native non-UTF-8 fixtures are not applicable on the APFS temporary volume, which refuses such names (`EILSEQ`). Record that here rather than leaving the case unrun.
- [x] The accepted [owner decisions](#owner-decisions) are recorded in [UX GUI](../ux-gui.md), in [product behavior](../product-behavior.md) where scope widens, and in the [changelog](changelog.md), all dated 2026-10-19.
- [ ] With the implementation, replace the released-version read-only Path Edit Control note in [UX GUI](../ux-gui.md), and update the [UX terms](../ux-terms.md) `EditPath` row and the README where user-visible behavior changes.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, and so do `make check`, `scripts/check-settings-window.sh`, and `scripts/check-text-editor-focus.sh`. Record evidence here before marking P4-M6 Done.

## Acceptance evidence

Pending implementation.
