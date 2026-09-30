#include "dual_pane_desktop/desktop_window.hpp"

#include "dual-pane-desktop/src/listing_model.cxxqt.h"

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtWidgets/QApplication>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>
#include <QtWidgets/QTreeView>

#include <utility>

namespace dual_pane_desktop {

namespace {

constexpr int initial_window_width = 900;
constexpr int initial_window_height = 600;

} // namespace

auto run_desktop(::rust::Box<PaneStartup> startup) -> int {
    int argument_count = 1;
    char application_name[] = "dual-pane";
    char *arguments[] = {application_name, nullptr};
    QApplication application(argument_count, arguments);

    // Declared in this order so that each child is destroyed, and detached
    // from its parent, before the window that it belongs to.
    QMainWindow window;
    ListingModel model;
    QTreeView view;
    QLabel status;

    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.resize(initial_window_width, initial_window_height);

    view.setUniformRowHeights(true);
    view.setRootIsDecorated(false);
    view.setHeaderHidden(true);
    view.setModel(&model);
    window.setCentralWidget(&view);
    window.statusBar()->addWidget(&status, 1);

    QObject::connect(&model, &ListingModel::statusTextChanged, &status, [&status, &model]() -> void { status.setText(model.getStatusText()); });
    model.start(std::move(startup));

    window.show();
    window.raise();
    window.activateWindow();

    return QApplication::exec();
}

} // namespace dual_pane_desktop
