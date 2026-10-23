# P4-M12: Repair file drag and drop interaction

Status: Active

This milestone lets a Folder Items drag start after a primary press with any held keyboard modifier, then decides whether the attempted drop is allowed. A refused drop explains why to the user and starts no file operation; an accepted drop performs the advertised Move or Copy. It closes the real-pointer acceptance gap left by [P4-M7](milestone-07-overview.md) without changing the internal-only transfer boundary or the existing file-operation safeguards.

## Scope

In scope:

- Primary-button press, threshold crossing, drag preview, modifier changes, drop, and cancellation for selected Folder Items in either Browser.
- Command, Shift, Option, Control, and combined modifiers at drag start, including macOS Control-primary-click behavior, with ordinary click selection behavior still defined when the threshold is not crossed.
- Shift-secondary-click and other secondary-click Context Menu gestures while primary-button drag handling changes.
- Consistent Move or Copy feedback and actual filesystem result for Browser-folder, eligible folder-row, active-tab, and inactive-tab targets.
- A visible, accessible reason after a release over a Dual Pane drop surface is refused at native admission, application validation, or asynchronous destination preflight.
- A temporary, clearly labeled current-folder drop target over each Browser's Folder Items List during an internal file drag. It uses the existing Browser-folder target and stays reachable when every visible row is occupied.
- A native event-ordering check and guided real-pointer macOS acceptance using only disposable profile and temporary file data.

Out of scope:

- External drags, new application target classes, spring-loaded navigation, row reordering, or a new file-operation executor.
- Changes to conflict, replacement, cancellation, journal, or canonical-destination safety policy.

## Owner decisions

- The owner directed that holding **any modifier before pressing a Folder Item and moving beyond the drag threshold can start a drag**. The P4-M7 rule that excludes Command- and Shift-modified presses is superseded for M12.
- A modified click keeps its normal selection gesture when released below threshold. Once that press becomes a drag, pressing a selected row drags the selection that existed before the press; pressing an unselected row drags that row alone. The drag does not apply the modified click gesture. A changed or vanished press observation rejects rather than silently retargeting another selection.
- At drop, Option selects Copy even with another modifier held; other modifier combinations select Move. The modifier state at drop, rather than at press, determines the operation.
- Let the user begin and carry a drag from an identifiable Folder Item over a target before deciding whether a transfer can start. Unsupported item kinds can be carried as a drag but must be rejected at drop. A row that disappeared before its identity can be captured cannot start a meaningful drag. Hover may show an ineligible cue, but merely hovering does not report a failure. On release over a Dual Pane drop surface, reject an impossible drop with a specific reason in that Browser's Status Bar and an accessibility announcement. External and malformed drags receive a safe refusal message and cannot start an operation. Platform drag admission solely to receive a release does not approve a file operation.

## Completion checklist

- [x] Record the owner decisions above in this plan, the active UX rule, and the P4-M7 supersession note. The owner answered the selection and drop-modifier interview and directed the visible post-drop rejection behavior on 2026-10-23.
- [ ] A physical primary press on an identifiable Folder Item can begin a file drag after the platform threshold with no modifier, each modifier, or a combination, without an early target or operation-eligibility decision. Cover macOS Control-primary-click specifically. A press on empty space cannot begin a file drag. A click released below threshold applies its ordinary selection gesture exactly once.
- [ ] Exact Shift-secondary-click still opens the Comprehensive Folder Items Context Menu, while other secondary-click modifier combinations follow the existing Normal rule. Neither gesture begins a file drag or changes the selected drag roots.
- [x] The drag freezes the intended selected roots once at threshold crossing and validates the press observation; a changed view or selection cannot silently substitute roots. Cancellation, an invalid source, or a rejected target starts no operation and does not create a second selection change on release.
- [x] An internal drag reveals a labeled current-folder target in each Browser even when its list has no empty space. Hover and drop use the existing Browser-folder preview, authoritative acceptance, refusal, and asynchronous physical preflight. The target updates Move/Option-Copy feedback, handles an ineligible drop, and disappears after drop or cancellation without changing selection, focus, or navigation.
- [ ] A real drag can reach a Dual Pane target even when hover validation predicts refusal. Eligible targets preview the current operation; ineligible targets use an ineligible cue without an eligible outline. Changing modifiers updates the action and feedback. Hover alone causes no rejection message, operation, or file change. Event delivery is verified separately from the platform cursor or action badge.
- [x] Releasing over a Dual Pane target invokes one authoritative drop decision. Every refused attempt, including an unsupported source kind or target row, same-folder or descendant destination, stale source or target, unavailable destination, failed physical preflight, external source, or malformed token, clears hover and briefly displays a safe reason in the target Browser Status Bar with an accessibility announcement. A native decode or bridge failure also produces feedback. A rejected drop starts no operation and changes no file.
- [x] A drop that passes validation chooses and revalidates the advertised Move or Copy action at drop time, then enters the existing asynchronous physical-destination and operation workflow. A later preflight rejection receives the same visible reason treatment.
- [x] An accepted drop reports only that the existing operation has started. After successful completion, Copy leaves each source in place and produces the corresponding destination item; Move produces the destination and removes each source only under the existing verified-before-removal workflow. Both directions and all supported target classes use the same path; conflict, cancellation, and partial results retain their existing meanings.
- [x] Existing source provenance, one-use token, stale-view, physical-destination, same-folder, descendant, package, symlink, journal, and operation-decision safeguards still hold.
- [x] Focused native checks deliver press/move/release to the actual widgets, including selected and unselected rows, below-threshold clicks, modifier changes, eligible and ineligible list and tab targets, immediate and late rejection reasons, and completed temporary-directory Copy and Move effects. Qt-free tests cover any changed policy. No check uses real user file paths.
- [ ] Guided real-pointer macOS acceptance records the drag threshold, each modifier-start gesture including Control-primary click, Move and Option-Copy cursor/action badge, both Browsers, target classes, a refused release and its visible reason, cancellation, and an external-drag rejection. Record any platform cursor limitation separately from whether rejection feedback reaches the user. A synthetic drop alone does not satisfy this item.
- [x] Update the README if delivered user-visible drag behavior changes, and update affected UX, milestone, architecture, index, and changelog docs in the same change.
- [x] `make check`, `scripts/check-file-drag-drop.sh`, and `scripts/check-folder-items-context-menu.sh` pass after implementation; record exact results before marking P4-M12 Done.
- [ ] Reconcile P4-M7's pending real-pointer item with M12's guided physical evidence without retroactively treating its prior synthetic check as physical acceptance.

