# Phase 3 changelog

Status: Active

Chronological record of decisions and plan changes for Phase 3.

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

- Separated committed product scope from version 2.0.0 availability, clarified current versus deferred Sidebar and Browser Tabs behavior, and aligned active architecture and repository-layout terminology with Browser and Folder Items.

## 2026-10-01 — P3-M1 terminology baseline

- Product behavior moved out of the former active MVP document; UX vocabulary, information architecture, and GUI behavior now have dedicated active documents.
- Browser and Folder Items are the canonical source and bridge vocabulary. Deferred UI remains unrendered.
