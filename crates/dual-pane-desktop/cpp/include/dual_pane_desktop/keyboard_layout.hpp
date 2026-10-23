#pragma once

#include <cstdint>

namespace dual_pane_desktop {

// Returns an ASCII character from the active macOS layout with all modifiers
// removed, or zero when the current native event cannot be translated.
auto unshifted_key_character(std::uint32_t native_virtual_key) -> int;

enum class NativeMouseEvent {
    Unknown,
    PrimaryPress,
    PrimaryDrag,
    PrimaryRelease,
    SecondaryPress,
    SecondaryDrag,
    SecondaryRelease,
};

// Reports the physical AppKit button for the event currently being delivered.
// Synthetic Qt events and unrelated native events return Unknown.
auto current_native_mouse_event() -> NativeMouseEvent;

}
