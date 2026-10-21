#include "dual_pane_desktop/workspace_window.hpp"
#include "dual_pane_desktop/application_activation.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

/// Brings the workspace's current blocking surface forward. The target is
/// chosen again on the next GUI turn because a modal can appear between the
/// application command and the deferred native activation fallback.
void activate_workspace_window(WorkspaceWindow *window, WaitingWindow *waiting, int request, std::function<void(int)> focus_browser, bool allow_native_fallback) {
    const auto current_target = [window, waiting] {
        if (QWidget *modal = QApplication::activeModalWidget())
            return modal;
        return waiting->isVisible() ? static_cast<QWidget *>(waiting) : static_cast<QWidget *>(window);
    };
    QWidget *target = current_target();
    if (target == window && window->isMinimized()) {
        window->setWindowState((window->windowState() & ~Qt::WindowMinimized) | Qt::WindowActive);
        window->show();
    } else {
        target->show();
    }
    target->raise();
    target->activateWindow();
    QTimer::singleShot(0, window, [window, current_target, request, focus_browser = std::move(focus_browser), allow_native_fallback] {
        if (allow_native_fallback && QApplication::applicationState() != Qt::ApplicationActive)
            activate_application();
        QWidget *target = current_target();
        target->raise();
        target->activateWindow();
        if (target == window && request >= 0)
            focus_browser(request);
    });
}

auto scheduler_state() -> SchedulerState & {
    static SchedulerState state;
    return state;
}

auto has_run() -> std::atomic_bool & {
    static std::atomic_bool value = false;
    return value;
}

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
