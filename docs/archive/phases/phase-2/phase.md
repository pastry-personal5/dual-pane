# Phase 2: Two-Pane Browsing

Status: Done
Goal: Show two side-by-side panes, each browsing its own directory, and let the user navigate either pane independently.

## Exit criteria
- Both panes show real directory listings at the same time.
- Each pane keeps its own location, listing, and selection while the user navigates the other pane.
- The active pane is visible and determines which pane receives pane-specific actions.
- Directory reads stay off the interface thread, and an error in one pane does not replace or disrupt the other pane.
- Mouse and keyboard can invoke the same browsing commands, using concrete key bindings decided in P2-M1.

## Milestones

### P2-M1: Decide two-pane interaction details
Status: Done
Goal: Decide how users activate a pane and which keyboard bindings invoke pane focus and existing navigation commands.
Plan: [overview](milestone-01-overview.md), [architecture](milestone-01-architecture.md)

### P2-M2: Model two independent panes
Status: Done
Goal: Extend the Qt-free workspace and application behavior to own two pane states and one active pane, with independent navigation, selection, and listing-result handling.
Plan: [overview](milestone-02-overview.md), [architecture](milestone-02-architecture.md)

### P2-M3: Display and activate both panes
Status: Done
Goal: Show two real listings, display each Folder Pane Toolbar's current folder name, reflect the active pane, and route pointer and keyboard input through the agreed interaction model.
Plan: [overview](milestone-03-overview.md), [architecture](milestone-03-architecture.md)

### P2-M4: Verify cross-pane independence and recovery
Status: Done
Goal: Verify that navigation, loading, errors, and late listing results in one pane leave the other pane usable.
Plan: [overview](milestone-04-overview.md), [architecture](milestone-04-architecture.md)

## Scope boundary

Phase 2 is limited to two-pane browsing. Tabs, file operations, session recovery, automatic watching, and other remaining MVP features are not included in this phase.
