# P3-M12 architecture: GUI enhancement

Status: Active

P3-M12 is a desktop presentation refinement. It keeps the domain, application, settings worker, persistence schema, command set, and workspace semantics unchanged; only presentation formatting needed for the summary wording crosses into the adapter layer.

## Presentation boundary

Keep the palette and state-priority rules in the Qt desktop composition. Extend the existing shared stylesheet and targeted delegates/widgets rather than adding a general styling framework. Hover must be represented through Qt hover state or existing pointer events, never through a model mutation or an application command. Pointer entry and exit repaint only their affected widgets; they do not change active Browser, selection, cursor, tab, Favorite order, or persisted state.

The active Browser remains the strongest surface. The specified inactive colors apply consistently to its pane frame, rows, text, toolbar rows, and relative-date delegate. The semantic state order is: inline editor/error and selection/focus first, then disabled, then hover, then ordinary active or inactive appearance. Keyboard focus retains its blue indicator at every Browser brightness. This prevents hover from obscuring an error, selected Item, keyboard focus, or a disabled command.

Each Favorite Group has its own `#15171A` `QFrame` container with a one-pixel `#3A4048` border and 6px radius. A separate parent layout supplies an 8px margin on all four sides, so the box does not touch neighboring groups or the Sidebar edge. Inside that container, Sidebar row layouts use zero contents margins and zero row spacing. Favorite Items and their alias editors align flush left with their Favorite Group instead of using a hierarchy indent. Favorite Group labels use the existing font at bold weight; Favorite Items retain regular weight. Each Favorite Item’s full logical row receives neutral-gray hover feedback, not merely its text label. Favorite Group Menu and Add Favorite Item Buttons retain their own blue-accent hover area and suppress parent-row hover while they own the pointer. Folder Items rows, inactive Browser Tabs, and sort controls use restrained neutral-gray hover; Back, Forward, Up, and Main Toolbar controls use blue-accent hover. Existing drag-and-drop and inline edit behavior owns the row while active and suppresses ordinary hover painting where needed. Drag target geometry maps nested rows into the Favorites panel before calculating its indicator.

Replace the text-only Back, Forward, and Up controls with three locally generated monochrome bitmap `QIcon`s. Back and Forward are arrows with visible shafts; Up is a matching vertical shafted arrow. The current sort icons are already generated from `QPixmap`/`QPainter` in `desktop_window.cpp`, rather than loaded assets; use the same pattern, a 16-logical-pixel footprint, and one- and two-device-pixel-ratio pixmaps. `QToolButton` remains icon-only, and its existing object name, enablement, tooltip, accessible name, focus policy, and command routing stay unchanged. Cache normal and disabled variants in-process; browser/window activity continues to be expressed through the surrounding palette, with no filesystem loading, Qt resource file, or build dependency.

## Summary and verification

`BrowserViewModel::Summary` continues to own the count, selected-count, and size values. Change its presenter formatting so `count` is `1 item` for one Item and `N items` otherwise, while `selected` is always a selected count, including zero. The existing CXX-Qt properties continue to carry the two left-side strings, so no domain or application interface changes. Desktop composition places them as one concise left summary in Toolbar Row #2, paints the selected segment muted blue, and mirrors the combined count and selected text in Browser Status Bar. Both remain blank before the first successful listing and retain last-successful values during later loading or failure. The Status Bar retains its loading, error, monitoring, and temporary command-feedback text whenever that is more specific than the ordinary location status. The right size string and its unknown-size behavior remain unchanged.

Add focused presenter tests for no listing, zero, singular, plural, selected, and retained-after-failure wording. Preserve the existing bridge properties (`summaryCountText`, `summarySelectedText`, and `summarySizeText`) and cover their separate propagation through the existing Rust boundary where practical. Hover and style priority are framework presentation behavior: verify non-mutation with existing command/model seams, and verify their visual precedence natively rather than inventing display-dependent pixel snapshots. Use native checks for visual quality, platform hover behavior, Full Keyboard Access, and contrast. The milestone does not add snapshots that depend on a specific display, font rasterizer, or user directory.

## Implementation sequence

1. Update the UX and terms documents, then change presenter summary formatting with focused tests.
2. Add the cached navigation-icon factory, then refine shared Qt styles, Sidebar row layout, delegates, and hover handling while preserving existing object names and event routing.
3. Exercise existing deterministic model/bridge seams, perform the native visual/accessibility check, run the complete gate, and record evidence in the overview.

