#include "dual_pane_desktop/desktop_window.hpp"
#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual_pane_desktop/settings_glyph.hpp"

#include <QtCore/QEvent>
#include <QtCore/QItemSelectionModel>
#include <QtCore/QMetaObject>
#include <QtCore/QString>
#include <QtCore/QVariant>
#include <QtGui/QIcon>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QPaintEvent>
#include <QtGui/QPainter>
#include <QtGui/QPixmap>
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
      QFrame#folderPane[browserActive="true"][windowActive="true"] { border:1px solid %5; }
      QTreeView { background:%2; color:%3; border:0; outline:none; }
      QTreeView::item:selected { background:%6; color:white; }
      QTreeView[browserActive="true"][windowActive="true"]::item:selected { background:%5; color:white; }
      QToolButton { background:%2; color:%3; border:1px solid %4; padding:4px; }
      QToolButton:disabled { color:#737A84; }
      QToolButton#sortControl { min-width:16px; max-width:16px; min-height:16px; max-height:16px; padding:0; }
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

class FolderItemsList final : public QTreeView {
  public:
    explicit FolderItemsList(FolderItemsListModel *model) : model_(model) {
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
        connect(model_, &FolderItemsListModel::selectedRowChanged, this, [this] { synchronize_selection(); });
    }

    void setActivationHandler(std::function<void()> handler) { activation_handler_ = std::move(handler); }
    void activateBrowser() {
        if (!hasFocus())
            setFocus(Qt::MouseFocusReason);
        activate_browser();
    }

  protected:
    auto selectionCommand(const QModelIndex &, const QEvent *) const -> QItemSelectionModel::SelectionFlags override { return QItemSelectionModel::NoUpdate; }
    void focusInEvent(QFocusEvent *event) override {
        QTreeView::focusInEvent(event);
        activate_browser();
    }
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            activateBrowser();
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
                activateBrowser();
        }
        return QTreeView::eventFilter(watched, event);
    }
    void activate_browser() {
        model_->activate_browser();
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
    FolderItemsListModel *model_;
    std::function<void()> activation_handler_;
};

void repolish(QWidget *widget) {
    widget->style()->unpolish(widget);
    widget->style()->polish(widget);
    widget->update();
}

class BrowserHighlightController final : public QObject {
  public:
    BrowserHighlightController(QWidget *window, QFrame *left_folder, FolderItemsList *left_view, QFrame *right_folder, FolderItemsList *right_view) : window_(window), left_folder_(left_folder), left_view_(left_view), right_folder_(right_folder), right_view_(right_view), active_view_(left_view) {
        window_->installEventFilter(this);
        apply(window_->isActiveWindow());
    }

    void activate(FolderItemsList *view) {
        active_view_ = view;
        apply(window_->isActiveWindow());
    }

    [[nodiscard]] auto other_view() const -> FolderItemsList * { return active_view_ == left_view_ ? right_view_ : left_view_; }

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

    static void apply_to(QFrame *folder, FolderItemsList *view, bool browser_active, bool window_active) {
        folder->setProperty("browserActive", browser_active);
        folder->setProperty("windowActive", window_active);
        view->setProperty("browserActive", browser_active);
        view->setProperty("windowActive", window_active);
        repolish(folder);
        repolish(view);
    }

    QWidget *window_;
    QFrame *left_folder_;
    FolderItemsList *left_view_;
    QFrame *right_folder_;
    FolderItemsList *right_view_;
    FolderItemsList *active_view_;
};

