#ifndef DUAL_PANE_DESKTOP_DESKTOP_WINDOW_HPP
#define DUAL_PANE_DESKTOP_DESKTOP_WINDOW_HPP

namespace dual_pane_desktop {

[[nodiscard]] auto run_desktop() -> int;

} // namespace dual_pane_desktop

extern "C" [[nodiscard]] auto dual_pane_run_desktop() -> int;

#endif // DUAL_PANE_DESKTOP_DESKTOP_WINDOW_HPP
