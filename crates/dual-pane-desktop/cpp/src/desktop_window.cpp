#include "dual_pane_desktop/desktop_window.hpp"
#include "dual-pane-desktop/src/listing_model.cxxqt.h"

#include <QtCore/QEvent>
#include <QtCore/QItemSelectionModel>
#include <QtCore/QMetaObject>
#include <QtCore/QString>
#include <QtCore/QVariant>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QPaintEvent>
#include <QtGui/QPainter>
#include <QtGui/QShortcut>
#include <QtWidgets/QAbstractItemView>
#include <QtWidgets/QApplication>
#include <QtWidgets/QFrame>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QScrollBar>
#include <QtWidgets/QSplitter>
#include <QtWidgets/QSplitterHandle>
#include <QtWidgets/QStyle>
#include <QtWidgets/QToolButton>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>

#include <atomic>
#include <cstdint>
#include <functional>
#include <mutex>
#include <pthread.h>
#include <utility>

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type)
namespace dual_pane_desktop {
namespace {
constexpr int initial_window_width = 1100, initial_window_height = 680;
constexpr int splitter_width = 3;
constexpr auto window_color = "#1B1D21", surface_color = "#23262B", text_color = "#ECEFF3", border_color = "#3A4048", active_color = "#2F6D9A", inactive_browser_color = "#1E4668", divider_hover_border_color = "#737A84";

enum class ArrowAction : std::uint8_t { None,
                                        Previous,
                                        Next,
                                        Parent,
                                        Activate,
};

constexpr auto arrow_action(int key) -> ArrowAction {
    switch (key) {
    case Qt::Key_Up:
        return ArrowAction::Previous;
    case Qt::Key_Down:
        return ArrowAction::Next;
    case Qt::Key_Left:
        return ArrowAction::Parent;
    case Qt::Key_Right:
        return ArrowAction::Activate;
    default:
        return ArrowAction::None;
    }
}

constexpr auto user_modifiers(Qt::KeyboardModifiers modifiers) -> Qt::KeyboardModifiers {
    // Qt sets KeypadModifier for the physical arrow cluster on macOS. It
    // describes the key's origin, not a modifier held by the user.
    return modifiers & ~Qt::KeypadModifier;
}

static_assert(arrow_action(Qt::Key_Up) == ArrowAction::Previous);
static_assert(arrow_action(Qt::Key_Down) == ArrowAction::Next);
static_assert(arrow_action(Qt::Key_Left) == ArrowAction::Parent);
static_assert(arrow_action(Qt::Key_Right) == ArrowAction::Activate);
static_assert(user_modifiers(Qt::KeypadModifier) == Qt::NoModifier);
static_assert(user_modifiers(Qt::ShiftModifier | Qt::KeypadModifier) == Qt::ShiftModifier);

auto style_sheet() -> QString {
    return QStringLiteral(R"(
      QMainWindow, QWidget#browser, QWidget#sidebar { background:%1; color:%3; }
      QLabel, QLineEdit { color:%3; background:%2; }
      QLineEdit { border:1px solid %4; padding:4px; }
      QFrame#folderPane { background:%1; border:1px solid %4; }
      QFrame#folderPane[paneActive="true"][windowActive="true"] { border:1px solid %5; }
      QTreeView { background:%2; color:%3; border:0; outline:none; }
      QTreeView::item:selected { background:%6; color:white; }
      QTreeView[paneActive="true"][windowActive="true"]::item:selected { background:%5; color:white; }
      QToolButton { background:%2; color:%3; border:1px solid %4; padding:4px; }
      QSplitter::handle { background:%4; }
    )")
        .arg(window_color)
        .arg(surface_color)
        .arg(text_color)
        .arg(border_color)
        .arg(active_color)
        .arg(inactive_browser_color);
}

class ThinSplitterHandle final : public QSplitterHandle {
  public:
    ThinSplitterHandle(Qt::Orientation orientation, QSplitter *parent, const QString &accessible_name) : QSplitterHandle(orientation, parent) { setAccessibleName(accessible_name); }

  protected:
    void enterEvent(QEnterEvent *event) override {
        hovered_ = true;
        update();
        QSplitterHandle::enterEvent(event);
    }

    void leaveEvent(QEvent *event) override {
        hovered_ = false;
        update();
        QSplitterHandle::leaveEvent(event);
    }

    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            dragging_ = true;
            update();
        }
        QSplitterHandle::mousePressEvent(event);
    }

    void mouseReleaseEvent(QMouseEvent *event) override {
        QSplitterHandle::mouseReleaseEvent(event);
        if (event->button() == Qt::LeftButton) {
            dragging_ = false;
            update();
        }
    }

