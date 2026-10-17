# P4-M8: Advanced Path Edit Control

Status: Planned

This milestone makes each component of the confirmed absolute path directly navigable and lets the Path Edit Control expand vertically into available folder choices. P4-M6 supplies typed-path navigation; the [architecture plan](milestone-08-architecture.md) defines the additional read and UI boundaries. This checklist is the definition of done.

## Scope

In scope: clickable path components, an Expand/Collapse control, a column view of folders along the current path, branch navigation, keyboard and accessibility behavior, asynchronous option loading, narrow-Browser layout, and tab switching.

Out of scope: recursive search, filesystem mutation from the path control, network mounting, and changing the underlying Folder Pane into a permanent column view.

## Interaction sketch for P4-M8 planning

Collapsed, with each component of the confirmed path acting as a folder target:

```text
[/] › [Users] › [alex] › [Projects] › [App]                 [Expand] [Edit]
```

Expanded, inside the same Browser and above its Folder Pane:

```text
[/] › [Users] › [alex] › [Projects] › [App]               [Collapse] [Edit]
┌ / ───────────┬ /Users ────────┬ /Users/alex ────┬ /…/Projects ─┐
│ Applications │ alex       ✓   │ Documents       │ App        ✓  │
│ Users      ✓ │ Shared         │ Projects     ✓  │ Tests         │
│ Volumes      │                │ Pictures        │               │
└──────────────┴────────────────┴─────────────────┴───────────────┘
```

The sketch proposes one scrollable column per ancestor level. The selected row in each column traces the confirmed path; a trailing column may list children of the current folder. The expanded area takes a bounded share of that Browser's height, with horizontal scrolling or a single visible column at narrow widths. It does not cover the other Browser. These presentation details remain proposals for M8 owner review.

## Proposed behavior to decide in P4-M8

- Clicking a breadcrumb ancestor requests navigation for the active tab immediately. The current component is a no-op, and each component retains an exact target path even if its label is elided.
- Expand/Collapse changes only the path navigator's visibility. It never navigates, changes Folder Items selection, or writes session state by itself.
- Clicking a folder option requests navigation for the active tab and collapses the navigator under the M1-approved rule. M8 planning decides whether collapse happens at activation or only after a successful read. A pending branch is visually distinct from the confirmed path; failed reads leave the confirmed path and Folder Items intact.
- Edit enters the P4-M6 text-entry mode. Decide whether the expanded columns collapse while editing, and how Escape, Return, focus changes, and tab switches treat an unsubmitted draft.
- Decide which folder kinds appear, how visibility settings affect them, whether expansion height persists, and whether expanded columns follow the active tab or stay open per Browser.

## Completion checklist

- [ ] Before implementation, the owner decides the [P4-M8 column and navigation details](milestone-01-overview.md#deferred-owner-decisions), including the sketch, expansion state, collapse timing, folder choices, focus, and feedback; record them in product behavior, UX GUI, and information architecture.
- [ ] Breadcrumb components are individually pointer and keyboard accessible, expose the full exact target path, and navigate only the active Browser Tab through the ordinary history and request-token rules.
- [ ] Expand/Collapse exposes a bounded-height, scrollable column navigator for the M8-approved folder choices. The Folder Pane and other Browser remain usable at supported sizes.
- [ ] Selecting an approved folder option navigates under the M1 rules. Missing, unreadable, slow, and stale option reads give local feedback and cannot replace a newer path or listing.
- [ ] Breadcrumb, expanded, and typed-entry modes stay synchronized with Back, Forward, Up, tab switch, manual path entry, watcher refresh, and session restoration without changing file selection or launching a file operation.
- [ ] Keyboard focus, screen-reader labels, selected and pending states, Escape behavior, reduced-motion handling, and narrow-Browser layout follow the approved UX decision.
- [ ] Qt-free tests cover path-component identity and stale navigation; temporary-directory and native checks cover columns, symlinks, non-UTF-8 paths where supported, errors, rapid tab switching, and collapse during an in-flight read. Record native results here.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Update the directly affected product, UX, architecture, and user-facing docs and record evidence here before marking P4-M8 and Phase 4 Done.

## Acceptance evidence

Pending M8 owner decisions and implementation.
