# P4-M3 architecture

Status: Planned

The Qt Folder Items Filter Control edits application-owned tab state through a typed controller event and renders the resulting view model. The [overview checklist](milestone-03-overview.md#completion-checklist) defines acceptance.

## Desktop handoff

Place the control in the existing Browser hierarchy after the P4-M3 owner review fixes its exact position and focus rule in [UX information architecture](../ux-information-architecture.md) and [UX GUI](../ux-gui.md). `crates/dual-pane-desktop/cpp/src/desktop_window.cpp` owns widget construction and accessibility properties; the CXX-Qt bridge and `browser_session.rs` carry text changes and per-tab view updates. The application never holds a Qt widget or clipboard object.

Apply row updates through the Folder Items model on its GUI thread. The view must use the visible row mapping and revision from P4-M2, and pass that revision with row and selection-dependent gestures so a delayed signal cannot act on a previous filter result. Reuse the M2 bounded worker admission, result validation, and adapter projection. While work runs, show the owner-approved blank list and updating cue without clearing application-owned visit state; block Item and selection-dependent actions. Keep the filter control's own text editing ahead of workspace shortcuts while focused, as the M1 interaction decision requires. Reuse existing status precedence for loading and errors, and give an explicit no-match message rather than claiming the folder is empty.

When Folder Items List has keyboard focus, route an unmodified printable text event other than Space into the active tab's filter and focus its field for subsequent text input. Consume the initiating text exactly once; do not synthesize a second key press. Leave Space, movement, activation, modified shortcuts, and inline editor input with their existing owners. At the owner's request, P4-M3 planning decides whether the initiating text appends to or replaces an existing filter and how text composition is committed; the desktop must not guess from a physical key code when the event supplies text.

Desktop tests focus on signal routing and focus/shortcut precedence; Qt-free tests remain in P4-M2. Native verification covers the actual visual and accessibility result.
