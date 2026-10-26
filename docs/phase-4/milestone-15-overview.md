# P4-M15: Sidebar Favorite symbols

Status: Done

This milestone gives Favorite Items small Finder-style SF Symbols while preserving the Sidebar's current groups, names, editing, and gestures. The owner requested this focused addition on 2026-10-26 alongside the active Phase 4 acceptance work.

## Scope

- Choose a symbol from a Favorite's target path, independently of its editable alias. Home, Applications, Desktop, Documents, Downloads, Pictures, and Documents/Screenshots receive distinct symbols; other targets use a folder.
- Load symbols from macOS through AppKit, render them at Sidebar scale, and tint them for the dark Sidebar. Missing system symbols fall back to the generic folder symbol.
- Keep the full Favorite row as the pointer and keyboard target, with the existing accessible name and description. Align the inline alias editor with the new text edge.

## Completion checklist

- [x] Native Favorite rows show the intended built-in symbols beside their aliases, with a generic folder for other paths and a readable fallback if a symbol is unavailable.
- [x] Hover, focus, opening, context menu, drag, alias editing, and accessibility remain usable.
- [x] Update user-facing and UX documentation, then pass `make check` and the relevant native desktop check.

## Acceptance evidence

The 2026-10-26 offscreen Sidebar check confirmed each target-to-symbol mapping, visible Retina-scale pixels from macOS, and the fallback for an unavailable symbol. Review of `FavoriteItemRow` confirmed that its existing click, context menu, drag, focus, and accessible row semantics remain on the same full-row widget; only its painting and text inset changed. The alias editor receives the matching inset. Symbols render once per name and row tints are prepared before painting. Fresh-profile Favorites keep their existing membership and order; Home and Pictures receive distinct symbols if added. `scripts/check-application-title-row.sh` and `make check` passed.
