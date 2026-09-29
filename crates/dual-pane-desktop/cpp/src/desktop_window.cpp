#include "dual_pane_desktop/desktop_window.hpp"

#include <QtCore/QString>
#include <QtWidgets/QApplication>
#include <QtWidgets/QMainWindow>

namespace dual_pane_desktop {

auto run_desktop() -> int {
    int argument_count = 1;
    char application_name[] = "dual-pane";
    char *arguments[] = {application_name, nullptr};
    QApplication application(argument_count, arguments);
    QMainWindow window;

    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.show();
    window.raise();
    window.activateWindow();

    return QApplication::exec();
}

} // namespace dual_pane_desktop

extern "C" auto dual_pane_run_desktop() -> int {
    return dual_pane_desktop::run_desktop();
}
