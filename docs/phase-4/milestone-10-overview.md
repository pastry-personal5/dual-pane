# P4-M10: Enhance Folder Items Context Menu

Status: Planned

This milestone replaces the single fixed Folder Items Context Menu with a fast Normal variant, an exact-Shift Comprehensive variant, and persistent visibility controls in a new Context Menus Settings screen. The [architecture plan](milestone-10-architecture.md) defines the typed projection, persistence, and stale-action boundary; this checklist is the definition of done.

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
| Open | Open Item; Show Package Contents | Open Item requires exactly one selected Item. Show Package Contents requires exactly one selected Item whose direct kind is package. |
| Clipboard | Copy Items; Cut Items; Paste Items | Copy and Cut require one or more selected Items. Paste requires a ready confirmed destination and a supported cached clipboard offer; it is eligible on a row or empty space. |
| Other Browser | Copy to Other Browser; Move to Other Browser | One or more selected Items and a ready other-Browser destination are required. |
| Delete | Move to Trash; Delete Permanently | One or more selected Items are required. |

The factory-visible commands are:

| Variant | Factory configuration |
|---|---|
| Normal | Open Item; Copy Items; Cut Items; Paste Items; Copy to Other Browser; Move to Other Browser; Move to Trash |
| Comprehensive | Every command in the table above |

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
- [ ] Add the application-owned eleven-action catalogue and complete Normal and Comprehensive preferences, with fixed group/order, independent visibility, at-least-one validation, no-op behavior, local resets, global reset integration, and immediate settings-save output.
- [ ] Add one typed, observation-bearing menu request that atomically activates the Browser, applies the accepted row-selection rule, and returns either an immutable grouped snapshot plus one-use token or no menu. Row, empty-space, keyboard, accessibility, exact-modifier, both-Browser, and stale-request behavior match the accepted rules.
- [ ] Application-owned projection hides unavailable commands and empty groups, suppresses source actions for empty-space invocation, uses the cached clipboard offer without blocking work, and carries only the first effective shortcut as display metadata. Shortcut edit, removal, reset, and global Reset Settings update later menus and the Settings matrix immediately.
- [ ] Rename and Create, Open, Clipboard, Other Browser, and Delete render in the accepted flat order and obey the action-context table. Package, link, multiple-selection, empty-space, pending-load, failed-folder, filtered-update, journal, clipboard, destination, and same-folder conditions use application policy rather than Qt policy.
- [ ] Menu invocation consumes the one-use token and accepts only an action emitted in that snapshot. It revalidates exactly the Browser, tab, listing, selection, source, destination, and Item identity that action needs; selection, filter result, refresh, tab, destination, or Item changes cannot retarget an editor, navigation, clipboard read, or file operation.
- [ ] One native event path opens Normal or exact-Shift Comprehensive in either Browser without duplicate mouse/context events. It uses deterministic cursor or viewport anchoring, the shared native presentation and distinct accessible names, collapses separators, opens nothing for an empty snapshot, displays native shortcut annotations without registering them, and dispatches at most once after `QMenu::exec` returns.
- [ ] Context Menus Settings appears after Keyboard Shortcuts Settings with the accepted matrix, global scope, independent checkboxes, read-only native shortcut column, inline final-item rejection, settings-unavailable behavior, three scope-specific confirmed resets, accessible names, Full Keyboard Access behavior, and immediate persistence.
- [ ] The settings database gains a transactional schema-11 step that stores exactly both configurations under stable eligible `ActionId` names, seeds fresh and every supported older profile with the accepted defaults, round-trips custom values, rejects extra, unknown, duplicate, malformed, missing, or empty configurations, and preserves the existing corrupt/newer-database recovery behavior.
- [ ] Qt-free application and adapter tests cover defaults, independent edits, no-ops, last-item rejection, local and global resets, action-context availability and grouping, exact modifiers, both Browsers, one-use and stale observations, shortcut changes, settings view models, save/load, every supported migration path, corruption, and relaunch. Filesystem fixtures remain in temporary directories.
- [ ] `scripts/check-settings-window.sh` covers the new page, and a new `scripts/check-folder-items-context-menu.sh` covers both real-widget menus without reading the global clipboard: gesture selection, exact modifier routing, rows and empty space, keyboard/accessibility requests, section collapse, empty results, packages, links, multiple selection, shortcut annotations, one dispatch, dismissal, accessibility, and both Browsers. List the new script in AGENTS.md and README.
- [ ] The owner runs the guided macOS acceptance for visual parity, launch responsiveness, pointer and platform invocation, native shortcut notation, Settings customization and persistence, reset confirmations, and both Browsers; record the result here.
- [ ] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Update affected user and design docs and record all acceptance evidence here before marking P4-M10 Done.

## Acceptance evidence

Planning decisions are complete. Implementation and acceptance are pending.