    void paintEvent(QPaintEvent *event) override {
        QPainter painter(this);
        painter.fillRect(event->rect(), QColor(QString::fromLatin1(border_color)));
        if (hovered_ && !dragging_) {
            painter.setPen(QColor(QString::fromLatin1(divider_hover_border_color)));
            painter.drawRect(rect().adjusted(0, 0, -1, -1));
        }
    }

  private:
    bool hovered_ = false;
    bool dragging_ = false;
};

enum class SplitterKind : std::uint8_t { Sidebar,
                                         Browser,
};

class ThinSplitter final : public QSplitter {
  public:
    explicit ThinSplitter(SplitterKind kind, QWidget *parent = nullptr) : QSplitter(Qt::Horizontal, parent), handle_accessible_name_(kind == SplitterKind::Sidebar ? QStringLiteral("Sidebar Splitter") : QStringLiteral("Browser Divider")) {
        setObjectName(kind == SplitterKind::Sidebar ? QStringLiteral("sidebarSplitter") : QStringLiteral("browserDivider"));
        setHandleWidth(splitter_width);
        setChildrenCollapsible(false);
    }

  protected:
    auto createHandle() -> QSplitterHandle * override { return new ThinSplitterHandle(orientation(), this, handle_accessible_name_); }

  private:
    QString handle_accessible_name_;
};

class ListingView final : public QTreeView {
  public:
    explicit ListingView(ListingModel *model) : model_(model) {
        setModel(model_);
        setFocusPolicy(Qt::StrongFocus);
        setSelectionMode(QAbstractItemView::SingleSelection);
        setSelectionBehavior(QAbstractItemView::SelectRows);
        setEditTriggers(QAbstractItemView::NoEditTriggers);
        setUniformRowHeights(true);
        setRootIsDecorated(false);
        setHeaderHidden(true);
        verticalScrollBar()->installEventFilter(this);
        horizontalScrollBar()->installEventFilter(this);
        connect(model_, &ListingModel::selectedRowChanged, this, [this] { synchronize_selection(); });
    }

    void setActivationHandler(std::function<void()> handler) { activation_handler_ = std::move(handler); }
    void activatePane() {
        if (!hasFocus())
            setFocus(Qt::MouseFocusReason);
        activate_pane();
    }

  protected:
    auto selectionCommand(const QModelIndex &, const QEvent *) const -> QItemSelectionModel::SelectionFlags override { return QItemSelectionModel::NoUpdate; }
    void focusInEvent(QFocusEvent *event) override {
        QTreeView::focusInEvent(event);
        activate_pane();
    }
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            activatePane();
            const auto index = indexAt(event->position().toPoint());
            if (index.isValid())
                model_->select_row(index.row());
            else
                model_->clear_selection();
        }
        event->accept();
    }
    void mouseReleaseEvent(QMouseEvent *event) override {
        event->accept();
    }
    void mouseDoubleClickEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            const auto index = indexAt(event->position().toPoint());
            if (index.isValid()) {
                model_->select_row(index.row());
                model_->activate_row(index.row());
            }
        }
        event->accept();
    }
    void keyPressEvent(QKeyEvent *event) override {
        // Qt maps ControlModifier to the Command key on macOS unless
        // AA_MacDontSwapCtrlAndMeta is explicitly enabled (it is not here).
        const auto modifiers = user_modifiers(event->modifiers());
        if (modifiers == Qt::ControlModifier && event->key() == Qt::Key_Up) {
            model_->go_to_parent();
            event->accept();
            return;
        }
        if (modifiers != Qt::NoModifier || event->key() == Qt::Key_Tab) {
            event->accept();
            return;
        }
        switch (arrow_action(event->key())) {
        case ArrowAction::Previous:
            model_->select_previous();
            event->accept();
            return;
        case ArrowAction::Next:
            model_->select_next();
            event->accept();
            return;
        case ArrowAction::Parent:
            model_->go_to_parent();
            event->accept();
            return;
        case ArrowAction::Activate:
            model_->activate_selected();
            event->accept();
            return;
        case ArrowAction::None:
            break;
        }
        const int count = model_->rowCount(QModelIndex());
        if (count == 0) {
            event->accept();
            return;
        }
        const int selected = model_->getSelectedRow();
        int target = selected;
        const int page = qMax(1, viewport()->height() / qMax(1, sizeHintForRow(0)));
        switch (event->key()) {
        case Qt::Key_Home:
            target = 0;
            break;
        case Qt::Key_End:
            target = count - 1;
            break;
        case Qt::Key_PageUp:
            target = selected < 0 ? 0 : qMax(0, selected - page);
            break;
        case Qt::Key_PageDown:
            target = selected < 0 ? count - 1 : qMin(count - 1, selected + page);
            break;
        case Qt::Key_Return:
            if (selected >= 0)
                model_->activate_row(selected);
            event->accept();
            return;
        default:
            event->accept();
            return;
        }
        model_->select_row(target);
        scrollTo(model_->index(target, 0));
        event->accept();
    }

  private:
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if ((watched == verticalScrollBar() || watched == horizontalScrollBar()) && event->type() == QEvent::MouseButtonPress) {
            const auto *mouse_event = dynamic_cast<QMouseEvent *>(event);
            if (mouse_event != nullptr && mouse_event->button() == Qt::LeftButton)
                activatePane();
        }
        return QTreeView::eventFilter(watched, event);
    }
    void activate_pane() {
        model_->activate_pane();
        if (activation_handler_)
            activation_handler_();
    }
    void synchronize_selection() {
        const auto row = model_->getSelectedRow();
        if (row >= 0)
            selectionModel()->select(model_->index(row, 0), QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
        else
            selectionModel()->clearSelection();
    }
    ListingModel *model_;
    std::function<void()> activation_handler_;
};

