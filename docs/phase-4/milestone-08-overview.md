# P4-M8: Advanced Path Edit Control

Status: Done

This milestone adds clickable breadcrumbs for the confirmed location and a bounded column navigator showing only its ancestor trail. P4-M6 supplies typed-path navigation; M8 adds a distinct Edit control that enters that existing mode. The [architecture plan](milestone-08-architecture.md) defines the proposed read and UI boundaries. This checklist is the definition of done.

## Scope

In scope: clickable path components, an Expand/Collapse control, a column view of visible folder choices along the confirmed path's ancestor trail, keyboard and accessibility behavior, asynchronous option loading, narrow-Browser layout, and tab switching.

Out of scope: recursive search, filesystem mutation from the path control, network mounting, and changing the underlying Folder Pane into a permanent column view.

## Interaction sketch for P4-M8 planning

Collapsed, with each component of the confirmed path acting as a folder target. The final Edit control enters the P4-M6 text-entry mode:

```text
[/] › [Users] › [alex] › [Projects] › [App]                 [Expand] [Edit]
```

Expanded, inside the same Browser and directly below Folder Pane Toolbar Row #1. Each column lists visible child folders of an ancestor parent; the selected rows trace the path. The last column is the current folder's parent, so the current folder's children do not appear:

```text
[/] › [Users] › [alex] › [Projects] › [App]               [Collapse] [Edit]
┌ / ───────────┬ /Users ────────┬ /Users/alex ────┬ /Users/alex/Projects ┐
│ Applications │ alex       ✓   │ Documents       │ App                ✓ │
│ Users      ✓ │ Shared         │ Projects     ✓  │ Tests               │
│ Volumes      │                │ Pictures        │                     │
└──────────────┴────────────────┴─────────────────┴─────────────────────┘
```

There is one column per ancestor parent, beginning at `/` and ending at the current folder's parent. Each lists visible child folders and highlights the child on the confirmed path. Activating any folder row navigates that tab. At `/`, Expand is disabled because there is no parent column. At narrow widths, only the nearest columns that fit remain visible; older ancestors remain reachable through breadcrumbs. The navigator stays within its Browser and leaves the Folder Pane usable.

## Owner decisions — 2026-10-23

- The expanded navigator shows folder choices along the confirmed ancestor trail. Each column lists visible child folders of a path parent and highlights the next confirmed component. The final column is the current folder's parent; children of the current folder are not listed.
- At the root folder, disable Expand and show no columns. If navigation reaches root while expanded, collapse the navigator. Keep the root breadcrumb available as an exact navigation target from other folders.
- Expansion has a fixed maximum height, is not user-resizable, and never persists its height. Expanded/collapsed state belongs to each Browser and is not restored after relaunch.
- Selecting a folder option starts ordinary navigation and immediately collapses the columns. Breadcrumbs continue to show the confirmed path until success; a pending target is distinguished without replacing it. A failure reports locally and preserves the confirmed path and Folder Items.
- Each ancestor column enumerates visible child folders on a worker. A component already in the confirmed path remains visible and selected even when hidden by item-visibility settings; hidden siblings are omitted. Include navigable folder symlinks and omit packages as choices. Preserve P4-M6 stored-name, symlink, package, and exact-path semantics.
- Edit collapses the columns and enters P4-M6 typed-path editing with its accepted Return, Escape, focus-loss, and tab-switch draft behavior.
- The M6 path text field remains the editor behind Edit and `Command+L`. In display mode, pointer and keyboard activation of a breadcrumb targets that component; the path text itself no longer starts editing. Keep the existing accessible Path Edit Control name for the editor, and give the new controls distinct names.
- Escape from column navigation collapses the columns and returns focus to the Folder Items List. Within a column, Up/Down move among rows; Left/Right move between columns; Return navigates to the selected folder. Controls have accessible names and announce loading, empty, failure, selected, and pending states.
- At narrow Browser widths, calculate how many columns fit at the minimum usable width and show that many, starting with the columns nearest the current folder. Older columns are hidden without horizontal scrolling; their ancestors remain accessible through breadcrumbs. If resizing hides the focused column, move focus to the nearest visible column without navigation or selection changes. Expansion and collapse use a short animation, disabled when reduced-motion is enabled.
- Each column reports loading, empty, and failure locally; failures offer retry. Requests are bounded and tagged with Browser, Tab ID, exact parent `Location`, visibility policy, and generation. Collapse, tab switch, path change, or a newer generation invalidates old requests. Expansion and collapse do not change selection. On Back, Forward, Up, manual path entry, watcher refresh, and tab switch, breadcrumbs and columns follow the newly active tab's confirmed path. The navigator is not session state.

