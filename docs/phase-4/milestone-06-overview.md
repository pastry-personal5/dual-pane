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
- Qt-free and native verification.

Out of scope:

- Path completion and suggestions.
- Shell syntax, quoting, escapes, environment variables, and `~name` user expansion. Such text is read literally as a path.
- Network mounting and remote file systems.
- Breadcrumb components and expanded columns, which belong to [P4-M8](milestone-08-overview.md).
- Changing command-line launch resolution, unless the owner extends M6 under proposal 7.

## Fixed rules

P4-M1 already fixes these rules, and this milestone does not restate or change them:

- Path syntax and draft lifetime: the M1 [owner decisions](milestone-01-overview.md#owner-decisions) under "Path syntax and draft".
- The `Command+L` default: the same table, under "Shortcut defaults".
- Safety results for path entry: the path-entry rows of the [policy scenario matrix](milestone-01-architecture.md#policy-scenario-matrix).

## Proposed owner decisions

Nothing in this section is decided. Each row gives a question that P4-M1 [deferred to M6](milestone-01-overview.md#deferred-owner-decisions), or that the current code raises, with options and a recommendation for the owner interview. When the owner answers, follow the P4-M5 precedent:

- Replace this section with an owner-decision record.
- Record the accepted behavior in [UX GUI](../ux-gui.md) and, where it changes product scope, in [product behavior](../product-behavior.md).
- Add a changelog entry.

| # | Question | Options | Recommendation |
|---|---|---|---|
| 1 | Who owns a failed submission? Today a failed navigation sets a listing error on the tab. While that error stands, Copy, Cut, Paste, and file commands are unavailable until a read succeeds, and the Status Bar shows the error instead of the path. | (a) Check the path before the tab navigates. A rejected path changes nothing in the tab, and the error stays with the editor. (b) Submit an ordinary navigation and accept its failed-navigation state. | (a). A typo should not disable the tab's commands. |
| 2 | Where does failure feedback appear? | (a) The Path Edit Control keeps the failed text with an error style. The Browser Status Bar shows the reason until the text changes or is discarded, and the reason is announced. (b) The Status Bar shows the reason for about three seconds, as unavailable commands do. | (a). The reason stays visible while the person corrects the text. |
| 3 | How does editing start? M8 will make clicks on path components navigate. | (a) `Command+L` only. (b) `Command+L` or a click. A click on an inactive Browser's control also activates that Browser. M8 later limits editing clicks to empty space and its Edit control. In both options, the control joins the Full Keyboard Access Tab order. | (b), with all text selected when editing starts. Say now that M8 will narrow the click so the change is not a surprise. |
| 4 | What happens after a successful submission? | (a) Leave editing, show the new confirmed path, and focus that Browser's Folder Items List. (b) Keep focus in the control. | (a). This matches Favorite Item activation. |
| 5 | What do Escape and focus loss do after a failure? M1 says a failed submission keeps its text, while Escape and focus loss discard an unsubmitted draft. | (a) The failed text stays until it is edited, Escape is pressed, or focus leaves. Escape and focus loss then restore the confirmed path, and Escape returns focus to the Folder Items List. Editing the failed text clears the error. (b) The failed text persists until the next successful navigation. | (a). |
| 6 | What happens on a tab or Browser switch? | Failed text: (a) discarded, as an unsubmitted draft is, or (b) kept per tab and restored on return. A check still running: (c) it finishes in its own tab, so success navigates that tab in the background and failure is dropped with the discarded draft, or (d) it is dropped, as link activation is. | (a) and (c). The person committed to the navigation by pressing Return. |
| 7 | How are symbolic links, `..`, and the command line handled? A `Location` is logical: Up from a folder entered through a link returns to the folder that holds the link. | `..`: (a) removes the previous typed or base component (lexical), so `/a/link/..` is `/a`, as with Up. (b) Resolves physically to the target's parent, as the kernel does. Links: a link to a folder anywhere in the path is followed for reading, and the Location keeps the link's name. Command line: `dual-pane <path>` resolves links to a physical path today. Keep that, or align it with the editor in a later change. | (a). Keep link names. Leave the command line unchanged in M6 and record the difference. |
| 8 | What happens when the path names something other than a folder? | File, or a link to a file: (a) refuse as "not a folder", or (b) show its folder and select it, as `dual-pane <file>` does. The second option widens M1's "folder path" wording. Package, such as `.app`, or a link to one: (c) navigate into its contents, as Show Package Contents does, (d) show it selected in its folder, or (e) open it. A dangling link counts as missing. | (b) and (c). Pasting a file path to reach it is common, and entering a package path is an explicit request for its contents. |
| 9 | What special text is accepted? | Empty or whitespace-only Return: (a) acts like Escape, or (b) shows an error. Spaces: always literal, because names can begin or end with spaces. A bare `~` means home, as on the command line. `~name` is an ordinary relative name. Line breaks or control characters: reject as malformed. Unedited Return: rereads the exact location the editor was filled with. | (a), with the listed rules. |
| 10 | How do non-UTF-8 paths behave? The control shows undecodable bytes as U+FFFD. | (a) Return without editing submits the exact Location the editor was filled with. A relative path typed in such a folder uses that folder's exact bytes. Edited text that contains U+FFFD is refused, with a hint to use a relative path, Up, or Back. (b) Refuse every path in such a folder. | (a). APFS refuses such names, so this applies only to other file systems. |
| 11 | What feedback appears while a check runs, and how is it cancelled? | While the path is checked, the control shows the submitted text read-only with a busy cue, and the Status Bar shows “Loading…”. Escape cancels the check and restores the confirmed path. A newer navigation, submission, or tab close supersedes the check. Once the check accepts the folder, editing ends and the ordinary navigation read takes over. If that read then fails, it is an ordinary failed navigation. | As described. |
| 12 | Should `Command+L` work while another text field has focus, such as the Folder Items Filter Row? Since P4-M5, every workspace shortcut except Quit stands aside while a text field has focus. | (a) Keep the rule, so the person presses Escape first. (b) Exempt `EditPath`, as browsers do for `Command+L`. This needs a `ShortcutBinder` exception and a case in the focus script. | (a), for consistency, unless the owner wants browser-like behavior. |

## Completion checklist

- [ ] Before implementation, the owner decides the [proposed owner decisions](#proposed-owner-decisions). Record the accepted behavior in UX GUI, product behavior where scope changes, an owner-decision record in this overview, and the changelog. Update the [architecture plan](milestone-06-architecture.md) wherever an answer differs from its recommendation. Renaming this section breaks links to `#proposed-owner-decisions`, so update those links in [UX GUI](../ux-gui.md) and the architecture plan.
- [ ] Add `EditPath` to the end of `ActionId::ALL`. It is named **Edit Path**, has Window scope and the `Command+L` default, and appears in Keyboard Shortcuts Settings. Older profiles gain it through the existing missing-action completion, which never takes a shortcut an older profile already accepted. Conflict, rebinding, reset, and persistence tests pass. `scripts/check-settings-window.sh` shows the action.
- [ ] `Command+L`, and any approved pointer or Full Keyboard Access entry, starts editing the active Browser's Path Edit Control as approved, without changing Folder Items selection. When editing ends, the control shows only the confirmed path. A confirmed-path update during editing, a pending check, or a failed submission never overwrites the text. Such updates come from Back, a watcher refresh, a launch target, or Reset Settings.
- [ ] Qt-free parsing resolves the following and rejects malformed text with a reason, without I/O:
  - Absolute paths.
  - `~` and `~/` paths, using the workspace's home location rather than `$HOME`.
  - Paths relative to the submitting tab's captured confirmed folder.
  - Root, repeated and trailing slashes, `.`, and `..` under the approved rule.
  - A relative path is refused when the tab has no confirmed folder.
  - Text containing U+FFFD or an unpaired surrogate is refused, never replaced.
- [ ] A worker resolves each component to its stored name without replacing link names. The resolved Location therefore equals the Location reached through Folder Items rows, even when the typed text differs in letter case or Unicode normalization. Temporary-directory tests show the two routes share history, sort memory, and operation refresh.
- [ ] An accepted folder navigates through the ordinary workflow: history, filter clearing, sort memory, watching, and session saving. An unedited or same-location submission is a same-folder reread. Files, packages, and links follow the approved rules.
- [ ] These failures receive distinct feedback in the approved place: malformed, missing, not a folder, permission denied, privacy restricted, busy, and internal. Under the approved failure ownership, a failure leaves the tab's Folder Items, confirmed path, selection, error state, and command availability unchanged.
- [ ] Cancellation is covered. A check is cancelled or superseded by Escape, a newer submission, a navigation the person starts in that tab (a row, Back, Forward, Up, `Command+R`, a Favorite, or a launch target), or closing the tab. Watcher and operation rereads of the current folder do not cancel it. A cancelled or late result never navigates a tab after a newer request and never reaches a different tab. Tab switches follow the approved rule. Busy read capacity reports busy without changing the tab.
- [ ] `scripts/check-text-editor-focus.sh` builds the Path Edit Control. It verifies that while editing, Command+C, Command+X, Command+V, and Command+A edit the text, Folder Items and workspace shortcuts stand aside, Return and Escape reach the editor, and Quit stays bound.
- [ ] The accessible name stays “Path Edit Control”. Editing, pending, and error states are exposed to assistive technology, and errors are announced. Focus returns as approved.
- [ ] Qt-free tests cover parsing, home-relative and relative bases, root, history, stale and cancelled results, and tab switching. Non-UTF-8 coverage uses synthetic byte-exact Locations. Native tests use temporary directories and a disposable profile to cover the following:
  - Unicode normalization variants, and letter-case variants where the temporary volume ignores case.
  - Pressing Return unedited after the confirmed folder changed during editing.
  - A link to a folder, `..` after a link, a dangling link, and a link to a file.
  - A package, a file, an unreadable folder, root, and a slow or cancelled check.
  - Native non-UTF-8 fixtures are not applicable on the APFS temporary volume, which refuses such names (`EILSEQ`). Record that here rather than leaving the case unrun.
- [ ] Update [UX GUI](../ux-gui.md) to replace the Phase 3 read-only Path Edit Control rule. Also update [product behavior](../product-behavior.md), the [UX terms](../ux-terms.md) `EditPath` row, and the README where user-visible behavior changes.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, and so do `make check`, `scripts/check-settings-window.sh`, and `scripts/check-text-editor-focus.sh`. Record evidence here before marking P4-M6 Done.

## Acceptance evidence

Pending M6 owner decisions and implementation.