void repolish(QWidget *widget) {
    widget->style()->unpolish(widget);
    widget->style()->polish(widget);
    widget->update();
}

class PaneHighlightController final : public QObject {
  public:
    PaneHighlightController(QWidget *window, QFrame *left_folder, ListingView *left_view, QFrame *right_folder, ListingView *right_view) : window_(window), left_folder_(left_folder), left_view_(left_view), right_folder_(right_folder), right_view_(right_view), active_view_(left_view) {
        window_->installEventFilter(this);
        apply(window_->isActiveWindow());
    }

    void activate(ListingView *view) {
        active_view_ = view;
        apply(window_->isActiveWindow());
    }

    [[nodiscard]] auto other_view() const -> ListingView * { return active_view_ == left_view_ ? right_view_ : left_view_; }

  private:
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if (watched == window_ && event->type() == QEvent::WindowActivate)
            apply(true);
        else if (watched == window_ && event->type() == QEvent::WindowDeactivate)
            apply(false);
        return QObject::eventFilter(watched, event);
    }

    void apply(bool window_active) {
        apply_to(left_folder_, left_view_, active_view_ == left_view_, window_active);
        apply_to(right_folder_, right_view_, active_view_ == right_view_, window_active);
    }

    static void apply_to(QFrame *folder, ListingView *view, bool pane_active, bool window_active) {
        folder->setProperty("paneActive", pane_active);
        folder->setProperty("windowActive", window_active);
        view->setProperty("paneActive", pane_active);
        view->setProperty("windowActive", window_active);
        repolish(folder);
        repolish(view);
    }

    QWidget *window_;
    QFrame *left_folder_;
    ListingView *left_view_;
    QFrame *right_folder_;
    ListingView *right_view_;
    ListingView *active_view_;
};

class DrainScheduler final : public QObject {
  public:
    DrainScheduler(ListingModel *left, ListingModel *right) : left_(left), right_(right) {}
    void schedule() {
        if (!scheduled_.exchange(true))
            QMetaObject::invokeMethod(this, [this] { drain_one(); }, Qt::QueuedConnection);
    }

