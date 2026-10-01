# Phase 2 changelog

Status: Active
This file records owner decisions and changes to the active Phase 2 plan.

## Entries

- 2026-10-01 — Project owner set the Sidebar Splitter and Browser Divider resting color to the inactive Folder Pane border color `#3A4048`.
- 2026-10-01 — Project owner requested minimal margins for both Browsers; the shared native Browser layout now uses zero content margins and zero gaps for a flat appearance.
- 2026-10-01 — Project owner specified `Command+W` to close the current window (and exit the single-window Phase 2 app) and `Command+Q` to quit the app; [mvp.md](../mvp.md#interaction) records the bindings.
- 2026-10-01 — Project owner applied the Browser Divider’s thin resting, hover-border, and drag appearance to the Sidebar Splitter as well.
- 2026-10-01 — Project owner specified the Browser Divider look: three pixels wide, inactive-Browser background at rest, gray hover border, and no hover border during primary-button dragging.
- 2026-10-01 — Project owner added exact Left (parent) and Right (activate selected row) bindings and clarified that the active highlight is the Folder Pane border. The P2-M3 review fixed Command-Up verification, pane/window-active styling, scrollbar activation, and unaffected-pane model resets.
- 2026-10-01 — Completed P2-M3. The desktop boundary now renders the Standard Layout from one shared workspace/runtime coordinator with pane-addressed input and independent browser presentation.
- 2026-10-01 — Finalized Standard Layout terminology: Sidebar, Sidebar Splitter, Left Browser, Browser Divider, Right Browser, their Navigation Pane and Folder Pane contents, and deferred Browser Tabs. [mvp.md](../mvp.md#standard-layout) is canonical.
- 2026-09-30 — Project owner made displaying each Folder Pane Toolbar's current folder name an explicit P2-M3 delivery requirement.
- 2026-09-30 — Project owner added Sidebar Splitter, between Sidebar and Left Browser, and Browser Divider, between Left and Right Browser, to the canonical Standard Layout terminology. [mvp.md](../mvp.md#standard-layout) remains the canonical source.
- 2026-09-30 — Project owner completed the canonical planned component hierarchy: Standard Layout contains Sidebar, Left Browser, and Right Browser; Sidebar contains Drives Group and Favorites Groups; each Browser contains Browser Tabs and a Folder Pane; a Folder Pane contains Folder Pane Toolbar and Folder Items List. [mvp.md](../mvp.md#standard-layout) is the canonical terminology source.
- 2026-09-30 — Completed P2-M2. The Qt-free workspace now owns independent left and right state with explicit pane-addressed inputs, outputs, work, and terminal events; the existing visible pane remains mechanically pinned to `Left` until P2-M3.
- 2026-09-30 — Planned P2-M2. Its Qt-free workspace plan introduces a domain pane-side value, two independent application pane states, one active pane, pane-addressed listing inputs and outputs, and focused application tests. Pane-targeted commands never implicitly activate a pane, so P2-M3 can preserve the canonical gesture order explicitly; the plan adds no dependencies or Qt/CXX-Qt surface changes.
- 2026-09-30 — Project owner ratified P2-M1's interaction model. [mvp.md](../mvp.md#interaction) is its canonical source; AGENTS.md records the bindings as decided. P2-M1 is documentation-only and is complete with its acceptance evidence.
- 2026-09-30 — Project owner approved the Phase 2 goal and milestone sequence. The phase plan and P2-M1 planning documents were created; P2-M3 and P2-M4 have no milestone documents yet.
