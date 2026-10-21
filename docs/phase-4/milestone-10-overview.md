# P4-M10: Enhance Folder Items Context Menu

Status: Done

This milestone replaces the single fixed Folder Items Context Menu with a fast Normal variant, an exact-Shift Comprehensive variant, and persistent visibility controls in a new Context Menus Settings screen. It is active and implemented through the application, storage, adapter, bridge, Settings, and owned non-blocking native-menu boundaries. Automated checks and the full gate pass; the owner's guided macOS acceptance remains. The [architecture plan](milestone-10-architecture.md) defines the typed projection, persistence, asynchronous native-menu lifecycle, and stale-action boundary; this checklist is the definition of done.

## Scope

In scope:

- Normal and Comprehensive Folder Items Context Menus for selected rows and empty space.
- Independent, globally shared visibility settings for the two variants.
- The eleven already catalogued actions named below; fixed command groups, contextual availability, separators, effective shortcut annotations, pointer and platform context-menu invocation, selection and target semantics, accessibility, persistence, reset controls, and verification.

Out of scope:

- New file-operation or integration commands, including Open With, Get Info, Duplicate, Compress, Copy Path, Reveal in Finder, aliases, tags, sharing, Services, and terminal commands.
- Quick Look, navigation, sort, filter, selection, and tab commands in either menu.
- User-defined command ordering, grouping, or separators.
- Changes to shortcut defaults, scopes, conflict handling, or persistence.

## Implementation state at plan review

Already present in the worktree:

- The application-owned eleven-action catalogue, factory preferences, at-least-one validation, typed preference commands, local/global reset methods, and settings-save requests.
- Schema-11 storage, default seeding, strict load validation, and initial round-trip and version-10 migration tests.
- Observation-bearing request/invoke/dismiss commands, grouped snapshots with first-shortcut metadata, one-use action allow-listing, and initial selection, empty-space, stale-source, and stale-destination tests.
- A shared native `QContextMenuEvent` path for pointer and non-pointer requests, exact-Shift routing, accessible variant names, section collapse, native shortcut text, and both Browser instances.
- Adapter-owned Context Menus Settings rows and rejection projection, typed bridge getters and commands, and the third Settings page with independent checkboxes, first-shortcut text, inline final-action feedback, and three scope-specific confirmations.

Still required to close the milestone: run and record the owner's guided macOS acceptance.

## Owner decisions

The owner decided the P4-M10 behavior on 2026-10-21.

### Variants and invocation

| Topic | Accepted behavior |
|---|---|
| Normal menu | Secondary-click opens the Normal Folder Items Context Menu. Its factory configuration is deliberately shorter for low launch latency and faster scanning. |
| Comprehensive menu | Secondary-click with exact Shift and no Command, Option, or Control opens the Comprehensive Folder Items Context Menu. Any other secondary-click opens Normal. |
| Non-pointer invocation | A platform keyboard or accessibility context-menu request opens Normal by default and Comprehensive only when the event carries exact Shift. It targets the visible cursor row, or empty space when there is no visible cursor, and uses the same selection rule as a pointer invocation. This adds no new fixed or customizable shortcut. |
| Selection | Both variants use the existing rule: a selected row preserves the selection; an unselected row becomes the sole selection. Empty-space invocation preserves the selection but offers only destination actions. The selection change precedes display and is not rolled back when the menu is dismissed. |
| Appearance | Both variants use the same native look, spacing, section treatment, and shortcut column. There is no visible variant header; distinct accessible names identify them. |
| Runtime availability | A configured command appears only when it can run in the current context. Unavailable commands, empty sections, and redundant separators are hidden. If no configured command is available, no menu opens and no feedback is shown. |
| Responsiveness | Opening either variant waits for no worker, filesystem call, or clipboard-provider read. It uses the cached clipboard offer; Paste performs its ordinary fresh validation when invoked. There is no numeric latency threshold. |

### Command information architecture

Both variants use one fixed, flat section order. Settings changes visibility only. An action must be both configured for the requested variant and applicable to the invocation context:

| Section | Commands, in order | Invocation context |
|---|---|---|
| Rename and Create | Rename Item; New Folder | Rename requires exactly one selected Item. New Folder requires a ready confirmed destination and is eligible on a row or empty space. |
| Open | Open Item; Show Package Contents; Open in Finder | Open Item requires exactly one selected Item. Show Package Contents requires exactly one selected Item whose direct kind is package. Open in Finder requires exactly one selected Item and reveals it in the Finder without opening or navigating to it. |
| Clipboard | Copy Items; Cut Items; Paste Items | Copy and Cut require one or more selected Items. Paste requires a ready confirmed destination and a supported cached clipboard offer; it is eligible on a row or empty space. |
| Other Browser | Copy to Other Browser; Move to Other Browser | One or more selected Items and a ready other-Browser destination are required. |
| Delete | Move to Trash; Delete Permanently | One or more selected Items are required. |

