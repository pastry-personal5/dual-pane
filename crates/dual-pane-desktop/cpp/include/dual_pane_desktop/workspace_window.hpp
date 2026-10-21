#ifndef dual_pane_DESKTOP_WORKSPACE_WINDOW_HPP
#define dual_pane_DESKTOP_WORKSPACE_WINDOW_HPP

#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

/// The workspace window. Every close, including Command+Q, Dock Quit, and the
/// close button, asks `handler` whether to proceed.
class WorkspaceWindow final : public QMainWindow {
  public:
    WorkspaceWindow() { setWindowFlags(Qt::Window | Qt::FramelessWindowHint); }
    void setCloseHandler(std::function<void(QCloseEvent *)> handler) { close_ = std::move(handler); }
    void setBeforeCloseHandler(std::function<void()> handler) { before_close_ = std::move(handler); }
    /// The enclosing desktop code debounces these notifications before it
    /// reports a layout snapshot to Rust.
    void setLayoutChangeHandler(std::function<void()> handler) { layout_changed_ = std::move(handler); }

  protected:
    bool event(QEvent *event) override {
        const auto type = event->type();
        if (layout_changed_ && (type == QEvent::Move || type == QEvent::Resize || type == QEvent::WindowStateChange))
            layout_changed_();
        return QMainWindow::event(event);
    }
    void closeEvent(QCloseEvent *event) override {
        if (before_close_)
            before_close_();
        if (close_)
            close_(event);
        else
            QMainWindow::closeEvent(event);
    }

  private:
    std::function<void(QCloseEvent *)> close_;
    std::function<void()> before_close_;
    std::function<void()> layout_changed_;
};

/// A deliberately small, non-progress-bearing launch surface. The workspace
/// stays hidden until the settings result decides which layout to show.
class WaitingWindow final : public QWidget {
  public:
    explicit WaitingWindow(WorkspaceWindow *workspace) : QWidget(nullptr, Qt::Tool), workspace_(workspace) {
        setObjectName(QStringLiteral("waitingWindow"));
        setWindowTitle(QStringLiteral("Dual Pane"));
        setAccessibleName(QStringLiteral("Loading Workspace"));
        setWindowModality(Qt::ApplicationModal);
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(32, 28, 32, 28);
        auto *title = new QLabel(QStringLiteral("Loading workspace"), this);
        title->setAccessibleName(QStringLiteral("Loading workspace"));
        QFont font = title->font();
        font.setPointSize(font.pointSize() + 2);
        font.setBold(true);
        title->setFont(font);
        auto *status = new QLabel(QStringLiteral("Preparing your saved workspace…"), this);
        status->setAccessibleName(QStringLiteral("Loading workspace status"));
        auto *quit = new QPushButton(QStringLiteral("Quit"), this);
        QObject::connect(quit, &QPushButton::clicked, this, [this] { workspace_->close(); });
        layout->addWidget(title);
        layout->addWidget(status);
        layout->addSpacing(12);
        layout->addWidget(quit, 0, Qt::AlignRight);
        resize(340, 160);
    }

  private:
    WorkspaceWindow *workspace_;
};

/// Brings the workspace's current blocking surface forward. The target is
/// chosen again on the next GUI turn because a modal can appear between the
/// application command and the deferred native activation fallback.
void activate_workspace_window(WorkspaceWindow *window, WaitingWindow *waiting, int request, std::function<void(int)> focus_browser, bool allow_native_fallback = true);

