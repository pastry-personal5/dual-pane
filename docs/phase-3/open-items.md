# Phase 3 open items

Status: Active

Unresolved design and product questions, and deferred verification, found during Phase 3 reviews. Each item names the milestone or phase that must settle it; record the decision in the [phase changelog](changelog.md) and remove the item here once it is resolved.

## P3-M8 native behavior is unverified

Target: a later phase, not yet planned

The owner marked P3-M8 Done on 2026-10-02 without its human check. Its file commands, inline name editors, confirmation windows, Operation Panels and Decision Cards, Missing Folder Overlay, and accessibility behavior are implemented, and their logic has automated tests, but nobody has verified them in the native interface. The [deferred human check](milestone-08-overview.md#deferred-human-check) lists what to verify and which checklist items stay unchecked. Fixes and enhancements found during that check belong to the same later phase.

## Refreshes could keep cancelling each other

Target: [P3-M10](phase-3.md#p3-m10-watch-open-locations)

Each `Event::LocationInvalidated` cancels and restarts any read already running for that folder. Once the P3-M10 watcher exists, a folder that changes constantly, such as one receiving an active download, might never finish refreshing. Nothing sends this event yet, and the behavior predates the 2026-10-02 review changes.

Fixing it is a design choice:

- merge repeated invalidations for the same location, or
- re-read once the current read finishes.
