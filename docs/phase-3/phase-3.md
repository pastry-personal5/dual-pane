# Phase 3: Tabbed File Management and Durable Workspace

Status: Active
Goal: Make each Browser a durable, independently tabbed workspace that can safely manage files between Browsers and stay current with file-system changes.

## Exit criteria

- Both Browsers support independent tab sets, active tabs, tab history, and tab-local browsing state.
- One level of Favorite Groups and Favorite Items can be edited in the Sidebar and retains its hierarchy and order during the session and across relaunches.
- Copy, move, rename, directory creation, Trash, permanent deletion, and default-application opening work with every safeguard in [product-behavior.md](../product-behavior.md#file-operation-safeguards).
- A Folder Pane cursor item previews through native macOS Quick Look without creating a built-in viewer.
- Relaunch restores each Browser's tabs, active tab, locations, location-shared folder sort, the active Browser, and the workspace window's frame, zoomed or full-screen state, and splitter positions; a tab whose saved location cannot be restored is discarded without preventing the remaining session from restoring. Text filtering is outside Phase 3.
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

Status: Done
Goal: Refine the workspace experience and supporting structure before adding Phase 3 functionality.
Plan: [overview](milestone-03-overview.md), [architecture](milestone-03-architecture.md)

### P3-M4: Decide workspace interactions

Status: Done
Goal: Record the tab, Favorites, selection, file-operation, session-restoration, sort, Notices, and automatic-refresh interactions that later milestones implement.
Plan: [overview](milestone-04-overview.md), [architecture](milestone-04-architecture.md)

### P3-M5: Model tabbed workspace state

Status: Done
Goal: Add Qt-free tab and Favorites state, tab history, location-shared sort, cursor, selection, and Folder Items refresh behavior; keep back/forward history session-only.
Plan: [overview](milestone-05-overview.md), [architecture](milestone-05-architecture.md)

### P3-M6: Deliver tabbed browser UI

Status: Done
Goal: Render and operate independent Browser tab sets, Sidebar Favorites, sort controls, and their agreed focus and selection behavior in the desktop interface.
Plan: [overview](milestone-06-overview.md), [architecture](milestone-06-architecture.md)

### P3-M7: Establish safe file-operation workflow

Status: Done
Goal: Model file-operation intents, captured source and destination, decisions, progress, cancellation, and destructive-operation confirmation outside the desktop framework.
Plan: [overview](milestone-07-overview.md), [architecture](milestone-07-architecture.md)

### P3-M8: Execute file operations and open files

Status: Done
Goal: Deliver macOS-backed safe file operations, Operation Panels and Decision Cards, Notices summaries, and regular-file opening with the default application.
Plan: [overview](milestone-08-overview.md), [architecture](milestone-08-architecture.md)

### P3-M9: Persist and restore the workspace

Status: Done
Goal: Persist coalesced workspace-session snapshots and restore valid saved Browser tabs, fallback tabs, the window layout, and Notices preferences at launch.
Plan: [overview](milestone-09-overview.md), [architecture](milestone-09-architecture.md)

### P3-M11: Native Quick Look

Status: Active
Goal: Preview the Folder Pane cursor item through a tap-or-held-Space macOS Quick Look gesture.
Plan: [overview](milestone-11-overview.md), [architecture](milestone-11-architecture.md)
Notes: This milestone deliberately runs before P3-M10 and is active again for follow-up work. Its prior automated gate and native human check passed.

### P3-M10: Watch open locations

Status: Planned
Goal: Watch open tab locations, including inactive tabs, coalesce invalidations, and refresh listings without losing matching selection or cursor state.
Plan: [overview](milestone-10-overview.md), [architecture](milestone-10-architecture.md)