/// The auxiliary Notices window. It never blocks browsing; storage errors
/// offer Reset Settings behind an explicit confirmation.
class NoticesWindow final : public QWidget {
  public:
    NoticesWindow(WorkspaceBridge *bridge, QWidget *owner) : QWidget(owner, Qt::Window), bridge_(bridge), list_(new QWidget()), startup_checkbox_(new QCheckBox(QStringLiteral("Don’t show notices at startup"), this)) {
        setObjectName(QStringLiteral("notices"));
        setWindowTitle(QStringLiteral("Notices"));
        setAccessibleName(QStringLiteral("Notices"));
        resize(440, 320);
        auto *layout = new QVBoxLayout(this);
        auto *scroll = new QScrollArea(this);
        scroll->setWidgetResizable(true);
        list_->setObjectName(QStringLiteral("noticesList"));
        new QVBoxLayout(list_);
        scroll->setWidget(list_);
        layout->addWidget(scroll);
        startup_checkbox_->setObjectName(QStringLiteral("noticesStartupCheckbox"));
        startup_checkbox_->setAccessibleName(QStringLiteral("Notices Startup Checkbox"));
        startup_checkbox_->setEnabled(bridge_->getNoticesStartupReady());
        startup_checkbox_->setChecked(bridge_->getHideNoticesAtStartup());
        QObject::connect(startup_checkbox_, &QCheckBox::toggled, this, [this](bool checked) { bridge_->changeHideNoticesAtStartup(checked); });
        QObject::connect(bridge_, &WorkspaceBridge::hideNoticesAtStartupChanged, this, [this] {
            QSignalBlocker block(startup_checkbox_);
            startup_checkbox_->setChecked(bridge_->getHideNoticesAtStartup());
        });
        QObject::connect(bridge_, &WorkspaceBridge::noticesStartupReadyChanged, this, [this] { startup_checkbox_->setEnabled(bridge_->getNoticesStartupReady()); });
        layout->addWidget(startup_checkbox_);
        auto *close = new QShortcut(QKeySequence::Close, this);
        close->setContext(Qt::WindowShortcut);
        QObject::connect(close, &QShortcut::activated, this, &QWidget::close);
        QObject::connect(bridge_, &WorkspaceBridge::noticesRevisionChanged, this, [this] { rebuild(); });
        rebuild();
    }

  private:
    void rebuild() {
        auto *layout = list_->layout();
        while (auto *child = layout->takeAt(0)) {
            if (auto *widget = child->widget())
                widget->deleteLater();
            delete child;
        }
        for (int index = bridge_->noticeCount() - 1; index >= 0; --index) {
            auto *notice = new QLabel(bridge_->noticeText(index), list_);
            notice->setObjectName(QStringLiteral("notice"));
            notice->setWordWrap(true);
            notice->setAccessibleName(QStringLiteral("Notice"));
            notice->setAccessibleDescription(notice->text());
            layout->addWidget(notice);
            if (bridge_->noticeOffersReset(index)) {
                auto *reset = new QPushButton(QStringLiteral("Reset Settings…"), list_);
                reset->setAccessibleName(QStringLiteral("Reset Settings"));
                QObject::connect(reset, &QPushButton::clicked, this, [this] { confirm_reset(); });
                layout->addWidget(reset);
            }
            if (bridge_->noticeOffersJournalRetry(index)) {
                auto *retry = new QPushButton(QStringLiteral("Try Again"), list_);
                retry->setAccessibleName(QStringLiteral("Try Again"));
                QObject::connect(retry, &QPushButton::clicked, this, [this] { bridge_->retryJournal(); });
                layout->addWidget(retry);
            }
        }
        if (bridge_->noticeCount() == 0)
            layout->addWidget(new QLabel(QStringLiteral("No notices."), list_));
        dynamic_cast<QVBoxLayout *>(layout)->addStretch();
    }
    void confirm_reset() {
        auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Reset Settings"), QStringLiteral("Reset Dual Pane’s settings?"), QMessageBox::NoButton, this);
        box->setInformativeText(QStringLiteral("Your settings and Favorites, including changes made in this session, will be replaced with defaults. Open tabs will close and the workspace layout will return to its defaults. The current settings database is kept as a backup."));
        auto *reset = box->addButton(QStringLiteral("Reset"), QMessageBox::DestructiveRole);
        auto *cancel = box->addButton(QMessageBox::Cancel);
        box->setDefaultButton(cancel);
        box->setEscapeButton(cancel);
        box->setWindowModality(Qt::WindowModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, this, [this, box, reset] {
            if (box->clickedButton() == reset)
                bridge_->resetSettings();
        });
        box->open();
    }

    WorkspaceBridge *bridge_;
    QWidget *list_;
    QCheckBox *startup_checkbox_;
};

class DrainScheduler final : public QObject {
  public:
    explicit DrainScheduler(WorkspaceBridge *bridge) : bridge_(bridge) {}
    void schedule() {
        if (!scheduled_.exchange(true))
            QMetaObject::invokeMethod(this, [this] { drain_one(); }, Qt::QueuedConnection);
    }

  private:
    void drain_one() {
        scheduled_.store(false);
        if (bridge_ != nullptr && bridge_->drain())
            schedule();
    }
    WorkspaceBridge *bridge_;
    std::atomic_bool scheduled_ = false;
};

struct SchedulerState {
    std::mutex mutex;
    DrainScheduler *scheduler = nullptr;
};

auto scheduler_state() -> SchedulerState &;

auto has_run() -> std::atomic_bool &;

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_WORKSPACE_WINDOW_HPP
