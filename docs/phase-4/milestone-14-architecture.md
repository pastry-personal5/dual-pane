# P4-M14 architecture

Status: Active

The repair belongs at the native `FolderItemsList` gesture boundary. Application menu policy, snapshot creation, and dispatch remain as designed in [P4-M10](milestone-10-architecture.md); P4-M12 drag admission must still distinguish a physical Control-primary press from a true secondary gesture. The [overview](milestone-14-overview.md) is the acceptance contract.

## Root-cause analysis

The post-M12 code in `cpp/include/dual_pane_desktop/folder_pane.hpp` accepts every `QContextMenuEvent`, then returns without calling `menu_handler_` when `current_native_mouse_event()` says primary press/release or `remapped_primary_press_` is set. `current_native_mouse_event()` reads `[NSApp currentEvent]` in `keyboard_layout.mm`; it is an ambient event, not an identity attached to the Qt context event. A context event caused by a genuine secondary gesture can therefore be dropped if that ambient event is primary or a prior remapped-primary flag is still set. The right-button `mousePressEvent` deliberately has no menu fallback. This explains how both menu variants can disappear together: classification fails before the variant or action snapshot is requested. M10's successful physical acceptance predates this guard. The precise AppKit/Qt event sequence for the reported trackpad tap remains to be measured rather than assumed.

The offscreen Context Menu check sends isolated Qt context events without a controlled native secondary event; the M12 check tests only that a simulated Control-primary remap is suppressed. Both miss the physical-secondary-versus-remapped-primary distinction. The repair must add coverage for the event sequence observed on the owner's MacBook.

## Implementation approach

1. Add an opt-in native gesture trace for the disposable profile. It observes `QMouseEvent` press/release and `QContextMenuEvent` on the real viewport, plus AppKit event type, button number, modifiers, and event identity. It is gated by both `DUAL_PANE_TEST_PROFILE` and `DUAL_PANE_GESTURE_TRACE`, and records no file paths or names.
2. Replace ambient-event-only suppression with gesture state tied to the observed press and a confirmed physical Control-primary origin. The implementation creates suppression only for a Qt right-button press with the mapped Control modifier and an AppKit primary press. A genuine secondary press clears it, while a matching context event remains suppressed across release. An unrelated press or focus loss retires it. No Qt right-button value alone implies a physical primary origin.
3. If the physical trace shows Qt never emits a context event for the two-finger gesture, add a secondary-press fallback that calls the same menu coordinator once. Correlate a later context event with that press so it cannot open a duplicate menu. Preserve the existing keyboard/accessibility route and exact user-modifier classification. Use this fallback only when the measured Qt delivery requires it.
4. Keep the existing application request as the single action-policy path: activate the source Browser, apply the secondary-selection rule, request an immutable menu snapshot, and let the owned `QMenu::popup` session dispatch or dismiss its one-use token. No native branch may construct a file operation or bypass stale-target checks.

## Verification

`desktop_checks.cpp` now delivers press/context/release to actual `FolderItemsList` viewports with simulated native-secondary and Control-primary states. It asserts callback count, Normal versus Comprehensive classification, selected-row and empty-space behavior, state recovery after Control-primary, and no drag or root mutation from secondary input. The two isolated native scripts and `make check` passed on 2026-10-23. Physical trackpad and mouse gestures in a disposable profile remain necessary to close the bug and update the P4-M12/P4-M7 physical-acceptance records.
