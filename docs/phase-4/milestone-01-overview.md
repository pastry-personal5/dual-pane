# P4-M1: Decide interaction and safety behavior

Status: Done

This documentation milestone fixes the Phase 4 feature boundaries and safety rules. The owner chose to decide finer interactions when each feature milestone is planned for implementation. The [architecture plan](milestone-01-architecture.md) maps scenarios to existing boundaries; this checklist is the definition of done. A proposed rule in a later plan is not an owner decision.

## Scope

In scope: a reviewable policy scenario matrix; owner decisions on filter purpose and lifetime, clipboard and drag boundaries, transfer intent, path-entry syntax, basic visual path navigation, and default shortcuts; a named handoff for finer interaction choices in M2–M8; updates to [product behavior](../product-behavior.md), [UX GUI](../ux-gui.md), [UX terms](../ux-terms.md), and [information architecture](../ux-information-architecture.md) where decisions change them.

Out of scope: implementation, recursive search, remote filesystems, new dependencies, and weakening existing conflict or deletion safeguards.

## Owner decisions

| Topic | Accepted behavior | Feature-milestone handoff |
|---|---|---|
| Name filter | Match names in the current folder. Each tab keeps its filter when another tab becomes active; navigating that tab to a different folder clears it. Filtering removes hidden Items from selection and cursor state. Filters reset on relaunch. P4-M3's later owner decision supersedes the original substring rule with prefix matching. | P4-M2/M3: matching edge cases, view reconciliation, row and focus. |
| Clipboard interchange and lifetime | File Copy in Dual Pane publishes every selected Item, including files, folders, packages, and symbolic links, for Paste into Finder until the macOS clipboard is replaced, including after Dual Pane quits or relaunches. Dual Pane also accepts its own Copy and Cut for internal Paste. A file Copy in Finder can be pasted into Dual Pane as a Copy, leaving the Finder sources in place. Cut does not promise a Finder move. Dual Pane's internal transfer intent becomes invalid when another app replaces the clipboard or Dual Pane relaunches; a valid Finder file Copy remains eligible after Dual Pane relaunches while the clipboard is unchanged. | P4-M4/M5: external-source validation, preflight, feedback, and native Finder verification in both directions. |
| Drag interchange, action, and targets | Support drags within Dual Pane. Finder-to-app and app-to-Finder drags are outside Phase 4. An accepted drop moves by default; holding Option copies. Either Browser's confirmed folder, folder rows, and Browser Tabs are drop targets; a tab targets its confirmed folder. Hovering an inactive tab does not activate it. | P4-M7: source selection, post-drop tab activation, rejection, and feedback. |
| Cut and Paste outcome | A same-folder Paste is rejected with an explanation. A started Cut Paste consumes the pending move when the operation reaches any terminal result: success, failure, cancellation, or partial completion. To retry remaining sources, select and Cut them again. | P4-M4/M5: rejected-before-start behavior and presentation. |
| Path syntax and draft | Accept absolute paths, `~/` home-relative paths, and relative paths. Resolve a relative path against the submitting tab's confirmed folder, captured at submission. Do not interpret shell syntax or environment variables. A failed submission keeps its typed text with an error; Escape, focus loss, or switching tabs discards an unsubmitted draft. | P4-M6: focus, symlinks, packages, and non-UTF-8 paths. |
| Shortcut defaults | `Command+F` focuses the Folder Items Filter Row, `Command+L` edits the path, and `Command+C`, `Command+X`, and `Command+V` perform file Copy, Cut, and Paste while the Folder Items List has focus. A focused text editor retains ordinary text Copy, Cut, and Paste. | P4-M3/M5/M6: menus and focus return. |
| Type to filter | With a Folder Items List focused, unmodified printable text other than Space begins editing that Browser Tab's filter and transfers focus to its filter field for continued typing. Space retains Quick Look, and movement, activation, and modified shortcuts retain their existing actions. | P4-M3: query continuation and composition/IME behavior are in the [M3 owner-decision record](milestone-03-overview.md#owner-decisions). |
| Visual path navigation | Clicking a component of the confirmed absolute path navigates to that ancestor. The Path Edit Control can expand into folder-choice columns. Selecting a folder in the expanded columns collapses them. | P4-M8: recorded in [M8 owner decisions](milestone-08-overview.md#owner-decisions--2026-10-23). |

## Deferred owner decisions

These are explicit handoffs, not permission to invent behavior during implementation. Each owning milestone must record the owner's choice in the active product or UX document before feature code depends on it. Its checklist remains unchecked until then. The [policy scenario matrix](milestone-01-architecture.md#policy-scenario-matrix) records what is already fixed.

| Owner milestone | Choices to bring back for review |
|---|---|
| P4-M2 | Unicode and non-UTF-8 name matching; range anchor, scroll, counts, refresh, and same-folder history behavior. |
| P4-M3 | Placement, append behavior, Close Filter Button behavior, prefix matching, the visually empty no-match result, IME commit behavior, Escape focus return, and empty-row dismissal are decided in the [M3 owner-decision record](milestone-03-overview.md#owner-decisions). |
| P4-M4 | Copy repeat behavior, Cut rejection before a job starts, destination-unavailable policy, and preflight for Finder-sourced files are decided in the [M4 owner-decision record](milestone-04-overview.md#owner-decisions). |
| P4-M5 | Menus, pending-Cut cue, feedback for replaced or unsupported clipboard content, and behavior if a copied Finder or Dual Pane source changes. Finder-compatible representation, Finder-source reading, and native verification in both directions are M5 design obligations, not separate owner decisions. |
| P4-M6 | Path editor focus and validation feedback, symlinks, packages, and non-UTF-8 paths; failed draft behavior after a tab switch. |
| P4-M7 | Source selection, same-folder and descendant drops, symlink/package targets, post-drop tab activation, stale payloads, and hover/rejection feedback. |
| P4-M8 | [Column sketch](milestone-08-overview.md#interaction-sketch-for-p4-m8-planning), expansion ownership and height, root behavior, collapse timing on selection, Edit transition, path-component visibility, keyboard and accessibility behavior, narrow-width layout, and animation. Recorded in [M8 owner decisions](milestone-08-overview.md#owner-decisions--2026-10-23). |

The P4-M2, P4-M3, P4-M4, and P4-M8 choices have since been answered in their linked owner-decision records. P4-M5 through P4-M7 have their own decision records; M9 through M11 were added later as directed.

## Completion checklist

- [x] The owner accepts the feature boundaries and safety rules in the decision table, including Finder Copy and Paste in both directions, internal Cut and drag, and the explicit handoff of finer choices to M2–M8. Record the decision date and source.
- [x] The [policy scenario matrix](milestone-01-architecture.md#policy-scenario-matrix) covers both Browsers, inactive tabs, filtering, unavailable folders, conflicts, symlinks and packages, cancellation, partial operations, and changed clipboard or drag sources. Each row states its fixed safety result and owning milestone for remaining behavior.
- [x] The active product and UX docs record accepted behavior; the M1 architecture plan records ownership and invariants. Other plans link to these decisions. New action IDs and controls follow canonical terms and persisted-action compatibility rules.
- [x] M2–M8 overviews explicitly require deferred owner decisions before implementation; their architecture plans remain proposals until those decisions are recorded. Any dependency or build-tool change is requested separately.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record the decision and gate evidence here before marking P4-M1 Done.

## Acceptance evidence

On 2026-10-16, the owner accepted the [decision table](#owner-decisions) through the P4-M1 interview and explicitly deferred finer interactions to M2–M8. Later the same day, the owner expanded Copy to support Paste into Finder for all selected Item types, including after Dual Pane quits or relaunches while the clipboard is unchanged. The owner subsequently added Copy from Finder and Paste into Dual Pane; this is an inbound Copy through the existing safe operation workflow. The [policy scenario matrix](milestone-01-architecture.md#policy-scenario-matrix) assigns handoffs. The accepted behavior is recorded in [product behavior](../product-behavior.md), [UX GUI](../ux-gui.md), and the planned canonical names and action IDs in [UX terms](../ux-terms.md); each later overview has an owner-decision checklist item. On 2026-10-16, direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed. This milestone changed documentation only.
