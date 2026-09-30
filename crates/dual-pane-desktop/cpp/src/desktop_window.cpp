#include "dual_pane_desktop/desktop_window.hpp"

#include "dual-pane-desktop/src/listing_model.cxxqt.h"

#include <QtCore/QEvent>
#include <QtCore/QItemSelectionModel>
#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtGui/QIcon>
#include <QtGui/QMouseEvent>
#include <QtWidgets/QAbstractItemView>
#include <QtWidgets/QApplication>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>
#include <QtWidgets/QStyle>
#include <QtWidgets/QToolButton>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>

#include <atomic>
#include <mutex>
#include <pthread.h>
#include <utility>

namespace dual_pane_desktop {

namespace {

constexpr int initial_window_width = 900;
constexpr int initial_window_height = 600;
constexpr int pane_margin = 8;
constexpr int pane_spacing = 6;
constexpr auto window_color = "#1B1D21";
constexpr auto surface_color = "#23262B";
constexpr auto text_color = "#ECEFF3";
constexpr auto muted_border_color = "#3A4048";
constexpr auto row_hover_color = "#303640";
constexpr auto selection_color = "#2F6D9A";
constexpr auto selection_text_color = "#FFFFFF";

auto pane_style_sheet() -> QString {
    return QStringLiteral(R"(
        QMainWindow#mainWindow, QWidget#paneRoot, QStatusBar {
            background: %1;
            color: %3;
        }
        QWidget#buttonStrip, QTreeView {
            background: %2;
            color: %3;
        }
        QTreeView {
            border: 1px solid %4;
            outline: none;
            show-decoration-selected: 1;
        }
        QTreeView::item:hover:!selected {
            background: %5;
            color: %3;
        }
        QTreeView::item:selected:active, QTreeView::item:selected:!active {
            background: %6;
            color: %7;
        }
        QToolButton {
            background: %2;
            color: %3;
            border: 1px solid %4;
            padding: 4px;
        }
        QToolButton:hover {
            background: %6;
            color: %7;
            border: 1px solid %6;
        }
        QToolButton:pressed {
            background: %5;
            border: 1px solid %4;
        }
        QLabel {
            color: %3;
        }
    )")
        .arg(QString::fromLatin1(window_color))
        .arg(QString::fromLatin1(surface_color))
        .arg(QString::fromLatin1(text_color))
        .arg(QString::fromLatin1(muted_border_color))
        .arg(QString::fromLatin1(row_hover_color))
        .arg(QString::fromLatin1(selection_color))
        .arg(QString::fromLatin1(selection_text_color));
}

class ListingView final : public QTreeView {
  public:
    explicit ListingView(ListingModel *model) : model_(model) {
        setModel(model_);
        setSelectionMode(QAbstractItemView::SingleSelection);
        setSelectionBehavior(QAbstractItemView::SelectRows);
        setEditTriggers(QAbstractItemView::NoEditTriggers);
        setMouseTracking(true);
        QObject::connect(model_, &ListingModel::selectedRowChanged, this, [this]() -> void { synchronize_selection(); });
    }

  protected:
    auto selectionCommand([[maybe_unused]] const QModelIndex &index, [[maybe_unused]] const QEvent *event) const -> QItemSelectionModel::SelectionFlags override {
        return QItemSelectionModel::NoUpdate;
    }

    auto event(QEvent *event) -> bool override {
        if (event->type() == QEvent::KeyPress) {
            event->accept();
            return true;
        }
        return QTreeView::event(event);
    }

    void mousePressEvent(QMouseEvent *event) override {
        event->accept();
    }

    void mouseReleaseEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            const auto index = indexAt(event->position().toPoint());
            if (index.isValid()) {
                model_->select_row(index.row());
            }
        }
        event->accept();
    }

    void mouseDoubleClickEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            const auto index = indexAt(event->position().toPoint());
            if (index.isValid()) {
                model_->activate_row(index.row());
            }
        }
        event->accept();
    }

  private:
    void synchronize_selection() {
        const auto row = model_->getSelectedRow();
        const auto index = model_->index(row, 0);
        if (row >= 0 && index.isValid()) {
            selectionModel()->select(index, QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
        } else {
            selectionModel()->clearSelection();
        }
    }

    ListingModel *model_;
};

class DrainScheduler final : public QObject {
  public:
    explicit DrainScheduler(ListingModel *model) : model_(model) {}

    void schedule() {
        // Wakes come from the listing worker while drain_one runs on the GUI
        // thread. The atomic exchange coalesces them without a cross-thread
        // access to ordinary state.
        if (scheduled_.exchange(true)) {
            return;
        }
        // NOLINTNEXTLINE(readability-redundant-lambda-parameter-list)
        QMetaObject::invokeMethod(this, [this]() -> void { drain_one(); }, Qt::QueuedConnection);
    }

  private:
    void drain_one() {
        scheduled_.store(false);
        if (model_ != nullptr && model_->drain()) {
            schedule();
        }
    }

    ListingModel *model_;
    std::atomic_bool scheduled_ = false;
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
    if (pthread_main_np() == 0) {
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
    QWidget pane;
    QVBoxLayout pane_layout(&pane);
    QWidget button_strip;
    QHBoxLayout button_layout(&button_strip);
    QToolButton up_button;
    ListingView view(&model);
    QLabel status;

    window.setObjectName(QStringLiteral("mainWindow"));
    pane.setObjectName(QStringLiteral("paneRoot"));
    button_strip.setObjectName(QStringLiteral("buttonStrip"));
    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.resize(initial_window_width, initial_window_height);
    window.setStyleSheet(pane_style_sheet());

    view.setUniformRowHeights(true);
    view.setRootIsDecorated(false);
    view.setHeaderHidden(true);
    const auto standard_up_icon = window.style()->standardIcon(QStyle::SP_ArrowUp);
    up_button.setIcon(QIcon(standard_up_icon.pixmap(up_button.iconSize(), QIcon::Active)));
    up_button.setAccessibleName(QStringLiteral("Up"));
    up_button.setToolTip(QStringLiteral("Up"));
    up_button.setFocusPolicy(Qt::NoFocus);
    button_layout.setContentsMargins(0, 0, 0, 0);
    button_layout.addWidget(&up_button);
    button_layout.addStretch(1);
    pane_layout.setContentsMargins(pane_margin, pane_margin, pane_margin, pane_margin);
    pane_layout.setSpacing(pane_spacing);
    pane_layout.addWidget(&button_strip);
    pane_layout.addWidget(&view, 1);
    window.setCentralWidget(&pane);
    window.statusBar()->addWidget(&status, 1);

    QObject::connect(&model, &ListingModel::statusTextChanged, &status, [&status, &model]() -> void { status.setText(model.getStatusText()); });
    QObject::connect(&up_button, &QToolButton::clicked, &model, [&model]() -> void { model.go_to_parent(); });
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