The factory-visible commands are:

| Variant | Factory configuration |
|---|---|
| Normal | Open Item; Copy Items; Cut Items; Paste Items; Copy to Other Browser; Move to Other Browser; Move to Trash |
| Comprehensive | Every command in the table above, including Open in Finder |

The existing application admission rules remain authoritative in addition to the table: loading, failed, missing, stale, filtered-update, journal, clipboard, destination, and operation conditions can make an action unavailable. Empty-space invocation reduces either configured variant to configured, available destination actions such as New Folder or Paste Items even though it preserves the existing selection. Every visible command shows only the first shortcut in its effective ordered shortcut list, using native macOS notation; an unbound command shows no annotation. The annotation is display-only and does not install another shortcut.

### Context Menus Settings

Context Menus Settings is the third top-level Settings category, after Keyboard Shortcuts Settings. It applies globally to both Browsers and presents one categorized matrix with these columns:

| Column | Content |
|---|---|
| Action | Canonical command name, grouped in the menu section order above |
| Normal | Whether the command may appear in the Normal menu |
| Comprehensive | Whether the command may appear in the Comprehensive menu |
| Shortcut | Read-only first effective shortcut in the same native notation as the menus, or None |

The two visibility columns are independent; neither variant must remain a subset of the other. Changes apply immediately and save through the existing settings workflow. Each variant must retain at least one configured command. Attempting to clear its final checkbox leaves it selected and shows inline feedback for that variant.

The screen provides Reset Normal…, Reset Comprehensive…, and Reset Both…. Every reset opens its own confirmation, names the scope it will restore, and uses Cancel as the default and Escape action. An accepted local reset changes only the named context-menu configuration and leaves the Settings Window open; global Reset Settings restores both as part of all compiled defaults. The controls follow the existing Settings availability and save-failure behavior.

## Completion checklist