class DrainScheduler final : public QObject {
  public:
    DrainScheduler(FolderItemsListModel *left, FolderItemsListModel *right) : left_(left), right_(right) {}
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
    FolderItemsListModel *left_;
    FolderItemsListModel *right_;
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

class MainToolbar final : public QWidget {
  public:
    explicit MainToolbar(QWidget *parent) : QWidget(parent) {
        setObjectName(QStringLiteral("mainToolbar"));
        auto *layout = new QHBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        auto *settings = new QToolButton(this);
        settings->setObjectName(QStringLiteral("settings"));
        settings->setIcon(QIcon(QPixmap(settings_glyph::pixels)));
        settings->setToolTip(QStringLiteral("Settings"));
        settings->setAccessibleName(QStringLiteral("Settings"));
        settings->setFocusPolicy(Qt::NoFocus);
        settings->setDisabled(true);
        layout->addWidget(settings);
        layout->addStretch();
    }
};

class Sidebar final : public QWidget {
  public:
    explicit Sidebar(QWidget *parent) : QWidget(parent) {
        setObjectName(QStringLiteral("sidebar"));
        auto *layout = new QVBoxLayout(this);
        layout->addWidget(new QLabel(QStringLiteral("Drives"), this));
        layout->addWidget(new QLabel(QStringLiteral("Macintosh HD"), this));
        layout->addWidget(new QLabel(QStringLiteral("Favorites"), this));
        for (const auto &item : {"Home", "Desktop", "Documents", "Downloads"})
            layout->addWidget(new QLabel(QString::fromLatin1(item), this));
        layout->addStretch(1);
        layout->addWidget(new MainToolbar(this));
    }
};

class BrowserTabsStrip final : public QWidget {
  public:
    explicit BrowserTabsStrip(FolderItemsListModel *model, QWidget *parent) : QWidget(parent), label_(new QLabel(this)) {
        setObjectName(QStringLiteral("browserTabsStrip"));
        setAccessibleName(QStringLiteral("Browser Tabs Strip"));
        setMinimumHeight(24);
        auto *layout = new QHBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->addWidget(label_);
        QObject::connect(model, &FolderItemsListModel::folderNameChanged, this, [this, model] { label_->setText(model->getFolderName()); });
    }

