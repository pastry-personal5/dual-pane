# Phase 3: Tabbed File Management and Durable Workspace

Status: Active
Goal: Make each Browser a durable, independently tabbed workspace that can safely manage files between Browsers and stay current with file-system changes.

## Exit criteria

- Both Browsers support independent tab sets, active tabs, tab history, and tab-local browsing state.
- One level of Favorite Groups and Favorite Items can be edited in the Sidebar and retains its hierarchy and order during the session and across relaunches.
- Copy, move, rename, directory creation, Trash, permanent deletion, and default-application opening work with every safeguard in [product-behavior.md](../product-behavior.md#file-operation-safeguards).
- Relaunch restores each Browser's tabs, active tab, locations, sort/filter state, and the active Browser; a tab whose saved location cannot be restored is discarded without preventing the remaining session from restoring.
- Open locations are watched; changed Folder Items refresh automatically and retain matching selection and cursor state.
- Directory loading, watching, persistence, and file operations keep the interface responsive as required by [architecture.md](../architecture.md#51-threading-and-responsiveness).

## Milestones

### P3-M1: UX documentation and terminology refactor

Status: Done
Goal: Establish the active UX sources of truth and a focused, verified Browser/Folder Items naming baseline.
Plan: [overview](milestone-01-overview.md), [architecture](milestone-01-architecture.md)
Notes: The automated gate passed. Native visual verification is deferred because the execution environment has no screen.

### P3-M2: Configs, Defaults, Settings and persistent storage

Status: Done
Goal: Establish Phase 3 configuration and defaults, application-owned settings, and durable storage contracts for favorites, folder sort memory, and later workspace restoration.
Plan: [overview](milestone-02-overview.md), [architecture](milestone-02-architecture.md)

### P3-M3: UX Design and refactoring

Status: Planned
Goal: Refine the workspace experience and supporting structure before adding Phase 3 functionality.

### P3-M4: Decide tab, operation, and recovery interaction

Status: Planned
Goal: Record the user-facing tab, file-operation, session-recovery, sort/filter, and automatic-refresh interaction model that later milestones implement.

### P3-M5: Model tabbed workspace state

Status: Planned
Goal: Add Qt-free tab and Favorites state, tab history, sort/filter, cursor, selection, and Folder Items refresh behavior; keep back/forward history session-only.

### P3-M6: Deliver tabbed browser UI

Status: Planned
Goal: Render and operate independent Browser tab sets, Sidebar Favorites, and their agreed controls in the desktop interface.

### P3-M7: Establish safe file-operation workflow

Status: Planned
Goal: Model file-operation intents, planning, decisions, progress, cancellation, and destructive-operation confirmation outside the desktop framework.

### P3-M8: Execute file operations and open files

Status: Planned
Goal: Deliver macOS-backed safe file operations, their interface flow, and regular-file opening with the default application.

### P3-M9: Persist and restore the workspace

Status: Planned
Goal: Persist coalesced workspace-session snapshots and restore valid saved Browser tabs and their state at launch.

### P3-M10: Watch open locations

Status: Planned
Goal: Watch open tab locations, coalesce invalidations, and refresh listings without losing matching selection or cursor state.