## Expanded folder-choice readability follow-up checklist

- [x] Restore the prior confirmed-path breadcrumb styling and improve sibling folder rows inside expanded ancestor columns.
- [x] Verify sibling, selected-path, hover, and inactive-column presentation in the native path checks; update the UX description.
- [x] Pass the repository gate after the expanded-column styling change.

The collapsed full-path breadcrumbs use their prior transparent styling. Expanded ancestor columns now give enabled sibling rows light text against dark alternating surfaces, with explicit hover and selected-path colors and dimmed inactive surfaces. The offscreen path-columns check verifies foreground/background contrast and a visible selected row; the path-entry check and `make check` also passed on 2026-10-23.

## Inactive Browser path icon tone follow-up checklist

- [x] Dim Expand, Collapse, and Edit bitmap foregrounds when their Browser or window is inactive, and restore the active tone when focus returns.
- [x] Verify the icon variant follows Browser focus and expansion state in the native path checks; update the UX description.
- [x] Pass the repository gate after the icon-tone change.

The path icon renderer now has distinct normal and hover foregrounds for active and inactive Browsers, with the same disabled treatment in either state. Browser and window activation update both buttons, and Expand/Collapse keeps the current tone when it switches glyphs. The offscreen path-entry and path-columns checks passed; on 2026-10-23, `make check` passed formatting, lint, and the full test suite.

## Expand and Edit bitmap buttons follow-up checklist

- [x] Draw readable local bitmap icons for Expand, Collapse, and Edit at standard and high display density; keep the controls icon-only with clear hover, focus, disabled, tooltip, and accessibility feedback.
- [x] Verify icon presence and state changes in the native path checks; update the affected UX description.
- [x] Render the confirmed full path visibly in Row #1 by keeping breadcrumb widgets visible inside their scroll content; cover this in the native check.
- [x] Pass the repository gate after the button change.

The blank path was caused by breadcrumb widgets being created under the outer display and then reparented into the scroll content, leaving them hidden; the content measured only 8×0 despite having path components. Breadcrumbs and separators now start under the scroll content and are shown explicitly. The path-columns check verifies that the full-path components have visible text and nonzero content height, as well as the bitmap button states. It passed twice consecutively; path-entry and text-editor-focus checks also passed. On 2026-10-23, `make check` passed formatting, lint, and the full test suite.

## Folder Pane Row #1 follow-up checklist

- [x] Replace the folder-name title with the confirmed full-path display and its Expand and Edit controls inside Folder Pane Toolbar Row #1; keep typed editing in that same row and ancestor columns directly below it.
- [x] Verify row containment, path display, editing, expansion, and focus with the native path checks; update affected UX and user documentation.
- [x] Pass the repository gate after the layout change.

The Row #1 follow-up keeps the confirmed breadcrumb path, Expand, Edit, and the typed editor inside Folder Pane Toolbar Row #1, with ancestor columns immediately below it. The native path-columns check verifies the full-path tooltip and Row #1 widget containment; the path-entry and text-editor-focus checks also pass. On 2026-10-23, `make check` passed Rust and C++ formatting, lint, and the full test suite after this change.

## Empty path space editing follow-up checklist

- [x] Clicking the empty space to the right of the confirmed path enters the existing editor and selects its text; breadcrumb clicks still navigate.
- [x] Cover the pointer transition in the native path-columns check and update user-facing interaction docs.
- [x] Pass the repository gate after the change.