  private:
    QLabel *label_;
};

auto sort_control(const QString &field, const QString &direction, QWidget *parent) -> QToolButton * {
    auto *control = new QToolButton(parent);
    control->setObjectName(QStringLiteral("sortControl"));
    const auto description = QStringLiteral("Sort by %1 %2").arg(field, direction);
    control->setText(direction == QStringLiteral("ascending") ? QStringLiteral("^") : QStringLiteral("v"));
    control->setToolTip(description);
    control->setAccessibleName(description);
    control->setFocusPolicy(Qt::NoFocus);
    control->setDisabled(true);
    return control;
}

class FolderPane final : public QFrame {
  public:
    explicit FolderPane(FolderItemsListModel *model, QWidget *parent) : QFrame(parent), view_(new FolderItemsList(model)) {
        setObjectName(QStringLiteral("folderPane"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        auto *title = new QLabel(this);
        title->setObjectName(QStringLiteral("folderPaneToolbarRow1"));
        auto *summary = new QLabel(this);
        summary->setObjectName(QStringLiteral("folderPaneToolbarRow2"));
        summary->setMinimumHeight(20);
        auto *commands = new QWidget(this);
        commands->setObjectName(QStringLiteral("folderPaneToolbarRow3"));
        auto *command_layout = new QHBoxLayout(commands);
        command_layout->setContentsMargins(0, 0, 0, 0);
        command_layout->setSpacing(0);
        auto *up = new QToolButton(commands);
        up->setObjectName(QStringLiteral("upButton"));
        up->setText(QStringLiteral("Up"));
        up->setAccessibleName(QStringLiteral("Up Button"));
        up->setFocusPolicy(Qt::NoFocus);
        command_layout->addWidget(up);
        for (const auto &field : {QStringLiteral("Name"), QStringLiteral("Type"), QStringLiteral("Date"), QStringLiteral("Size")}) {
            command_layout->addWidget(sort_control(field, QStringLiteral("ascending"), commands));
            command_layout->addWidget(sort_control(field, QStringLiteral("descending"), commands));
        }
        command_layout->addStretch();
        view_->setObjectName(QStringLiteral("folderItemsList"));
        view_->setAccessibleName(QStringLiteral("Folder Items List"));
        auto *status = new QLabel(this);
        status->setObjectName(QStringLiteral("browserStatusBar"));
        status->setAccessibleName(QStringLiteral("Browser Status Bar"));
        layout->addWidget(title);
        layout->addWidget(summary);
        layout->addWidget(commands);
        layout->addWidget(view_, 1);
        layout->addWidget(status);
        QObject::connect(model, &FolderItemsListModel::folderNameChanged, this, [title, model] { title->setText(model->getFolderName()); });
        QObject::connect(model, &FolderItemsListModel::statusTextChanged, this, [status, model] { status->setText(model->getStatusText()); });
        QObject::connect(up, &QToolButton::clicked, model, [model, this] { view_->activateBrowser(); model->go_to_parent(); });
    }

    [[nodiscard]] auto view() const -> FolderItemsList * { return view_; }

  private:
    FolderItemsList *view_;
};

class Browser final : public QWidget {
  public:
    explicit Browser(FolderItemsListModel *model, QWidget *parent) : QWidget(parent), folder_(new FolderPane(model, this)) {
        setObjectName(QStringLiteral("browser"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        layout->addWidget(new BrowserTabsStrip(model, this));
        auto *path = new QLineEdit(this);
        path->setObjectName(QStringLiteral("pathEditControl"));
        path->setReadOnly(true);
        path->setFocusPolicy(Qt::NoFocus);
        path->setAttribute(Qt::WA_TransparentForMouseEvents);
        path->setAccessibleName(QStringLiteral("Path Edit Control"));
        layout->addWidget(path);
        layout->addWidget(folder_, 1);
        QObject::connect(model, &FolderItemsListModel::pathTextChanged, this, [path, model] { path->setText(model->getPathText()); });
    }

    [[nodiscard]] auto view() const -> FolderItemsList * { return folder_->view(); }
    [[nodiscard]] auto folder() const -> QFrame * { return folder_; }

  private:
    FolderPane *folder_;
};
} // namespace

void schedule_gui_drain() {
    auto &state = scheduler_state();
    std::scoped_lock lock(state.mutex);
    if (state.scheduler != nullptr)
        state.scheduler->schedule();
}
auto run_desktop(::rust::Box<BrowserStartup> startup) -> int {
    constexpr int launch_precondition_failed = 2;
    if (pthread_main_np() == 0 || QApplication::instance() != nullptr || QCoreApplication::instance() != nullptr || has_run().exchange(true))
        return launch_precondition_failed;
    int argc = 1;
    char name[] = "dual-pane";
    char *argv[] = {name, nullptr};
    QApplication app(argc, argv);
    QMainWindow window;
    FolderItemsListModel left_model, right_model;
    right_model.set_right_browser();
    DrainScheduler scheduler(&left_model, &right_model);
    ThinSplitter standard_layout(SplitterKind::Sidebar);
    auto *sidebar = new Sidebar(&standard_layout);
    auto *split = new ThinSplitter(SplitterKind::Browser, &standard_layout);
    auto *left_browser = new Browser(&left_model, split);
    auto *right_browser = new Browser(&right_model, split);
    auto *left_view = left_browser->view();
    auto *right_view = right_browser->view();
    auto *left_folder = left_browser->folder();
    auto *right_folder = right_browser->folder();
    split->addWidget(left_browser);
    split->addWidget(right_browser);
    standard_layout.addWidget(sidebar);
    standard_layout.addWidget(split);
    window.setCentralWidget(&standard_layout);
    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.resize(initial_window_width, initial_window_height);
    window.setStyleSheet(style_sheet());
    BrowserHighlightController browser_highlighter(&window, left_folder, left_view, right_folder, right_view);
    left_view->setActivationHandler([&browser_highlighter, left_view] { browser_highlighter.activate(left_view); });
    right_view->setActivationHandler([&browser_highlighter, right_view] { browser_highlighter.activate(right_view); });
    auto *shortcut = new QShortcut(QKeySequence(Qt::ALT | Qt::Key_F), &window);
    QObject::connect(shortcut, &QShortcut::activated, &window, [&browser_highlighter] { browser_highlighter.other_view()->setFocus(); });
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