## Planning baseline

On 2026-10-23, `scripts/check-file-drag-drop.sh` passed: eight application tests and the offscreen native check completed temporary-file Copy and Move transfers in both Browser directions. That check begins drag sessions through the model; it does not exercise a real modified mouse press, the macOS drag threshold, or the platform action badge. The Context Menu Qt-free tests passed (ten tests), and the offscreen native menu check passed with a disposable profile and `--new-instance`: exact Shift context-menu events route to Comprehensive, while Shift+Control routes to Normal. The script's standard launch without `--new-instance` exited with “Operation not permitted” in this sandbox. Synthetic events do not prove either gesture on a physical macOS pointer; those remain M12 acceptance evidence.

## Implementation evidence pending physical acceptance

The current-folder target is shown in both Browsers while the native drag session runs and hidden when it ends. The expanded disposable native check observes that session callback, then exercises the target's Move and Option-Copy preview, malformed-payload refusal, visibility cleanup, and a completed Move through the ordinary Browser-folder operation path. After this change, `scripts/check-file-drag-drop.sh` passed its nine Rust tests and native check, and `make check` passed Rust/C++ formatting, Rust/C++ lint, and the full test suite. Physical pointer acceptance remains in the unchecked M12 and M7 checklists.

The updated native check sends primary press, threshold move, and release events through the Folder Items viewport with every modifier class, a combined modifier, selected and unselected rows, a below-threshold modified click, an empty-space press, and a secondary-button sequence. It confirms that a changed selection before threshold does not retarget a drag. A disposable-check-only source seam supplies a registered live Folder Items widget to synthetic Qt drag events; production events use Qt’s source. The check delivers eligible and ineligible drag-enter, drag-move, and drop events through the actual list and Browser Tab widgets, verifies drop-time Move/Copy updates, stale-target, malformed, same-folder, and external refusal messages, and captures the Status Bar accessibility announcement. A link to the source folder passes logical hover then rejects during asynchronous physical preflight with no source removal. The application test confirms that an unsupported item kind receives a one-use drag token but rejects on accept without work. Disposable-file checks complete Browser-folder Copy and Move, folder-row Move, active-tab Move and Copy, inactive-tab Copy, and a right-to-left Copy; they check source presence or removal and destination creation. These offscreen checks establish widget event handling and operation effects, not physical macOS pointer delivery or cursor badges.

The native check also simulates Qt reporting a physical Control-primary sequence as right-button events. It verifies that the context menu stays closed, a drag starts after threshold, a below-threshold release selects once, and an ordinary secondary-button sequence still does not drag. This simulation does not establish the event sequence produced by a real pointer.

On 2026-10-23, after the widget-event seam and AppKit follow-up, `make check` passed Rust and C++ formatting, Rust and C++ lint, and the full test suite. `scripts/check-file-drag-drop.sh` passed nine application drag tests and the expanded offscreen native check. `scripts/check-folder-items-context-menu.sh` passed ten application tests and the native check with a disposable profile and `--new-instance`. Physical macOS pointer acceptance is still unrecorded.

## Guided real-pointer acceptance

Use a real macOS pointer, a disposable profile, and only files created for this check. The offscreen native check cannot substitute for this run. In Terminal, from the repository root:

```sh
drag_home=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-m12.XXXXXX")
mkdir "$drag_home/Source" "$drag_home/Destination" "$drag_home/Destination/Folder"
: > "$drag_home/Source/move-me"
: > "$drag_home/Source/copy-me"
: > "$drag_home/Source/refuse-me"
printf 'Disposable drag profile: %s\n' "$drag_home"
DUAL_PANE_TEST_PROFILE="$drag_home" target/debug/dual-pane-desktop --new-instance "$drag_home/Source"
```

Use the right Browser's Path Edit Control to open the printed `Destination` path. With the left Browser showing `Source`, check the actual threshold and try a primary drag begun with no modifier, Command, Shift, Option, Control, and at least one combined modifier. Check a selected row and an unselected row, and release one modified press below threshold to verify its click gesture. Check a real Shift-secondary-click and another modified secondary-click against the Context Menu rules. Move `move-me` to the right Browser folder and Option-Copy `copy-me`; confirm the resulting files in the disposable folders. Copy or move a separate item to `Folder`, then check an active tab and an inactive tab without activating the latter on hover. Change Option while hovering and record the cursor/action badge as well as the target cue. Release `refuse-me` over a non-folder row or its own source folder and confirm a specific Status Bar reason, accessibility announcement, and no operation; press Escape during another drag and confirm no operation. Finally, drag one disposable file from Finder into the right Browser and confirm its refusal reason and unchanged file. Record the observed platform cursor separately from whether the refusal appeared. Close the test window when finished; the printed disposable folder can be inspected or removed afterward.
