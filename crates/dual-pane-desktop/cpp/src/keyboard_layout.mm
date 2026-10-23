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

auto current_native_mouse_event() -> NativeMouseEvent {
    NSEvent *event = [NSApp currentEvent];
    if (event == nil)
        return NativeMouseEvent::Unknown;
    switch (event.type) {
    case NSEventTypeLeftMouseDown:
        return NativeMouseEvent::PrimaryPress;
    case NSEventTypeLeftMouseDragged:
        return NativeMouseEvent::PrimaryDrag;
    case NSEventTypeLeftMouseUp:
        return NativeMouseEvent::PrimaryRelease;
    case NSEventTypeRightMouseDown:
        return NativeMouseEvent::SecondaryPress;
    case NSEventTypeRightMouseDragged:
        return NativeMouseEvent::SecondaryDrag;
    case NSEventTypeRightMouseUp:
        return NativeMouseEvent::SecondaryRelease;
    default:
        return NativeMouseEvent::Unknown;
    }
}

}
