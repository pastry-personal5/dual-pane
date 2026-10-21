# P4-M3: Deliver text filtering UI

Status: Active

This milestone makes the tab-local prefix filter usable in each Browser. The [architecture plan](milestone-03-architecture.md) defines the desktop wiring; this checklist is the definition of done.

## Scope

In scope: an accessible temporary Folder Items Filter Row, direct printable-key entry from a focused Folder Items List, prefix matching, the Close Filter Button, empty-result feedback, focus and shortcut behavior, view-model updates, and native verification.

Out of scope: recursive search, file content search, and filter persistence beyond the M1 decision.

## Owner decisions

The owner accepted these M3 behaviors on 2026-10-17. These decisions supersede the earlier M1 deferral.

| Topic | Accepted behavior |
|---|---|
| Canonical component | The temporary row is the **Folder Items Filter Row**. Its far-right dismissal control is the **Close Filter Button**. |
| Placement and lifetime | The row appears immediately below Folder Pane Toolbar Row #3 when direct printable input starts filtering. It remains visible while the tab has a nonempty query. `Command+F` also shows and focuses it with an empty query. An empty row hides on focus loss or tab switch; preserve the newly chosen focus. Explicit close hides it. |
| Query presentation and continuation | The row displays `Starts with: {query}`. Continued printable input appends once to the existing query, so `a` followed by `p` displays `Starts with: ap`. |
| Matching override | Match folders and files whose displayed names start with the query. This M3 prefix rule supersedes M2's substring matcher while retaining its Unicode, whitespace, invalid-UTF-8 identity, concurrency, and reconciliation rules. |
| Close behavior | Activating the Close Filter Button or pressing Escape clears the query, restores the full Folder Items List, hides the row, and returns focus to that Browser's Folder Items List. |
| Composed text | IME preedit remains in the text editor; only committed text changes the query, exactly once. |
| No matches | Leave the Folder Items List visually empty, without a no-match row, message, or overlay inside it. Keep the Folder Items Filter Row visible and expose zero-result count and accessibility feedback. |

## Completion checklist

