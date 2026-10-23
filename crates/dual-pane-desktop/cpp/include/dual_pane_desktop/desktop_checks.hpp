#ifndef dual_pane_DESKTOP_DESKTOP_CHECKS_HPP
#define dual_pane_DESKTOP_DESKTOP_CHECKS_HPP

#include "dual_pane_desktop/gui_common.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop {
class WorkspaceBridge;
class FolderItemsListModel;
} // namespace dual_pane_desktop
namespace dual_pane_desktop::gui {
class Browser;

/// Exercises the owned non-blocking native menu lifecycle without starting a
/// workspace or observing the global clipboard.
auto run_folder_items_context_menu_checks(QApplication &app) -> int;

/// Exercises real Qt widgets without a display. The repository check script
/// launches this path with Qt's offscreen platform plugin.
auto run_settings_window_checks(QApplication &app) -> int;

/// Exercises the keyboard focus rule for clipboard keys with real widgets and
/// no display. Keys reach Qt's shortcut map the way a platform key press does.
/// Folder Items commands only count, so nothing is published to the system
/// clipboard; Qt's offscreen platform keeps text copies in process, and the
/// check fails if the system clipboard changed anyway.
auto run_text_editor_focus_checks(QApplication &app) -> int;

/// Exercises path entry against the real workspace and read workers using a
/// disposable profile. Schedules checks and exits the running event loop.
void schedule_path_entry_checks(QMainWindow &window, WorkspaceBridge &bridge, const std::array<Browser *, 2> &browsers, FolderItemsListModel &left_model);

/// Exercises the private payload, typed target routing, one-use acceptance,
/// stale-state rejection, and real Copy/Move completion with temporary files.
void schedule_file_drag_drop_checks(QMainWindow &window, const std::array<Browser *, 2> &browsers, FolderItemsListModel &left_model, FolderItemsListModel &right_model);

/// Exercises the Application Title Row and its close button with real widgets
/// and no display.
auto run_application_title_row_checks(QApplication &app) -> int;

/// Exercises the single-instance activation target with real modal and
/// waiting widgets. The check keeps native application activation disabled;
/// foregrounding itself remains an interactive macOS acceptance item.
auto run_instance_activation_checks(QApplication &app) -> int;

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_DESKTOP_CHECKS_HPP
