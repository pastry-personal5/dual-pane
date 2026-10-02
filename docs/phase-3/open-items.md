# Phase 3 open items

Status: Active

Unresolved design and product questions found during Phase 3 reviews. Each item names the milestone that must settle it; record the decision in the [phase changelog](changelog.md) and remove the item here once it is resolved.

## Refreshes could keep cancelling each other

Target: [P3-M10](phase-3.md#p3-m10-watch-open-locations)

Each `Event::LocationInvalidated` cancels and restarts any read already running for that folder. Once the P3-M10 watcher exists, a folder that changes constantly, such as one receiving an active download, might never finish refreshing. Nothing sends this event yet, and the behavior predates the 2026-10-02 review changes.

Fixing it is a design choice:

- merge repeated invalidations for the same location, or
- re-read once the current read finishes.