The breadcrumb background now opens the existing editor when clicked after the final path component. The path-columns check confirms text selection and Escape restoration, then navigates by breadcrumb. On 2026-10-23, `scripts/check-path-columns.sh`, `scripts/check-path-entry.sh`, and `make check` passed; `git diff --check` passed before this checklist update.

## Completion checklist

- [x] Before implementation, the owner decides every [P4-M8 deferred detail](milestone-01-overview.md#deferred-owner-decisions), including column semantics, expansion ownership, collapse timing, folder choices, focus, and feedback; record decisions in product behavior, UX GUI, and information architecture.
- [x] Breadcrumb components are individually pointer and keyboard accessible, expose the full exact target path, and navigate only the active Browser Tab through the ordinary history and request-token rules.
- [x] The M6 path editor remains available through Edit and `Command+L` in either Browser, including while a tab is loading or showing the Missing Folder Overlay. Its accepted text selection, validation, failure, cancellation, and focus behavior survives the display-mode change; clicking a breadcrumb cannot start editing.
- [x] Expand/Collapse exposes visible folder choices along the confirmed path as a bounded-height column navigator and uses the approved width adaptation. Expand is disabled at root, and navigation to root collapses an open navigator. Hidden ancestor columns remain reachable through breadcrumbs, and resize moves focus out of a hidden column safely. The Folder Pane and other Browser remain usable at supported sizes.
- [x] Selecting a folder option collapses immediately and navigates under the M1 rules. A pending target is visually distinct, and stale navigation results cannot replace a newer path or listing.
- [x] Only visible columns request folder options. Reads, retries, and result payloads stay bounded and cannot delay ordinary Folder Items reads; stale responses after collapse, resize, tab switch, path change, or visibility-setting change cannot populate or activate a column.
- [x] Breadcrumbs, expanded columns, and typed-entry mode stay synchronized with Back, Forward, Up, tab switch, manual path entry, watcher refresh, and session restoration. Navigation may apply the already-approved file-selection behavior of P4-M6; merely expanding/collapsing does not change selection or launch a file operation.
- [x] Keyboard focus, screen-reader labels, selected and pending states, Escape behavior, reduced-motion handling, and narrow-Browser layout follow the approved UX decision.
- [x] Qt-free tests cover path-component identity and stale navigation; temporary-directory and native checks cover bounded column reads, visibility settings, symlinks, non-UTF-8 paths where supported, root, errors and retry, rapid tab switching, resizing across the column cutoff, and collapse during an in-flight read. Record native results here.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Update the directly affected product, UX, architecture, and user-facing docs and record evidence here before marking P4-M8 Done. Phase 4 remains Active until P4-M7's guided acceptance is recorded.

## Acceptance evidence

Owner decisions were recorded on 2026-10-23, including root and narrow-width behavior from the plan review interview. Exact `Location` breadcrumb targets and observed navigation reject stale tab or listing gestures; a late result cannot replace the newer confirmed path. Each Browser has a scrolling breadcrumb row with persistent Edit and Expand/Collapse controls, bounded ancestor columns, a distinct pending cue, and a dedicated read lane. Column generations invalidate collapse, resize, path, tab, and visibility changes. The temporary-directory tests cover page bounds, hidden selected components, symlinks, packages, missing-parent retry, and read-lane isolation. APFS on this test volume rejects arbitrary non-UTF-8 names with `EILSEQ`; the domain identity test covers those bytes and the native fixture skips that unsupported case. The offscreen `scripts/check-path-columns.sh` exercises folder choice selection, package exclusion, the other Browser's isolation, keyboard Escape, resize adaptation and focus, breadcrumb navigation, and root behavior. `scripts/check-path-entry.sh` and `scripts/check-text-editor-focus.sh` pass after the display-mode change.

On 2026-10-23, direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed; `make check` passed Rust and C++ formatting, lint, and tests. `git diff --check` and the three native scripts above passed. The path-columns check also passed three sequential runs after the resize and focus acceptance assertion was added. P4-M8 is Done. Phase 4 remains Active because the separate [P4-M7 guided real-pointer acceptance](milestone-07-overview.md#completion-checklist) is still unchecked.