  private:
    void drain_one() {
        scheduled_.store(false);
        if (left_ != nullptr && left_->drain())
            schedule();
        if (right_ != nullptr)
            right_->refresh();
    }
    ListingModel *left_;
    ListingModel *right_;
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

auto browser(ListingModel *model, QWidget *parent, ListingView **out_view, QFrame **out_folder) -> QWidget * {
    auto *root = new QWidget(parent);
    root->setObjectName(QStringLiteral("browser"));
    auto *layout = new QVBoxLayout(root);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    auto *path = new QLineEdit(root);
    path->setReadOnly(true);
    path->setFocusPolicy(Qt::NoFocus);
    path->setAttribute(Qt::WA_TransparentForMouseEvents);
    path->setAccessibleName(QStringLiteral("Path"));
    auto *folder = new QFrame(root);
    folder->setObjectName(QStringLiteral("folderPane"));
    auto *folder_layout = new QVBoxLayout(folder);
    folder_layout->setContentsMargins(0, 0, 0, 0);
    folder_layout->setSpacing(0);
    auto *title = new QLabel(folder);
    title->setObjectName(QStringLiteral("folderName"));
    auto *up = new QToolButton(folder);
    up->setText(QStringLiteral("Up"));
    up->setAccessibleName(QStringLiteral("Up"));
    up->setFocusPolicy(Qt::NoFocus);
    auto *view = new ListingView(model);
    auto *status = new QLabel(folder);
    status->setObjectName(QStringLiteral("status"));
    layout->addWidget(path);
    folder_layout->addWidget(title);
    folder_layout->addWidget(up);
    folder_layout->addWidget(view, 1);
    folder_layout->addWidget(status);
    layout->addWidget(folder, 1);
    QObject::connect(model, &ListingModel::pathTextChanged, root, [path, model] { path->setText(model->getPathText()); });
    QObject::connect(model, &ListingModel::folderNameChanged, root, [title, model] { title->setText(model->getFolderName()); });
    QObject::connect(model, &ListingModel::statusTextChanged, root, [status, model] { status->setText(model->getStatusText()); });
    QObject::connect(up, &QToolButton::clicked, model, [model, view] { view->activatePane(); model->go_to_parent(); });
    *out_view = view;
    *out_folder = folder;
    return root;
}
} // namespace

void schedule_gui_drain() {
    auto &state = scheduler_state();
    std::scoped_lock lock(state.mutex);
    if (state.scheduler != nullptr)
        state.scheduler->schedule();
}
auto run_desktop(::rust::Box<PaneStartup> startup) -> int {
    constexpr int launch_precondition_failed = 2;
    if (pthread_main_np() == 0 || QApplication::instance() != nullptr || QCoreApplication::instance() != nullptr || has_run().exchange(true))
        return launch_precondition_failed;
    int argc = 1;
    char name[] = "dual-pane";
    char *argv[] = {name, nullptr};
    QApplication app(argc, argv);
    QMainWindow window;
    ListingModel left_model, right_model;
    right_model.set_right_pane();
    DrainScheduler scheduler(&left_model, &right_model);
    ThinSplitter standard_layout(SplitterKind::Sidebar);
    auto *sidebar = new QWidget(&standard_layout);
    sidebar->setObjectName(QStringLiteral("sidebar"));
    auto *side_layout = new QVBoxLayout(sidebar);
    side_layout->addWidget(new QLabel(QStringLiteral("Drives"), sidebar));
    side_layout->addWidget(new QLabel(QStringLiteral("Macintosh HD"), sidebar));
    side_layout->addWidget(new QLabel(QStringLiteral("Favorites"), sidebar));
    for (const auto &item : {"Home", "Desktop", "Documents", "Downloads"})
        side_layout->addWidget(new QLabel(QString::fromLatin1(item), sidebar));
    side_layout->addStretch();
    auto *split = new ThinSplitter(SplitterKind::Browser, &standard_layout);
    ListingView *left_view = nullptr, *right_view = nullptr;
    QFrame *left_folder = nullptr, *right_folder = nullptr;
    split->addWidget(browser(&left_model, split, &left_view, &left_folder));
    split->addWidget(browser(&right_model, split, &right_view, &right_folder));
    standard_layout.addWidget(sidebar);
    standard_layout.addWidget(split);
    window.setCentralWidget(&standard_layout);
    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.resize(initial_window_width, initial_window_height);
    window.setStyleSheet(style_sheet());
    PaneHighlightController pane_highlighter(&window, left_folder, left_view, right_folder, right_view);
    left_view->setActivationHandler([&pane_highlighter, left_view] { pane_highlighter.activate(left_view); });
    right_view->setActivationHandler([&pane_highlighter, right_view] { pane_highlighter.activate(right_view); });
    auto *shortcut = new QShortcut(QKeySequence(Qt::ALT | Qt::Key_F), &window);
    QObject::connect(shortcut, &QShortcut::activated, &window, [&pane_highlighter] { pane_highlighter.other_view()->setFocus(); });
    auto *close_shortcut = new QShortcut(QKeySequence::Close, &window);
    QObject::connect(close_shortcut, &QShortcut::activated, &window, [&window] { window.close(); });
    auto *quit_shortcut = new QShortcut(QKeySequence::Quit, &window);
    QObject::connect(quit_shortcut, &QShortcut::activated, &app, [] { QApplication::quit(); });
    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = &scheduler;
    }
    left_model.start(std::move(startup));
    right_model.refresh();
    window.show();
    left_view->setFocus(Qt::OtherFocusReason);
    const auto result = QApplication::exec();
    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = nullptr;
    }
    return result;
}
} // namespace dual_pane_desktop
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type)
