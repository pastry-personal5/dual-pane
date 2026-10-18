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
- [ ] With a Folder Items List focused, an unmodified printable character other than Space starts filtering that Browser Tab, shows `Starts with: {character}`, and moves focus to the row's text input with the character entered once. Further committed printable input appends once, including IME committed text; preedit remains in the text editor without changing the projection. The other Browser and tabs are unchanged.
- [ ] Prefix matching replaces M2's substring match while retaining M2's normalization and safety rules. Typing and clearing update rows and count feedback. While projection work runs, the list is blank with an updating cue and Item actions blocked, without erasing visit state.
- [ ] If no folder or file starts with the query, the Folder Items List is visually empty with no no-match row, message, or overlay inside it. The visible Folder Items Filter Row, zero-result count, and accessibility feedback distinguish the result from an empty folder, loading, a projection failure, and a read error.
- [ ] The far-right Close Filter Button and Escape clear the active tab's query, restore its complete current listing, hide the Folder Items Filter Row, and return focus to that Browser's Folder Items List. `Command+F` shows and focuses an empty row; focus loss or switching tabs hides an empty row without moving focus, while a nonempty row stays with its tab.
- [ ] Add the stable `FocusFolderItemsFilter` action at the end of the persisted action catalogue with `Command+F` as its default and Window scope. It appears as **Focus Folder Items Filter** in Keyboard Shortcuts Settings, follows the existing conflict and profile-compatibility rules, and shows and focuses the active Browser's Folder Items Filter Row even when its query is empty.
- [ ] Keyboard entry, Escape or clear behavior, focus return, context menus, Quick Look, and inline name editing do not steal or misroute filter keystrokes. Focused filter text editing wins over workspace shortcuts while preserving ordinary text-editor commands.
- [ ] Every row or selection-dependent native gesture carries the tab and visible-row revision observed when the widget receives the gesture; row gestures also preserve the exact byte-backed Item identity. A delayed gesture from before a filter result, tab switch, sort, refresh, or clear is rejected rather than retargeted.
- [ ] User-originated filter edits and Close Filter Button activation carry the tab shown when the signal originated. Rendering application state, restoring another tab's query, or hiding the row cannot emit a user edit or overwrite another tab's filter.
- [ ] A failed or cancelled projection leaves the Folder Items List blank, keeps Item actions blocked, and provides retryable status and accessibility feedback distinct from a successful zero-result projection and a directory-read error. Editing or clearing retries through the existing M2 contract.
- [ ] Accessible names for the Folder Items Filter Row and Close Filter Button, the query value, result count, and zero-result feedback are available to assistive technology, and the row remains usable at supported Browser widths.
- [ ] Native macOS checks cover direct `a` entry from Folder Items, continued `p` entry producing `Starts with: ap`, prefix results, composed text entry, `Command+F`, shortcut rebinding and conflict rejection, Close Filter Button clearing, an empty no-match list, projection failure and retry, delayed stale gestures, rapid tab switching, both Browsers, refresh, sort, selection, and reserved Space/navigation/shortcut behavior in a disposable profile; record the result here.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M3 Done.

## Acceptance evidence

Rust formatting, Clippy, and workspace tests pass; `make check` passes the Rust and C++ formatters, both linters, and all tests. `scripts/check-settings-window.sh` passes with Qt's offscreen platform. The interactive P4-M3 filter acceptance scenarios have not yet been verified in the running application, so the related checklist items remain open.
