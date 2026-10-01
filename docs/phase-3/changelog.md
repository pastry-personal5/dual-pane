# Phase 3 changelog

Status: Active

Chronological record of decisions and plan changes for Phase 3.

## 2026-10-02 — Pre-M6 codebase and docs review

- Hardening for paths P3-M6 and P3-M10 make frequent: a reload reconciles selection, cursor, anchor, and scroll hint against a set of present names, so refreshing a large selected folder is linear instead of quadratic. The settings worker writes a save queued before a `Load` first, so a load reports what storage holds; a confirmed `Reset` still discards saves queued before it. Neither path is reachable from the 2.0.0 window.
- Reordering a tab to its current position is a no-op and emits no `TabsChanged`. Mapping a Folder Items gesture no longer clones the Browser view model. The C++ format and lint scripts also check `settings_glyph.hpp`.
- Docs: the [architecture §5.2](../architecture.md#52-planned-desktop-execution-model) status note now describes the current reader lanes, its lane table renders, and the property-testing note names the approved `proptest`. Completed milestone architecture docs are marked Done, the [repository tree](../planned-repository-architecture.md#repository-layout-and-future-examples) is redrawn with `settings_storage.rs`, and the P3-M6 native-verification step matches the automated-first rule.

## 2026-10-02 — Item Icon Column

- The owner added an [Item Icon Column](../ux-gui.md#folder-items-fields-and-display) as the first Folder Items column. It shows macOS native icons loaded off the GUI thread, has no header or sort buttons, and is the last column hidden as a Browser narrows. P3-M6 delivers it with the other columns.

## 2026-10-02 — Borderless Folder Pane commands

- The owner removed the border from the Up Button and the eight sort buttons in Folder Pane Toolbar Row #3; [visual layout](../ux-gui.md#visual-layout-and-accessibility) records the rule.

## 2026-10-02 — Pre-M6 review fixes

The owner asked for every finding of the Claude Code review of the P3-M2/P3-M5 code to be fixed before P3-M6 starts.

- Settings safety: the application tracks whether stored settings are loading, loaded, or failed, and saves only after a successful load. A sort chosen before the load result is replayed over the loaded snapshot; Favorites edits wait for the load. After a failed load, edits stay in memory and are never saved, so a damaged database is preserved until an explicit reset. Load failures arrive as a typed `SettingsLoadFailed` event, and a session without a settings worker settles as failed instead of loading forever.
- Quitting flushes unsaved settings with a bounded wait after the Qt event loop exits, as [architecture §5.2](../architecture.md#cancellation-and-shutdown) allows. The settings worker stays available after a failed open, and a reset that cannot move every file restores the moved ones and never falls back to an in-memory database.
- Back and Forward count a pending history step as taken, so repeated presses keep moving; `can_go_back` and `can_go_forward` expose the same rule for P3-M6. The application now uses the domain `BrowserTabs` for tab order and activation instead of a duplicate.
- Each Browser reads folders on four lanes, so a slow folder in one tab no longer delays the Browser's other tabs. Correction to the P3-M5 entry: admission was bounded per Browser, not per tab; it is now bounded per Browser at the pending capacity plus the lane count. Four lanes is provisional until measured.
- Same-folder reloads report one minimal row change, and the Qt list model applies it as row insertions, removals, and `dataChanged` notifications instead of a reset. Navigation and tab switches still replace every row.
- Settings schema version 2 gives `NavigateParent` its documented `Command+Up` default, replacing a stored plain `L` that version 1 used because shortcuts could not name arrow keys. A NULL key now means an unbound action; the migration restores defaults for NULL rows written by version 1, and unknown key text drops only that binding. P3-M6's `CloseTab` migration becomes schema version 3.
- Favorite target probes report `FavoriteProbeOutcome`, and only `Unavailable` removes an Item; P3-M6 still adds the probe runner. Listing events no longer have a panicking address path, settings tests clean up their temporary directories, `main.rs` reuses the settings path helper and runs without persistence when `HOME` is unset, and the application has a property test for tab, selection, and stale-result invariants.
- `scripts/lint-cpp.sh` passes Qt Core's header directory to clang-tidy, matching the compiler, because a CXX-Qt list header includes Qt headers without the `QtCore/` prefix.

## 2026-10-02 — P3-M6 interview decisions

- Development continues with Claude Code. The owner commits changes; the agent does not commit or push unless asked. Codex's existing code is reviewed before P3-M6 implementation starts, and P3-M6 stays one milestone delivered in its four planned steps.
- Verification uses automated tests first; the owner is notified when an automated check fails or cannot establish a checklist item. This replaces the earlier no-display exception question.
- [Folder Items fields](../ux-gui.md#folder-items-fields-and-display): Type shows `[DIR]` for folders, `[LNK]` for every symbolic link, and otherwise the lowercase text after the last `.` (blank without one). Type sort uses that text, replacing P3-M5's kind-only key. Folders have a blank Size cell and are excluded from totals. Relative dates show `now` under one minute or in the future and `N min` under one hour; sizes below 1 KB show whole bytes.
- The owner approved adding the `jiff` crate (MIT/Unlicense) to `dual-pane-adapters` for local-time Exact Date formatting.
- [Browser Tabs](../ux-gui.md#browser-tabs-strip-and-navigation) shrink and elide without scrolling, with at most eight tabs per Browser; New Tab is disabled at the limit. The root folder is labeled with its volume name, falling back to `/`.
- [Add Favorite Item](../ux-gui.md#sidebar-favorites) with an alias already in the group adds nothing and shows a brief inline “Already in this group” cue instead of a Notice.

## 2026-10-01 — P3-M6 second plan review

- The owner kept the Path Edit Control read-only and outside keyboard traversal. Favorite name validation stays in the inline editor without adding a Notice; automatic removal of an unavailable Favorite Item adds a Notice naming it.
- The P3-M6 plan now includes the settings worker's failed-open recovery path, typed storage failures, post-reset reload and stale-save invalidation, and multi-column row updates. Its native-verification item stays unchecked when no screen is available. The owner requested a note that a separate owner decision is needed on whether a documented no-display blocker may satisfy that item.

## 2026-10-01 — P3-M6 plan review and owner decisions

- The owner chose to show M6 storage and Favorites errors in Notices, with actionable storage errors opening Notices even at startup. M6 also delivers the P3-M2 explicit Reset Settings recovery UI; reset requires confirmation, preserves the failed database, and reports its outcome. Operation and restoration messages remain with P3-M8/P3-M9.
- The owner chose to migrate the workspace `Command+W` binding from `CloseWindow` to the new `CloseTab` action while retaining both stable IDs and the `CloseWindow` meaning. A focused Notices window handles `Command+W` locally.
- The [revised M6 plan](milestone-06-overview.md) adds typed Favorite edit rejections, stable-tab validation for delayed gestures, multi-column model work, launch probe sequencing, and direct native verification evidence.

## 2026-10-01 — P3-M5 review fixes

- Pending navigation now receives refreshed sort and settings choices without losing its target; settings load refreshes each open tab once. Right-click preserves a missing range anchor, movement at a row boundary still collapses a multiple selection, and tab/history outputs expose the retained scroll hint.
- Favorite IDs stay unique after deletion within a session, so a late target probe cannot remove a recreated Item. Settings storage rejects an invalid Favorites hierarchy, Folder Items sort keys are cached once per entry, and undelivered read results count against per-Browser runtime admission.

## 2026-10-01 — P3-M5 complete

- Added Qt-free Browser tab structure, tab-local history and view state, Favorites hierarchy validation and edits, location-shared sorting with direct metadata, tokenized refresh routing, and bounded per-tab reader admission.
- The desktop session now loads and saves settings on its serialized worker, seeds fresh Favorites once, and preserves the current two-Browser window while P3-M6 owns the interactive tab and Sidebar controls. The full automated gate passed.

## 2026-10-01 — P3-M5 Favorite name validation decision

- Favorite Group names and Favorite Item Aliases reject empty or whitespace-only text. The corresponding inline editor remains open and shows an error.

## 2026-10-01 — P3-M5 tab, Favorites, and sort decisions

- Closing a Browser's final tab opens a clean tab at the user's home directory. A newly created Favorite Item uses the active folder's current name as its alias.
- Type, Date, and Size sorts use natural Name ascending as their tie-breaker for both directions. Folder-first ordering, link metadata, and unknown values last remain as already decided.

## 2026-10-01 — P3-M5 planning

- Added the next milestone's implementation plan and unchecked completion checklist. It migrates Browser-wide state and read routing to stable tab identity, connects the P3-M2 settings records to Qt-free Favorites and shared sort state, and reserves the interactive UI, session restore, and native watches for their assigned milestones.

## 2026-10-01 — P3-M4 complete

- The remaining focus and range-anchor decisions are documented with scenario evidence. Every P3-M4 checklist item is complete; active-doc links, `git diff --check`, `make check`, and the direct Cargo format, Clippy, and test commands passed. Implementation remains assigned to P3-M5 through P3-M10.

## 2026-10-01 — P3-M4 workspace interview decisions

- The owner settled the remaining [keyboard-focus and range-anchor rules](../ux-gui.md#planned-phase-3-gui-decisions): focus entering any control in a Browser activates that Browser before Browser-specific commands, and Shift-click or Shift-arrow does nothing until a plain selection establishes an anchor. The [scenario trace](milestone-04-overview.md#scenario-trace) records both cases.
- The owner completed the [selection and file-command contract](../ux-gui.md#selection-and-file-commands): fixed range anchor and cursor behavior, confirmed-folder command availability, context-menu entry points, frozen destructive targets, and a Cancel-default Permanent Delete confirmation. Rename and New Folder reject whitespace-only names.
- The owner corrected Permanent Delete Confirmation to show the frozen target count only, without names; Cancel remains the default and deletion requires explicit activation.
- The owner completed [operation result behavior](../ux-gui.md#operation-panels-and-decisions): reveal panels for long work, decisions, and partial or failed results; hide successful and cleanly cancelled panels after about three seconds; close completed panels with `Command+W`; open Notices for quick failures and default-app opening failures. Tab reordering, Favorite Item movement, and panel docking remain drag-only by owner choice.
- The owner completed [refresh behavior](../ux-gui.md#automatic-refresh): `Command+R` is the sole manual entry point and reserves the `RefreshFolder` action ID; unavailable watched folders keep their last successful view and path with an error; affected open tabs refresh after operations; disappeared Items leave selection and a disappeared cursor clears.
- For keyboard focus, Browser Tabs and Favorite Items use arrow keys and Return under Full Keyboard Access, and text editing suppresses file-command shortcuts. The Favorite Item Context Menu remains pointer-only by owner choice.
- The owner set [tab, history, and selection gestures](../ux-gui.md#browser-tabs-strip-and-navigation), [Favorite controls](../ux-gui.md#sidebar-favorites), [file-command entry points](../ux-gui.md#selection-and-file-commands), and [Operation Panels and Decision Cards](../ux-gui.md#operation-panels-and-decisions). The [product safeguards](../product-behavior.md#file-operation-safeguards) retain conflict choices and define the conflict-only apply-to-all option.
- The owner set [fallback restoration and Notices policy](../product-behavior.md#session-recovery), [Notices controls](../ux-gui.md#notices-and-restoration-feedback), and [inactive-tab refresh feedback](../ux-gui.md#automatic-refresh). [Planned containment](../ux-information-architecture.md#planned-phase-3-containment) places the new components while current 2.0.0 layout and gestures stay separate.
- Text filtering was removed from Phase 3 scope and restored session state; location-shared [sort memory and controls](../ux-gui.md#sort-controls-and-remembered-choice) remain. Completed P3-M2 filter references now carry a supersession note. Existing code and persisted action IDs are unchanged; P3-M5/P3-M9 own later compatibility work.

## 2026-10-01 — Canonical terminology before P3-M4

- Recorded shortcut-capable action names and planned component names in [UX terms](../ux-terms.md). Nine new action IDs are reserved in documentation only; the persisted catalogue and tab behavior are unchanged. Renamed the inert native Browser Tabs Strip widget and its object and accessibility names to match the canonical container term.

## 2026-10-01 — P3-M4 plan review

- Renamed the milestone to cover its full workspace-interaction scope and made its decision questions, owner review, scenario evidence, and downstream handoffs explicit. Added gesture collisions, multi-selection and command targeting, user-visible filter rules, hidden-tab refresh, and operation/session error paths to the plan; no behavior decision was made in this review.

## 2026-10-01 — P3-M4 planning

- Added the documentation-first plan for the next milestone. It will settle the user-facing contracts for tabs, file operations, recovery, sort/filter, refresh, and Sidebar Favorite editing before P3-M5 models the new state.

## 2026-10-01 — P3-M3 complete

- Refactored the native desktop shell into static Sidebar, Main Toolbar, Browser, Browser Tabs Strip, Folder Pane, summary-row, command-row, and coordination components without changing the existing model or command interfaces. The full automated gate passed.
- A native launch was attempted after verification, but visual inspection remains deferred because the environment reported `Cannot create window: no screens available`.

## 2026-10-01 — P3-M3 layout decisions

- The static Standard Layout now has inert Browser Tabs Strip bound only to each current folder name, a bottom-anchored Main Toolbar with disabled Settings, and a three-row Folder Pane. Static Sidebar labels remain disconnected from persisted Favorites.
- Folder Pane Toolbar Row #2 reserves summary metrics for P3-M5/P3-M6. Those milestones will show `N items`, optional `X selected`, and selected or folder totals as non-recursive sums of known direct visible Folder Item sizes; unknown totals render `—`, and metrics stay with visible last-successful items across pending or failed navigation. Row #3 has the functional Up Button followed by disabled Name, Type, Date, and Size ascending/descending carets until P3-M6 connects them.

## 2026-10-01 — P3-M2 complete

- Added Qt-free settings values, sort memory, action catalogue, Favorites records, and workspace-session handoff values; the desktop SQLite driver uses a serialized worker and lossless BLOB locations.
- Added temporary-directory coverage for migrations, unsupported schemas and I/O failure, reset preservation, action preload, non-UTF-8 locations, Favorites ordering, LRU eviction, and asynchronous delivery. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed.

## 2026-10-01 — P3-M2 persistent-storage decisions

- SQLite is the sole live configuration store. The application preloads persistent shortcut bindings for every canonical action, including future inactive actions, from the compiled action catalogue.
- Supported older database schemas upgrade automatically and transactionally. Corrupt, unsupported, or unwritable storage shows a recoverable error with an explicit Reset Settings option; reset is never automatic and preserves the failed database for diagnosis.
- Natural Name ascending is retained as an explicit folder-sort choice and occupies one of the 100 LRU entries. The owner approved `rusqlite` linked to system SQLite.

## 2026-10-01 — P3-M2 plan review

- Separated P3-M2's Qt-free persistence records and settings boundary from P3-M5's future Favorites domain state and edit rules, clarified that the SQLite driver itself has no Qt dependency, and added the remaining owner decisions that must be made before implementation.

## 2026-10-01 — Tabs and Favorites persistence decisions

- The owner added durable open tabs and an ordered, one-level Favorite Group → Favorite Item hierarchy. Tab back/forward history stays session-only. Phase 3 includes Sidebar editing. A saved Favorite Item whose target is missing or unavailable at launch is automatically removed after a completed target probe.
- Fresh-profile Favorites are Applications, Desktop, Documents, Screenshots, and Downloads; Screenshots targets `~/Documents/Screenshots` only when it exists.

## 2026-10-01 — Date-column terminology

- Added Relative Date Column and Exact Date Column to [ux-terms.md](../ux-terms.md#canonical-components) as planned GUI components and aligned the planned GUI description with those exact names.

## 2026-10-01 — Planned GUI owner decisions recorded

- Recorded the owner's planned field order and display, direct sort buttons, responsive field hiding and `Expand` overlay, and future shortcut editor in [ux-gui.md](../ux-gui.md#planned-phase-3-gui-decisions). Current GUI behavior remains described separately.

## 2026-10-01 — P3-M2 scope clarified

- Expanded P3-M2 from an initial shortcut-focused preference plan to Phase 3-wide configuration, defaults, settings, and durable storage. Added a shared, 100-location folder sort history and selected SQLite for settings; sorting controls and session restoration stay in their later milestones.

## 2026-10-01 — P3-M2 planning

- Added P3-M2's plan for application-owned preferences, compiled defaults, asynchronous atomic persistence, canonical UpperCamelCase action IDs, and the future shortcut-editor path. `NewFolder` is reserved with the default `Command+Shift+N`; its command and editor remain later work.

## 2026-10-01 — P3-M1 complete

- P3-M1 is complete with its documentation, terminology, link, and automated verification evidence. The native launch attempt is recorded in its overview; visual inspection remains pending a screen-enabled environment.

## 2026-10-01 — Documentation consistency review

- Separated committed product scope from version 2.0.0 availability, clarified current versus deferred Sidebar and Browser Tabs Strip behavior, and aligned active architecture and repository-layout terminology with Browser and Folder Items.

## 2026-10-01 — P3-M1 terminology baseline

- Product behavior moved out of the former active MVP document; UX vocabulary, information architecture, and GUI behavior now have dedicated active documents.
- Browser and Folder Items are the canonical source and bridge vocabulary. Deferred UI remains unrendered.
