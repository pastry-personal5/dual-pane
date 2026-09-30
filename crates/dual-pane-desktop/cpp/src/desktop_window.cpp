#include "dual_pane_desktop/desktop_window.hpp"

#include "dual-pane-desktop/src/listing_model.cxxqt.h"

#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QThread>
#include <QtWidgets/QApplication>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>
#include <QtWidgets/QTreeView>

#include <atomic>
#include <mutex>
#include <utility>

namespace dual_pane_desktop {

namespace {

constexpr int initial_window_width = 900;
constexpr int initial_window_height = 600;

class DrainScheduler final : public QObject {
  public:
    explicit DrainScheduler(ListingModel *model) : model_(model) {}

    void schedule() {
        // NOLINTNEXTLINE(readability-redundant-lambda-parameter-list)
        QMetaObject::invokeMethod(this, [this]() -> void {
            if (scheduled_) {
                return;
            }
            scheduled_ = true;
            drain_one(); }, Qt::QueuedConnection);
    }

  private:
    void drain_one() {
        scheduled_ = false;
        if (model_ != nullptr && model_->drain()) {
            // NOLINTNEXTLINE(readability-redundant-lambda-parameter-list)
            QMetaObject::invokeMethod(this, [this]() -> void { drain_one(); }, Qt::QueuedConnection);
            scheduled_ = true;
        }
    }

    ListingModel *model_;
    bool scheduled_ = false;
};

struct SchedulerState {
    std::mutex mutex;
    DrainScheduler *scheduler = nullptr;
};

auto scheduler_state() -> SchedulerState & {
    static SchedulerState state;
    return state;
}

auto has_run() -> std::atomic_bool & {
    static std::atomic_bool value = false;
    return value;
}

} // namespace

void schedule_gui_drain() {
    auto &state = scheduler_state();
    std::scoped_lock lock(state.mutex);
    if (state.scheduler != nullptr) {
        state.scheduler->schedule();
    }
}

auto run_desktop(::rust::Box<PaneStartup> startup) -> int {
    constexpr int launch_precondition_failed = 2;
    if (!QThread::isMainThread()) {
        return launch_precondition_failed;
    }
    if (QApplication::instance() != nullptr || QCoreApplication::instance() != nullptr) {
        return launch_precondition_failed;
    }
    if (has_run().exchange(true)) {
        return launch_precondition_failed;
    }
    int argument_count = 1;
    char application_name[] = "dual-pane";
    char *arguments[] = {application_name, nullptr};
    QApplication application(argument_count, arguments);

    // Declared in this order so that each child is destroyed, and detached
    // from its parent, before the window that it belongs to.
    QMainWindow window;
    ListingModel model;
    DrainScheduler drain_scheduler(&model);
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
    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = &drain_scheduler;
    }
    model.start(std::move(startup));

    window.show();
    window.raise();
    window.activateWindow();

    const auto status_code = QApplication::exec();
    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = nullptr;
    }
    return status_code;
}

} // namespace dual_pane_desktop
