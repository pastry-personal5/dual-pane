#ifndef DUAL_PANE_DESKTOP_DESKTOP_WINDOW_HPP
#define DUAL_PANE_DESKTOP_DESKTOP_WINDOW_HPP

#include "rust/cxx.h"

namespace dual_pane_desktop {

struct PaneStartup;

[[nodiscard]] auto run_desktop(::rust::Box<PaneStartup> startup) -> int;

} // namespace dual_pane_desktop

#endif // DUAL_PANE_DESKTOP_DESKTOP_WINDOW_HPP
