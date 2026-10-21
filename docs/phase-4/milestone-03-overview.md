# P4-M3: Deliver text filtering UI

Status: Done

This milestone makes the tab-local prefix filter usable in each Browser. The [architecture plan](milestone-03-architecture.md) defines the desktop wiring; this checklist is the definition of done.

## Scope

In scope: an accessible temporary Folder Items Filter Row, direct printable-key entry from a focused Folder Items List, prefix matching, the Close Filter Button, empty-result feedback, focus and shortcut behavior, view-model updates, and automated desktop verification.

Out of scope: recursive search, file content search, filter persistence beyond the M1 decision, and the deferred native validation recorded below.

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

## Review corrections

The 2026-10-21 implementation review found that the native filter callbacks read the model's current tab when dispatching an edit, Close Filter Button activation, or Escape. A delayed callback could therefore name a tab other than the row that produced it. The row now records the tab whose text it has rendered and submits that recorded identity; the application remains the final active-tab validator. The review also separates an intentionally empty `Command+F` or user-cleared row from a programmatic query clear, so a successful navigation cannot leave an unwanted empty row visible. These are delivery corrections, not new product decisions.

## Completion checklist

- [x] Before implementation, the owner decides the remaining [P4-M3 type-to-filter details](milestone-01-overview.md#deferred-owner-decisions), including composed-text commitment and exact focus destinations; record the decisions in [UX GUI](../ux-gui.md) and [UX information architecture](../ux-information-architecture.md).
- [x] With a Folder Items List focused, an unmodified printable character other than Space starts filtering that Browser Tab, shows `Starts with: {character}`, and moves focus to the row's text input with the character entered once. Further committed printable input appends once, including IME committed text; preedit remains in the text editor without changing the projection. The other Browser and tabs are unchanged.
- [x] If no folder or file starts with the query, the Folder Items List is visually empty with no no-match row, message, or overlay inside it. The visible Folder Items Filter Row, zero-result count, and accessibility feedback distinguish the result from an empty folder, loading, a projection failure, and a read error.
- [x] The far-right Close Filter Button and Escape clear the active tab's query, restore its complete current listing, hide the Folder Items Filter Row, and return focus to that Browser's Folder Items List. `Command+F` shows and focuses an empty row; focus loss or switching tabs hides an empty row without moving focus, while a nonempty row stays with its tab.
- [x] Add the stable `FocusFolderItemsFilter` action at the end of the persisted action catalogue with `Command+F` as its default and Window scope. It appears as **Focus Folder Items Filter** in Keyboard Shortcuts Settings, follows the existing conflict and profile-compatibility rules, and shows and focuses the active Browser's Folder Items Filter Row even when its query is empty.
- [x] Every row or selection-dependent native gesture carries the tab and visible-row revision observed when the widget receives the gesture; row gestures also preserve the exact byte-backed Item identity. A delayed gesture from before a filter result, tab switch, sort, refresh, or clear is rejected rather than retargeted.
- [x] User-originated filter edits, Close Filter Button activation, and Escape carry the tab whose filter value was rendered when the signal originated. Programmatic rendering, restoring another tab's query, or hiding the row cannot emit a user edit or overwrite another tab's filter; a programmatic clear hides the row unless it was explicitly opened empty.
- [x] Accessible names for the Folder Items Filter Row and Close Filter Button, the query value, result count, and zero-result feedback are available to assistive technology, and the row remains usable at supported Browser widths.
- [x] Add an offscreen desktop seam check for filter-row tab capture, programmatic-update suppression, explicit-empty-row lifetime, and failure/updating status rendering. Keep this check independent of a physical input method and VoiceOver.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M3 Done.

## Acceptance evidence

Rust formatting, Clippy, and workspace tests pass; `make check` passes the Rust and C++ formatters, both linters, and all tests. `scripts/check-settings-window.sh`, `scripts/check-text-editor-focus.sh`, and `scripts/check-application-title-row.sh` pass with Qt's offscreen platform. The text editor focus check now also sends `QInputMethodEvent` composed text: a Folder Items List hands preedit and commit to its handler, and the filter field changes its text once, on commit only.

On 2026-10-21, after the native filter-row review correction, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed. The offscreen text-editor check also passed in its disposable profile.

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

On 2026-10-21, a Quick Look gesture regression fix addressed a retained normal-panel gesture blocking later Space presses, stale panel callbacks interrupting a newer gesture, and native auto-repeat key-up events that could dismiss a held preview. The desktop seam test covers those transitions, and `make check` plus the three direct Cargo gates pass.

On 2026-10-21, `scripts/check-text-editor-focus.sh` passed with the new offscreen filter-row seam. It uses a real `QLineEdit` signal path to verify programmatic-update suppression, rendered-tab capture across a simulated tab switch, explicit-empty-row dismissal, and distinct Updating, failed, no-match, empty-folder, and matched-result statuses. Together with the existing session test for rejection of a delayed tab identity, this closes the native signal-boundary checklist item.

## Deferred native validation

At the owner's direction, the following physical-platform validation is deferred with no target date and is not part of P4-M3 completion:

- Verify the row and its list state during rapid tab switching, updating, and projection failure or retry; include both Browsers, refresh, sort, selection, and a delayed stale gesture.
- Exercise keyboard routing across the filter, context menu, inline Rename, Quick Look (including an MP4), Escape, Close, and the reserved Space/navigation/shortcut behavior.
- Verify a real input method's preedit and committed text behavior, and confirm VoiceOver exposes and announces the query and filter-result states.
