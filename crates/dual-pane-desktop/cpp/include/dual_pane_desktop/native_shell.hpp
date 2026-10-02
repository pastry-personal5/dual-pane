#ifndef dual_pane_DESKTOP_NATIVE_SHELL_HPP
#define dual_pane_DESKTOP_NATIVE_SHELL_HPP

#include "rust/cxx.h"

#include <cstdint>

namespace dual_pane_desktop {

// Each path is the item's exact native bytes.
[[nodiscard]] auto move_to_trash(::rust::Slice<const std::uint8_t> path, ::rust::Vec<std::uint8_t> &trashed) -> bool;
[[nodiscard]] auto is_bundle(::rust::Slice<const std::uint8_t> path) -> bool;
[[nodiscard]] auto open_with_default_application(::rust::Slice<const std::uint8_t> path) -> bool;

} // namespace dual_pane_desktop

#endif // dual_pane_DESKTOP_NATIVE_SHELL_HPP
