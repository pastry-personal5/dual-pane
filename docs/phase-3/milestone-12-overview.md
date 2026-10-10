# P3-M12: GUI enhancement

Status: Active

This milestone makes the delivered workspace clearer, more consistent, and more modern without adding a new file-management workflow. It is a presentation refinement: the application and storage behavior remain authoritative and unchanged. [Architecture](milestone-12-architecture.md) defines the boundary; this checklist defines completion.

## Scope and fixed inputs

Use [Visual layout and accessibility](../ux-gui.md#visual-layout-and-accessibility), [Folder Items fields and display](../ux-gui.md#folder-items-fields-and-display), [Sidebar Favorites](../ux-gui.md#sidebar-favorites), and the current Standard Layout. P3-M10 and P3-M11 are Done before this milestone starts.

In scope: coherent dark-surface hierarchy; stronger inactive-Browser dimming; dark, separately margined Favorite Group containers with flush internal rows; visible non-layout-shifting mouse hover feedback for the controls named below; mirrored Folder Pane summary wording in Toolbar Row #2 and the Browser Status Bar; focus, selected, disabled, empty, error, and inline-edit visual clarity; automated summary and presentation-boundary tests; native macOS visual and accessibility verification; and directly affected documentation. Browser and splitter geometry, column visibility thresholds, and command layout order are fixed inputs.

Out of scope: a Settings Window, new preferences, new file-management commands, altered selection/navigation semantics, new persistent data, changing the application command boundary, or any dependency/build-tool change.

## Decisions recorded 2026-10-10

| Topic | Decision |
|---|---|
| Visual objective | Improve clarity, consistency, hover feedback, and visibility while retaining the established dark workspace and flat edge-to-edge composition. |
| Inactive Browser | Use the dimmer surface, text, selection, and Relative Date values in [ux-gui.md](../ux-gui.md#visual-layout-and-accessibility); selection, error, and focus remain clear. |
| Sidebar | Each Favorite Group is a `#15171A` container with a one-pixel `#3A4048` border, 6px radius, and 8px outer margin on all sides. Its rows and alias editors remain flush with no inter-row spacing. Favorite Group names are bold; Favorite Items are regular. Rows use a full-row neutral-gray `#2B3037` hover surface; Favorite Group Menu and Add Favorite Item Buttons instead use their own `#2F6D9A` blue-accent hover without lighting the parent row. |
| Hover | Every enabled interactive workspace control gets immediate, visible background hover feedback without a geometry change. Folder Items rows, inactive Browser Tabs, and sort controls use subtle neutral-gray hover surfaces; Back, Forward, Up, Main Toolbar Settings, and Notices controls use `#2F6D9A` blue-accent hover. Disabled, selected, inline-edit, and error states retain their stronger semantic state. |
| Navigation controls | Back, Forward, and Up replace their text glyphs with purpose-made monochrome local bitmap `QIcon`s while retaining their existing behavior, tooltip, and accessible name. Back and Forward are shafted arrows; Up is a matching vertical shafted arrow. The checked-in desktop source generates and caches normal and disabled HiDPI pixmaps; no runtime asset lookup, resource file, or build-step change is introduced. |
| Focus | Keyboard focus keeps a clear blue indicator even when the owning Browser is inactive. |
| Summary | After a first successful listing, Folder Pane Toolbar Row #2 and Browser Status Bar both display a grammatically inflected item count followed by `M selected`: `1 item 1 selected`, but `0 items` and `N items` otherwise. The Row #2 selected segment uses muted blue text. They stay blank before that first listing and retain last-successful values later. Loading, error, monitoring, and command feedback still takes precedence in the Status Bar. The right-hand size behavior is unchanged. |

## Completion checklist

- [ ] The desktop palette and widget styling implement the agreed active/inactive hierarchy without changing Browser/splitter geometry, command routing, persisted state, or accessible names and descriptions.
- [ ] The inactive Browser is visibly dimmer in its surfaces, normal text, selection, Relative Date cells, toolbars, and list while its selection, focus, error, and missing-folder states remain legible.
- [ ] Each Favorite Group has a darker `#15171A` modern container with a one-pixel `#3A4048` border, 6px radius, and 8px outer margin on every side. Its rows and alias editors remain flush with no inter-row spacing; Favorite Group names are bold and Favorite Items regular. Hovering an enabled Favorite Group row, Favorite Item, or New Group Button shows neutral-gray feedback, while hovering its menu or add button shows blue-accent feedback without shifting content, changing the activation gesture, or highlighting the parent row.
- [ ] Back, Forward, and Up use cached, purpose-made monochrome local bitmap `QIcon`s rather than text glyphs: Back and Forward are shafted arrows and Up a matching vertical shafted arrow. They preserve their tooltips and accessible names and receive blue-accent hover feedback. The icon factory supplies normal and disabled pixmaps at one and two device-pixel ratios without runtime asset lookup. Enabled Browser Tabs and tab controls, sort controls, Sidebar controls, Folder Items rows, operation controls, and Main Toolbar controls show immediate, consistent hover feedback. Folder Items, inactive Browser Tabs, and sort controls use subtle neutral-gray feedback; Back, Forward, Up, Favorite Group Menu, Add Favorite Item, Settings, and Notices use blue-accent feedback. Existing selected, focus, disabled, drag, inline-editor, and error feedback takes priority.
- [ ] After a first successful listing, Folder Pane Toolbar Row #2 and Browser Status Bar always show `1 item M selected` for one Item and `N items M selected` otherwise, including empty folders and zero selection, with `M selected` in muted blue in Row #2. They stay blank before the first successful listing and retain last-successful values during later loading or failure; loading, error, monitoring, and command feedback keeps precedence in the Status Bar, and selected and folder size totals retain their existing rules.
- [ ] Focus indicators, contrast, tooltips, canonical accessible names, full-keyboard-access traversal, and reduced-motion behavior remain usable. Adapter tests cover no listing, zero, singular, plural, selected, and retained-summary output; the existing model-to-widget properties carry the separate count, selected, and size strings. Any automated desktop check is limited to deterministic object/property or command-routing seams, not pixel snapshots.
- [ ] Native macOS verification covers active/inactive Browser contrast; dark Favorite Group containers, margins, rows, editors, and hover; a neutral-hover and blue-hover control; empty and selected lists; inline edit; disabled controls; narrow overlay; both summary locations; and Full Keyboard Access. Record the macOS version, display scale, each observed state, and pass/fail evidence here.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; update affected documentation and record acceptance evidence before marking P3-M12 Done.

## Acceptance evidence

- 2026-10-10 — Automated implementation evidence: after adding the dark, margined Favorite Group containers and mirroring the concise summary into Browser Status Bars, `make check` passed (formatters, Rust Clippy, C++ clang-tidy, and all tests).
- 2026-10-10 — Automated implementation evidence: `make check` passed after the adapter summary tests added explicit `1 item`, `0 items`, and `0 selected` coverage. The Qt desktop target compiled and its 165 non-ignored tests passed.
- 2026-10-10 — Native macOS verification remains pending. This execution environment exposes no screen to the debug executable (`Cannot create window: no screens available`), and the available UI automation surface cannot launch the unbundled binary. Run the checklist above from an interactive macOS desktop before checking the visual items or marking P3-M12 Done.