- [x] The owner decides the [variants, invocation, command hierarchy, defaults, Settings information architecture, persistence behavior, and acceptance level](#owner-decisions). The accepted behavior is recorded in product behavior, UX GUI, UX terms, information architecture, and the Phase 4 changelog before implementation.
- [x] Add the application-owned twelve-action catalogue (extended from eleven by Open in Finder) and complete Normal and Comprehensive preferences, with fixed group/order, independent visibility, at-least-one validation, no-op behavior, local resets, global reset integration, and immediate settings-save output.
- [x] Add one typed, observation-bearing menu request that atomically activates the Browser, applies the accepted row-selection rule, and returns either an immutable grouped snapshot plus one-use token or no menu. Row, empty-space, keyboard, accessibility, exact-modifier, both-Browser, and stale-request behavior match the accepted rules.
- [x] Application-owned projection hides unavailable commands and empty groups, suppresses source actions for empty-space invocation, uses the cached clipboard offer without blocking work, and carries only the first effective shortcut as display metadata. Shortcut edit, removal, reset, and global Reset Settings update later menus and the Settings matrix immediately.
- [x] Rename and Create, Open, Clipboard, Other Browser, and Delete render in the accepted flat order and obey the action-context table. Package, link, multiple-selection, empty-space, pending-load, failed-folder, filtered-update, journal, clipboard, destination, and same-folder conditions use application policy rather than Qt policy.
- [x] Menu invocation consumes the one-use token and accepts only an action emitted in that snapshot. It revalidates exactly the Browser, tab, listing, selection, source, destination, and Item identity that action needs; selection, filter result, refresh, tab, destination, or Item changes cannot retarget an editor, navigation, clipboard read, or file operation.
- [x] One native event path opens Normal or exact-Shift Comprehensive in either Browser without duplicate mouse/context events. It uses deterministic cursor or viewport anchoring, the shared native presentation and distinct accessible names, collapses separators, opens nothing for an empty snapshot, and displays native shortcut annotations without registering them. It uses an owned non-blocking `QMenu::popup` session—never `QMenu::exec` or another nested event loop—and a queued idempotent finalizer dispatches or dismisses exactly once regardless of `triggered`/`aboutToHide` signal order.
- [x] Context Menus Settings appears after Keyboard Shortcuts Settings with the accepted matrix, global scope, independent checkboxes, read-only native shortcut column, inline final-item rejection, settings-unavailable behavior, three scope-specific confirmed resets, accessible names, Full Keyboard Access behavior, and immediate persistence.
- [x] The settings database gains a transactional schema-11 step that stores exactly both configurations under stable eligible `ActionId` names, seeds fresh and every supported older profile with the accepted defaults, round-trips custom values, rejects extra, unknown, duplicate, malformed, missing, or empty configurations, and preserves the existing corrupt/newer-database recovery behavior.
- [x] Qt-free application and adapter tests cover defaults, independent edits, no-ops, last-item rejection, local and global resets, action-context availability and grouping, exact modifiers, both Browsers, one-use and stale observations, shortcut changes, settings and rejection view models, save/load, every supported migration path, corruption, rollback, recovery, and relaunch. Filesystem fixtures remain in temporary directories.
- [x] `scripts/check-settings-window.sh` covers the new page, and a new `scripts/check-folder-items-context-menu.sh` covers both real-widget menus without reading the global clipboard: gesture selection, exact modifier routing, rows and empty space, keyboard/accessibility requests, section collapse, empty results, packages, links, multiple selection, shortcut annotations, event-loop return while open, state change while open, one dispatch, dismissal, accessibility, and both Browsers. List the new script in AGENTS.md and README.
- [x] The owner runs the guided macOS acceptance for visual parity, launch responsiveness, pointer and platform invocation, native shortcut notation, Settings customization and persistence, reset confirmations, and both Browsers; record the result here.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Update affected user and design docs and record all acceptance evidence here before marking P4-M10 Done.

## Acceptance evidence

Planning decisions, implementation, automated checks, and the full repository gate are complete. Guided owner acceptance is the only remaining checklist item.

Settings implementation evidence on 2026-10-21:

- Application tests cover changed, unchanged, rejected-final-action, independent local reset, reset-both, and save-request behavior.
- Adapter tests cover ordered catalogue rows, factory visibility, first-shortcut projection, revisions, and final-action rejection state.
- `scripts/check-settings-window.sh` passes with the third category, eleven rows in five groups, independent factory columns, accessibility, category switching, and all three scope-specific confirmations with Cancel as default and Escape.
- Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass for this Settings checkpoint. The native tests require running outside the filesystem sandbox.

Implementation and hardening evidence on 2026-10-21:

- The Folder Items menu now uses a workspace-wide owned `QMenu::popup` session. A queued finalizer waits for closure and consumes or dismisses the application snapshot once for either native signal order; a newer request synchronously retires the previous session across both Browsers.
- Application tests cover all eleven grouped actions, package and link policy, multiple and empty-space selection, cached Paste projection, immutable first-shortcut metadata, both Browsers, stale requests, selection/listing/tab/destination changes, non-emitted actions, dismissal, replacement, and one-use dispatch.
- Schema tests cover fresh and customized round trips, versions 1 through 10, missing and extra counts, unknown kind/action, invalid visibility, empty variants, and the duplicate-row/missing-row combination. The duplicate check fixed a corruption-validation gap found during this audit.
- `scripts/check-folder-items-context-menu.sh` runs the application policy scenarios and then passes an offscreen native check without observing the global clipboard. Together they cover gesture selection, exact modifier routing, rows and empty space, keyboard/accessibility requests, empty results, package/link/multiple-selection shapes, both Browser widgets, accessible names, flat sections, native shortcut annotations, event-loop return while open, state change while open, both native signal orders, one dispatch, and idempotent dismissal.
- Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass. `make check` passes outside the filesystem sandbox, as required by its native clipboard, metadata, and local-socket tests. `scripts/check-settings-window.sh` and `scripts/check-folder-items-context-menu.sh` pass outside the sandbox with isolated profiles.

Catalogue extension evidence on 2026-10-21:

- Added `OpenInFinder` as a twelfth catalogued action (Open group), enabled by default only in Comprehensive, revealing the one selected Item in the Finder via `open -R` without opening or navigating to it. Settings and Context Menus Settings surface it through the existing generic catalogue and matrix code with no action-specific UI.
- Application tests (`context_menus.rs`, `settings.rs`) extended to cover the twelfth action's scope, default shortcut, availability, and comprehensive-only default visibility. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass across the workspace; `make fmt-check` and `make lint-cpp` pass for the changed C++.

Review fixes on 2026-10-21:

- Context Menus Settings now synchronizes existing controls in place when preferences or first shortcuts change, preserving keyboard focus and scroll position. The offscreen Settings check verifies that a revision retains the focused checkbox and its widget identity.
- The Settings sidebar now shows General, Keyboard Shortcuts, and Context Menus in 16px text. The offscreen Settings check verifies all three labels and the font size.
- The desktop explicitly dismisses a live Folder Items menu before its models are torn down. Non-pointer requests with an invalid cursor row now use the empty-space target and viewport anchor.
- Direct Cargo checks, both isolated offscreen scripts, and `make check` pass after these fixes.

Owner acceptance on 2026-10-21: the owner accepted the guided macOS acceptance for visual parity, launch responsiveness, pointer and platform invocation, native shortcut notation, Settings customization and persistence, reset confirmations, and both Browsers. All checklist items are checked and the full gate passes. P4-M10 is Done.
