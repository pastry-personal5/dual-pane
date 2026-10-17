# P4-M3: Deliver text filtering UI

Status: Planned

This milestone makes the tab-local filter usable in each Browser. The [architecture plan](milestone-03-architecture.md) defines the desktop wiring; this checklist is the definition of done.

## Scope

In scope: an accessible Folder Items Filter Control, direct printable-key entry from a focused Folder Items List, clear gestures, empty and no-match feedback, focus and shortcut behavior, view-model updates, and native verification.

Out of scope: recursive search, file content search, and filter persistence beyond the M1 decision.

## Completion checklist

- [ ] Before implementation, the owner decides the [P4-M3 control and type-to-filter details](milestone-01-overview.md#deferred-owner-decisions), including placement, clear/focus behavior, query continuation, and composition; record the decisions in [UX GUI](../ux-gui.md).
- [ ] Each Browser displays a Folder Items Filter Control for its active tab in the owner-approved location; changing tabs restores the correct tab's text and view without briefly showing another tab's results.
- [ ] With a Folder Items List focused, an unmodified printable character other than Space starts filtering that Browser Tab and moves focus to its filter field with the character entered once. Further typing edits the filter under the owner-approved continuation and composition rules; the other Browser and tabs are unchanged.
- [ ] Typing and clearing the filter update rows and count feedback under the M1 and M2 rules. While projection work runs, the list is blank with an updating cue and Item actions blocked, without erasing visit state. The no-match state is distinguishable from an empty folder, loading, and a read error.
- [ ] Keyboard entry, Escape or clear behavior, focus return, context menus, Quick Look, and inline name editing do not steal or misroute filter keystrokes. Any new shortcut appears in Keyboard Shortcuts Settings with a stable action ID.
- [ ] Accessible name, value, result count, and no-match feedback are available to assistive technology, and the control remains usable at supported Browser widths.
- [ ] Native macOS checks cover direct `g` entry from Folder Items, continued and composed text entry, clearing, two tabs, both Browsers, refresh, sort, selection, and reserved Space/navigation/shortcut behavior in a disposable profile; record the result here.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M3 Done.

## Acceptance evidence

Pending M3 owner decisions and implementation.
