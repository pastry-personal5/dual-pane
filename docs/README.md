# Documentation index

Status: Active

This index lists the design and process docs, one line each. Read only the doc you need. The rules for writing docs are in [AGENTS.md](../AGENTS.md) under "Documentation".

| Doc | What it covers |
|-----|----------------|
| [development-process.md](development-process.md) | Phases, milestones, IDs (`P1-M2`), definition of done, where plans live, and doc status values |
| [architecture.md](architecture.md) | Active Clean Architecture with MVVM presentation and command-based input, technical stack, crate boundaries, threading, file-operation safety, and testing. |
| [product-behavior.md](product-behavior.md) | Active product scope, Favorites identity, file-operation safeguards, session recovery, Notices startup policy, and exclusions. |
| [hidden-files-research.md](hidden-files-research.md) | Research and proposed bounded policy for hiding macOS housekeeping artifacts without hiding intentional dotfiles. |
| [ux-terms.md](ux-terms.md) | Canonical component and action names, code mappings, and naming exemptions. |
| [ux-information-architecture.md](ux-information-architecture.md) | Current Standard Layout and planned Phase 3 component containment. |
| [ux-gui.md](ux-gui.md) | Version 2.0.0 interaction table and planned Phase 3 tabs, Favorites, commands, operation UI, Notices, sort, and refresh rules. |
| [planned-repository-architecture.md](planned-repository-architecture.md) | Planned workspace tree, crate dependency and ownership rules, and repository naming conventions. |
| [roadmap.md](roadmap.md) | Every phase, one line each, and which phase is active. |
| [phase-3/phase-3.md](phase-3/phase-3.md) | Active Phase 3 goal, exit criteria, and milestone sequence for tabs, file operations, session recovery, and watching. |
| [phase-3/milestone-01-overview.md](phase-3/milestone-01-overview.md) | P3-M1 scope and executable completion checklist. |
| [phase-3/milestone-01-architecture.md](phase-3/milestone-01-architecture.md) | P3-M1 naming and bridge migration approach. |
| [phase-3/milestone-02-overview.md](phase-3/milestone-02-overview.md) | P3-M2 scope and checklist for Phase 3 configuration, defaults, Favorites, settings, and persistent storage. |
| [phase-3/milestone-02-architecture.md](phase-3/milestone-02-architecture.md) | P3-M2 ownership and SQLite storage design for Favorites, folder sort memory, future sessions, and canonical actions. |
| [phase-3/milestone-03-overview.md](phase-3/milestone-03-overview.md) | P3-M3 scope and executable checklist for the revised static Standard Layout. |
| [phase-3/milestone-03-architecture.md](phase-3/milestone-03-architecture.md) | P3-M3 native presentation composition and deferred-behavior boundaries. |
| [phase-3/milestone-04-overview.md](phase-3/milestone-04-overview.md) | P3-M4 recorded decisions, scenario trace, and executable completion checklist. |
| [phase-3/milestone-04-architecture.md](phase-3/milestone-04-architecture.md) | P3-M4 sources of truth and P3-M5–P3-M10 implementation handoffs. |
| [phase-3/milestone-05-overview.md](phase-3/milestone-05-overview.md) | P3-M5 scope and executable checklist for tabbed Qt-free workspace state, Favorites, sort, and refresh. |
| [phase-3/milestone-05-architecture.md](phase-3/milestone-05-architecture.md) | P3-M5 ownership, state transitions, request routing, and implementation sequence. |
| [phase-3/milestone-06-overview.md](phase-3/milestone-06-overview.md) | P3-M6 scope and executable checklist for the tabbed desktop interface, Favorites, sort, focus, and selection. |
| [phase-3/milestone-06-architecture.md](phase-3/milestone-06-architecture.md) | P3-M6 projection, adapter, Qt Widgets, runtime, and verification approach. |
| [phase-3/milestone-07-overview.md](phase-3/milestone-07-overview.md) | P3-M7 scope and completion checklist for safe Qt-free file-operation workflow. |
| [phase-3/milestone-07-architecture.md](phase-3/milestone-07-architecture.md) | P3-M7 operation state, request, decision, and verification boundaries. |
| [phase-3/milestone-08-overview.md](phase-3/milestone-08-overview.md) | P3-M8 recorded decisions and executable checklist for native file operations, Operation Panels, Notices, refresh, and file opening. |
| [phase-3/milestone-08-architecture.md](phase-3/milestone-08-architecture.md) | P3-M8 P3-M7 contract extensions, native gateway, safety journal, presentation, and verification approach. |
| [phase-3/milestone-09-overview.md](phase-3/milestone-09-overview.md) | P3-M9 decisions and executable checklist for session saving, restoration, Waiting timeout, and startup Notices. |
| [phase-3/milestone-09-architecture.md](phase-3/milestone-09-architecture.md) | P3-M9 launch sequence, timeout protection, restoration and discard rules, storage, and verification approach. |
| [phase-3/milestone-10-overview.md](phase-3/milestone-10-overview.md) | P3-M10 decisions, scope, and executable checklist for watching open locations and automatic refresh. |
| [phase-3/milestone-10-architecture.md](phase-3/milestone-10-architecture.md) | P3-M10 watch lifecycle, read coalescing, native and fallback monitoring, feedback, and verification approach. |
| [phase-3/milestone-11-overview.md](phase-3/milestone-11-overview.md) | P3-M11 scope and executable checklist for native macOS Quick Look. |
| [phase-3/milestone-11-architecture.md](phase-3/milestone-11-architecture.md) | P3-M11 Quick Look panel ownership, held-Space lifecycle, and verification approach. |
| [phase-3/milestone-12-overview.md](phase-3/milestone-12-overview.md) | P3-M12 scope and executable checklist for workspace visual clarity, Sidebar feedback, and summary wording. |
| [phase-3/milestone-12-architecture.md](phase-3/milestone-12-architecture.md) | P3-M12 presentation-only visual-polish and verification approach. |
| [phase-3/milestone-13-overview.md](phase-3/milestone-13-overview.md) | P3-M13 scope and executable checklist for the Settings Window shell. |
| [phase-3/milestone-13-architecture.md](phase-3/milestone-13-architecture.md) | P3-M13 modal Settings entry, availability, and desktop-boundary design. |
| [phase-3/milestone-14-overview.md](phase-3/milestone-14-overview.md) | P3-M14 decision gate and checklist for basic settings and keyboard shortcuts. |
| [phase-3/milestone-14-architecture.md](phase-3/milestone-14-architecture.md) | P3-M14 application-owned settings and shortcut-editor delivery approach. |
| [phase-3/milestone-15-overview.md](phase-3/milestone-15-overview.md) | P3-M15 decision gate and executable checklist for a macOS-housekeeping display setting. |
| [phase-3/milestone-15-architecture.md](phase-3/milestone-15-architecture.md) | P3-M15 application-owned filtering, refresh, persistence, and desktop-worker approach. |
| [phase-3/changelog.md](phase-3/changelog.md) | Phase 3 decisions and plan changes. |
| [phase-3/open-items.md](phase-3/open-items.md) | Unresolved Phase 3 design and product questions, deferred verification, and the milestone or phase that must settle each. |
| [archive/phases/phase-2/phase-2.md](archive/phases/phase-2/phase-2.md) | Completed Phase 2 goal, exit criteria, and milestone sequence. |
| [archive/phases/phase-2/milestone-01-overview.md](archive/phases/phase-2/milestone-01-overview.md) | Completed P2-M1 interaction-decision scope and acceptance evidence. |
| [archive/phases/phase-2/milestone-01-architecture.md](archive/phases/phase-2/milestone-01-architecture.md) | Completed P2-M1 documentation-only approach and implementation boundary. |
| [archive/phases/phase-2/milestone-02-overview.md](archive/phases/phase-2/milestone-02-overview.md) | Completed P2-M2 scope and completion checklist for the Qt-free two-Browser workspace. |
| [archive/phases/phase-2/milestone-02-architecture.md](archive/phases/phase-2/milestone-02-architecture.md) | Completed P2-M2 application-state, input/output, and testing plan. |
| [archive/phases/phase-2/milestone-03-overview.md](archive/phases/phase-2/milestone-03-overview.md) | Completed P2-M3 Standard Layout scope and completion checklist. |
| [archive/phases/phase-2/milestone-03-architecture.md](archive/phases/phase-2/milestone-03-architecture.md) | Completed P2-M3 shared desktop coordinator and native layout design. |
| [archive/phases/phase-2/milestone-04-overview.md](archive/phases/phase-2/milestone-04-overview.md) | Completed P2-M4 scope, completion checklist, and acceptance evidence. |
| [archive/phases/phase-2/milestone-04-architecture.md](archive/phases/phase-2/milestone-04-architecture.md) | Completed P2-M4 Browser-local Folder Items runtime and verification approach. |
| [archive/phases/phase-2/changelog.md](archive/phases/phase-2/changelog.md) | Completed Phase 2 owner decisions and plan changes. |
| [release-notes/1.0.0.md](release-notes/1.0.0.md) | Concise release notes for version 1.0.0. |
| [release-notes/2.0.0.md](release-notes/2.0.0.md) | Concise release notes for version 2.0.0. |
| [archive/phases/phase-1/phase-1.md](archive/phases/phase-1/phase-1.md) | Completed Phase 1 goal, exit criteria, and milestones. |
| [archive/phases/phase-1/milestone-02-overview.md](archive/phases/phase-1/milestone-02-overview.md) | Archived P1-M2 scope, checklist, and evidence for the Cargo/Qt Widgets stack spike. |
| [archive/phases/phase-1/milestone-02-architecture.md](archive/phases/phase-1/milestone-02-architecture.md) | Archived P1-M2 workspace, bridge, Qt window, and implementation plan. |
| [archive/phases/phase-1/milestone-03-overview.md](archive/phases/phase-1/milestone-03-overview.md) | Archived P1-M3 scope, decisions, and checklist for the Qt-free inner crates. |
| [archive/phases/phase-1/milestone-03-architecture.md](archive/phases/phase-1/milestone-03-architecture.md) | Archived P1-M3 domain, application, adapter, and test plan. |
| [archive/phases/phase-1/milestone-04-overview.md](archive/phases/phase-1/milestone-04-overview.md) | Archived P1-M4 scope, decisions, and checklist for Qt worker delivery. |
| [archive/phases/phase-1/milestone-04-architecture.md](archive/phases/phase-1/milestone-04-architecture.md) | Archived P1-M4 list model, Browser session, runtime, and synthetic-source plan. |
| [archive/phases/phase-1/milestone-05-overview.md](archive/phases/phase-1/milestone-05-overview.md) | Archived P1-M5 scope, GUI behavior, decisions, checklist, and evidence. |
| [archive/phases/phase-1/milestone-05-architecture.md](archive/phases/phase-1/milestone-05-architecture.md) | Archived P1-M5 directory gateway, selection, Qt UI, and test plan. |
| [archive/](archive/README.md) | Finished or superseded docs. Skip unless you need history. |