- [x] Before implementation, the owner decides the remaining [P4-M3 type-to-filter details](milestone-01-overview.md#deferred-owner-decisions), including composed-text commitment and exact focus destinations; record the decisions in [UX GUI](../ux-gui.md) and [UX information architecture](../ux-information-architecture.md).
- [ ] Each Browser displays its temporary Folder Items Filter Row immediately below Folder Pane Toolbar Row #3 while its active tab has a nonempty query; changing tabs restores or hides the row with the correct tab's text and view without briefly showing another tab's state.
- [x] With a Folder Items List focused, an unmodified printable character other than Space starts filtering that Browser Tab, shows `Starts with: {character}`, and moves focus to the row's text input with the character entered once. Further committed printable input appends once, including IME committed text; preedit remains in the text editor without changing the projection. The other Browser and tabs are unchanged.
- [ ] Prefix matching replaces M2's substring match while retaining M2's normalization and safety rules. Typing and clearing update rows and count feedback. While projection work runs, the list is blank with an updating cue and Item actions blocked, without erasing visit state.
- [x] If no folder or file starts with the query, the Folder Items List is visually empty with no no-match row, message, or overlay inside it. The visible Folder Items Filter Row, zero-result count, and accessibility feedback distinguish the result from an empty folder, loading, a projection failure, and a read error.
- [x] The far-right Close Filter Button and Escape clear the active tab's query, restore its complete current listing, hide the Folder Items Filter Row, and return focus to that Browser's Folder Items List. `Command+F` shows and focuses an empty row; focus loss or switching tabs hides an empty row without moving focus, while a nonempty row stays with its tab.
- [x] Add the stable `FocusFolderItemsFilter` action at the end of the persisted action catalogue with `Command+F` as its default and Window scope. It appears as **Focus Folder Items Filter** in Keyboard Shortcuts Settings, follows the existing conflict and profile-compatibility rules, and shows and focuses the active Browser's Folder Items Filter Row even when its query is empty.
- [ ] Keyboard entry, Escape or clear behavior, focus return, context menus, Quick Look, and inline name editing do not steal or misroute filter keystrokes. Focused filter text editing wins over workspace shortcuts while preserving ordinary text-editor commands.
- [x] Every row or selection-dependent native gesture carries the tab and visible-row revision observed when the widget receives the gesture; row gestures also preserve the exact byte-backed Item identity. A delayed gesture from before a filter result, tab switch, sort, refresh, or clear is rejected rather than retargeted.
- [ ] User-originated filter edits and Close Filter Button activation carry the tab shown when the signal originated. Rendering application state, restoring another tab's query, or hiding the row cannot emit a user edit or overwrite another tab's filter.
- [ ] A failed or cancelled projection leaves the Folder Items List blank, keeps Item actions blocked, and provides retryable status and accessibility feedback distinct from a successful zero-result projection and a directory-read error. Editing or clearing retries through the existing M2 contract.
- [x] Accessible names for the Folder Items Filter Row and Close Filter Button, the query value, result count, and zero-result feedback are available to assistive technology, and the row remains usable at supported Browser widths.
- [ ] Native macOS checks cover direct `a` entry from Folder Items, continued `p` entry producing `Starts with: ap`, prefix results, composed text entry, `Command+F`, shortcut rebinding and conflict rejection, Close Filter Button clearing, an empty no-match list, projection failure and retry, delayed stale gestures, rapid tab switching, both Browsers, refresh, sort, selection, and reserved Space/navigation/shortcut behavior in a disposable profile; record the result here.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M3 Done.

## Acceptance evidence

Rust formatting, Clippy, and workspace tests pass; `make check` passes the Rust and C++ formatters, both linters, and all tests. `scripts/check-settings-window.sh`, `scripts/check-text-editor-focus.sh`, and `scripts/check-application-title-row.sh` pass with Qt's offscreen platform. The text editor focus check now also sends `QInputMethodEvent` composed text: a Folder Items List hands preedit and commit to its handler, and the filter field changes its text once, on commit only.

On 2026-10-18 and 2026-10-19 a debug build in a disposable profile was driven through macOS System Events and CoreGraphics mouse events (an agent-driven native run, not an automated test) over `apple_dir`, `banana_dir`, `apple.txt`, `apricot.txt`, `Banana.txt`, `cherry.txt`, and later `avocado.txt`, `élan.txt`, `eagle.txt`. Observed in the Left Browser, with the Right Browser unchanged throughout:

- Direct `a` from the focused Folder Items List showed `Starts with: a` with 3 matches; continued `p` showed `Starts with: ap`; a further `a` from the list appended (`aa`); `zz` showed `No matches` with an empty list, `0 of 7 Items`, and the row still visible. `e` matched both `élan.txt` and `eagle.txt`.
- Escape in the filter field restored all 7 Items, hid the row, and returned focus to the list: a following `b` started a new filter with 2 case-insensitive matches. The Close Filter Button click cleared and hid the row the same way.
- `Command+F` showed and focused an empty row (placeholder, `7 items`); Tab out of it, and switching tabs, hid the empty row and left the full list. `Command+T` stood aside while the filter field had focus.
- Per tab: a filter on tab 2 was restored on returning to it, tabs 1 and 3 showed no row, and eight rapid switches ended on the right tab, text, and rows.
- Selecting `Banana.txt` and then filtering by `a` dropped it from the selection (`0 selected`, `3 of 7 Items`); a selected visible Item stayed selected through a sort and through a refresh that added `avocado.txt` (`4 of 8 Items`), and the filter stayed.
- Space opened Quick Look for the cursor row and typed nothing into the filter; Down moved the cursor.
- Keyboard Shortcuts Settings lists **Focus Folder Items Filter** with `Command+F` and Workspace window scope. Adding `Command+G` worked and focused the row; adding `Command+T` was rejected with `Already assigned to New Tab.`.
- The accessibility tree exposes `Folder Items Filter Text` (description `Starts with a. 3 matches`), `Filter Results`, and `Close Filter Button`. At the minimum window width (963 px) the row stays usable.

Covered by automated tests, not the native run: projection failure and retry, stale projections and same-named row gestures, closed tabs and replaced listings (`crates/dual-pane-application/tests/filtering.rs`), and observation-time native gesture revisions (`crates/dual-pane-desktop/src/browser_session.rs`).

Not yet verified, so the related checklist items remain open: a real input method (System Events and CoreGraphics key events did not start a dead-key composition), VoiceOver announcements, a native view of projection failure and retry, the blank list with the updating cue while projection runs, context menus and inline Rename against filter keystrokes, a native delayed stale gesture, and the momentary absence of another tab's state during a switch.

On 2026-10-21, a Quick Look gesture regression fix addressed a retained normal-panel gesture blocking later Space presses, stale panel callbacks interrupting a newer gesture, and native auto-repeat key-up events that could dismiss a held preview. The desktop seam test covers those transitions, and `make check` plus the three direct Cargo gates pass. A native MP4 retest remains part of the unchecked keyboard-interaction item.
