#import <AppKit/AppKit.h>

#include "dual_pane_desktop/keyboard_layout.hpp"

namespace dual_pane_desktop {

auto unshifted_key_character(std::uint32_t native_virtual_key) -> int {
    NSEvent *event = [NSApp currentEvent];
    if (event == nil || event.type != NSEventTypeKeyDown || event.keyCode != native_virtual_key)
        return 0;
    NSString *characters = [event charactersByApplyingModifiers:0];
    if (characters.length != 1)
        return 0;
    const unichar character = [characters characterAtIndex:0];
    return character <= 0x7f ? character : 0;
}

auto current_native_event_snapshot() -> NativeEventSnapshot {
    NSEvent *event = [NSApp currentEvent];
    if (event == nil)
        return {};
    NativeEventSnapshot snapshot;
    snapshot.type = static_cast<int>(event.type);
    snapshot.modifiers = static_cast<std::uint64_t>(event.modifierFlags);
    snapshot.event_number = static_cast<std::int64_t>(event.eventNumber);
    switch (event.type) {
    case NSEventTypeLeftMouseDown:
        snapshot.mouse = NativeMouseEvent::PrimaryPress;
        break;
    case NSEventTypeLeftMouseDragged:
        snapshot.mouse = NativeMouseEvent::PrimaryDrag;
        break;
    case NSEventTypeLeftMouseUp:
        snapshot.mouse = NativeMouseEvent::PrimaryRelease;
        break;
    case NSEventTypeRightMouseDown:
        snapshot.mouse = NativeMouseEvent::SecondaryPress;
        break;
    case NSEventTypeRightMouseDragged:
        snapshot.mouse = NativeMouseEvent::SecondaryDrag;
        break;
    case NSEventTypeRightMouseUp:
        snapshot.mouse = NativeMouseEvent::SecondaryRelease;
        break;
    default:
        break;
    }
    if (snapshot.mouse != NativeMouseEvent::Unknown)
        snapshot.button_number = static_cast<int>(event.buttonNumber);
    return snapshot;
}

auto current_native_mouse_event() -> NativeMouseEvent { return current_native_event_snapshot().mouse; }

}
