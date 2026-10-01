#ifndef dual_pane_DESKTOP_DESKTOP_WINDOW_HPP
#define dual_pane_DESKTOP_DESKTOP_WINDOW_HPP

#include "rust/cxx.h"

namespace dual_pane_desktop {

struct BrowserStartup;

[[nodiscard]] auto run_desktop(::rust::Box<BrowserStartup> startup) -> int;
void schedule_gui_drain();

} // namespace dual_pane_desktop

#endif // dual_pane_DESKTOP_DESKTOP_WINDOW_HPP
