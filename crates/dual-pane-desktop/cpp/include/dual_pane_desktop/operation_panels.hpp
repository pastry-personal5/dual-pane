#ifndef dual_pane_DESKTOP_OPERATION_PANELS_HPP
#define dual_pane_DESKTOP_OPERATION_PANELS_HPP

#include "dual-pane-desktop/src/operations_bridge.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

/// One job's Operation Panel: progress, the Cancel Operation Button, and any
/// Operation Decision Card. Dragging its title docks or floats it.
class OperationPanel final : public QFrame {
  public:
    OperationPanel(OperationsModel *ops, std::int64_t job, std::function<void(OperationPanel *)> drag, QWidget *parent) : QFrame(parent), ops_(ops), job_(job), drag_(std::move(drag)), title_(new QLabel(this)), detail_(new QLabel(this)), progress_(new QProgressBar(this)), bytes_(new QLabel(this)), status_(new QLabel(this)), cancel_(new QPushButton(QStringLiteral("Cancel"), this)), close_(new QPushButton(QStringLiteral("Close"), this)), card_(new QFrame(this)), message_(new QLabel(card_)), choices_(new QHBoxLayout()), apply_(new QCheckBox(QStringLiteral("Apply to all"), card_)) {
        setObjectName(QStringLiteral("operationPanel"));
        setAccessibleName(QStringLiteral("Operation Panel"));
        setMinimumWidth(320);
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(8, 6, 8, 6);
        layout->setSpacing(4);
        title_->setObjectName(QStringLiteral("operationPanelTitle"));
        title_->setCursor(Qt::OpenHandCursor);
        title_->setToolTip(QStringLiteral("Drag to dock or float this panel"));
        title_->installEventFilter(this);
        detail_->setWordWrap(true);
        status_->setWordWrap(true);
        progress_->setObjectName(QStringLiteral("operationProgressIndicator"));
        progress_->setAccessibleName(QStringLiteral("Operation Progress Indicator"));
        progress_->setTextVisible(false);
        cancel_->setObjectName(QStringLiteral("cancelOperationButton"));
        cancel_->setAccessibleName(QStringLiteral("Cancel Operation Button"));
        for (auto *button : {cancel_, close_}) {
            button->setAutoDefault(false);
            button->setFocusPolicy(Qt::TabFocus);
        }
        QObject::connect(cancel_, &QPushButton::clicked, this, [this] { ops_->cancelOperation(job_); });
        QObject::connect(close_, &QPushButton::clicked, this, [this] { ops_->closePanel(job_); });
        card_->setObjectName(QStringLiteral("operationDecisionCard"));
        card_->setAccessibleName(QStringLiteral("Operation Decision Card"));
        card_->setFocusPolicy(Qt::NoFocus);
        auto *card_layout = new QVBoxLayout(card_);
        card_layout->setContentsMargins(6, 6, 6, 6);
        message_->setWordWrap(true);
        apply_->setFocusPolicy(Qt::TabFocus);
        card_layout->addWidget(message_);
        card_layout->addWidget(apply_);
        card_layout->addLayout(choices_);
        card_->hide();
        auto *buttons = new QHBoxLayout();
        buttons->addWidget(bytes_, 1);
        buttons->addWidget(cancel_);
        buttons->addWidget(close_);
        layout->addWidget(title_);
        layout->addWidget(detail_);
        layout->addWidget(progress_);
        layout->addWidget(status_);
        layout->addLayout(buttons);
        layout->addWidget(card_);
    }

    [[nodiscard]] auto job() const -> std::int64_t { return job_; }
    [[nodiscard]] auto finished() const -> bool { return finished_; }
    [[nodiscard]] auto title() const -> QString { return title_->text(); }

    /// Shows the model's panel at `index`. A card that appears is announced
    /// without taking focus, and none of its buttons is a default.
    void update(int index) {
        title_->setText(ops_->panelTitle(index));
        detail_->setText(ops_->panelDetail(index));
        status_->setText(ops_->panelStatus(index));
        setAccessibleDescription(QStringLiteral("%1 %2. %3").arg(title_->text(), detail_->text(), status_->text()));
        const int total = ops_->panelTotal(index);
        finished_ = ops_->panelFinished(index);
        if (total < 0 && !finished_) {
            progress_->setRange(0, 0);
        } else {
            progress_->setRange(0, qMax(total, 1));
            progress_->setValue(finished_ ? qMax(total, 1) : ops_->panelDone(index));
        }
        bytes_->setText(ops_->panelBytes(index));
        cancel_->setVisible(!finished_);
        cancel_->setEnabled(ops_->panelCanCancel(index));
        close_->setVisible(finished_);
        if (!ops_->panelHasDecision(index)) {
            card_->hide();
            shown_message_.clear();
            shown_token_ = -1;
            return;
        }
        const auto message = ops_->decisionMessage(index);
        if (message == shown_message_ && ops_->decisionToken(index) == shown_token_ && card_->isVisible())
            return;
        const auto token = ops_->decisionToken(index);
        shown_token_ = token;
        shown_message_ = message;
        message_->setText(message);
        card_->setAccessibleDescription(message);
        while (auto *child = choices_->takeAt(0)) {
            if (auto *widget = child->widget())
                widget->deleteLater();
            delete child;
        }
        choices_->addStretch();
        for (int choice = 0; choice < ops_->decisionChoiceCount(index); ++choice) {
            auto *button = new QPushButton(ops_->decisionChoiceLabel(index, choice), card_);
            button->setAutoDefault(false);
            button->setDefault(false);
            button->setFocusPolicy(Qt::TabFocus);
            const int code = ops_->decisionChoiceCode(index, choice);
            QObject::connect(button, &QPushButton::clicked, this, [this, token, code] { ops_->decide(job_, token, code, apply_->isVisible() && apply_->isChecked()); });
            choices_->addWidget(button);
        }
        apply_->setChecked(false);
        apply_->setVisible(ops_->decisionOffersApplyToAll(index));
        card_->show();
        QAccessibleEvent announcement(card_, QAccessible::Alert);
        QAccessible::updateAccessibility(&announcement);
    }

  protected:
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if (watched == title_ && event->type() == QEvent::MouseButtonPress) {
            press_ = dynamic_cast<QMouseEvent *>(event)->position().toPoint();
            pressed_ = true;
        } else if (watched == title_ && event->type() == QEvent::MouseButtonRelease) {
            pressed_ = false;
        } else if (watched == title_ && event->type() == QEvent::MouseMove && pressed_) {
            const auto *mouse = dynamic_cast<QMouseEvent *>(event);
            if ((mouse->position().toPoint() - press_).manhattanLength() >= QApplication::startDragDistance()) {
                pressed_ = false;
                drag_(this);
                return true;
            }
        }
        return QFrame::eventFilter(watched, event);
    }

  private:
    OperationsModel *ops_;
    std::int64_t job_;
    std::function<void(OperationPanel *)> drag_;
    QLabel *title_;
    QLabel *detail_;
    QProgressBar *progress_;
    QLabel *bytes_;
    QLabel *status_;
    QPushButton *cancel_;
    QPushButton *close_;
    QFrame *card_;
    QLabel *message_;
    QHBoxLayout *choices_;
    QCheckBox *apply_;
    QString shown_message_;
    std::int64_t shown_token_ = -1;
    QPoint press_;
    bool pressed_ = false;
    bool finished_ = false;
};

/// A floating Operation Panel: a regular window that joins window cycling.
/// It never takes focus when it appears. Only a finished panel can be
/// closed; a close that is part of quitting goes to the workspace window's
/// quit decision instead.
class FloatingPanelWindow final : public QWidget {
  public:
    FloatingPanelWindow(OperationPanel *panel, OperationsModel *ops, const bool *quitting, QWidget *workspace) : QWidget(workspace, Qt::Window), panel_(panel), ops_(ops), quitting_(quitting), workspace_(workspace) {
        setObjectName(QStringLiteral("floatingOperationPanel"));
        // A top-level child window does not inherit the workspace's style
        // sheet on every platform. Apply the dark theme at this window root
        // so its panel and dynamically created decision buttons stay legible.
        setStyleSheet(style_sheet());
        setAttribute(Qt::WA_ShowWithoutActivating);
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        panel_->setParent(this);
        layout->addWidget(panel_);
        auto *close = new QShortcut(QKeySequence::Close, this);
        close->setContext(Qt::WindowShortcut);
        QObject::connect(close, &QShortcut::activated, this, [this] {
            if (panel_->finished())
                ops_->closePanel(panel_->job());
        });
        apply_flags(false);
    }

    /// Takes the panel back out, for docking.
    auto release() -> OperationPanel * {
        layout()->removeWidget(panel_);
        auto *panel = panel_;
        panel_ = nullptr;
        return panel;
    }
    void sync() {
        if (panel_ == nullptr)
            return;
        setWindowTitle(panel_->title());
        if (panel_->finished() != closable_)
            apply_flags(panel_->finished());
    }

  protected:
    void closeEvent(QCloseEvent *event) override {
        if (*quitting_ || panel_ == nullptr) {
            event->accept();
            return;
        }
        event->ignore();
        // A running panel has no close button, so any close request is part
        // of quitting and goes to the workspace window's quit decision.
        if (event->spontaneous() && panel_->finished()) {
            ops_->closePanel(panel_->job());
            return;
        }
        QTimer::singleShot(0, workspace_, [workspace = QPointer<QWidget>(workspace_)] {
            if (workspace)
                workspace->close();
        });
    }

  private:
    /// Only a finished panel shows a close button.
    void apply_flags(bool closable) {
        closable_ = closable;
        const bool visible = isVisible();
        setWindowFlags(Qt::Window | Qt::WindowTitleHint | Qt::CustomizeWindowHint | (closable ? Qt::WindowCloseButtonHint : Qt::WindowType(0)));
        if (visible)
            show();
    }

    OperationPanel *panel_;
    OperationsModel *ops_;
    const bool *quitting_;
    QWidget *workspace_;
    bool closable_ = true;
};

/// The Operation Panel Strip below both Browsers. Docked panels arrange left
/// to right; a panel dragged onto it docks.
class OperationPanelStrip final : public QWidget {
  public:
    explicit OperationPanelStrip(QWidget *parent) : QWidget(parent), layout_(new QHBoxLayout(this)) {
        setObjectName(QStringLiteral("operationPanelStrip"));
        setAccessibleName(QStringLiteral("Operation Panel Strip"));
        setAcceptDrops(true);
        layout_->setContentsMargins(4, 4, 4, 4);
        layout_->setSpacing(4);
        layout_->addStretch();
        hide();
    }
    void setDropHandler(std::function<void(std::int64_t)> handler) { drop_ = std::move(handler); }
    void dock(OperationPanel *panel) {
        panel->setParent(this);
        layout_->insertWidget(layout_->count() - 1, panel);
        panel->show();
        show();
    }
    void undock(OperationPanel *panel) {
        layout_->removeWidget(panel);
        setVisible(layout_->count() > 1);
    }

  protected:
    void dragEnterEvent(QDragEnterEvent *event) override { accept(event); }
    void dragMoveEvent(QDragMoveEvent *event) override { accept(event); }
    void dropEvent(QDropEvent *event) override {
        if (!accept(event))
            return;
        drop_(event->mimeData()->data(QString::fromLatin1(operation_panel_mime)).toLongLong());
    }

  private:
    auto accept(QDropEvent *event) -> bool {
        if (!event->mimeData()->hasFormat(QString::fromLatin1(operation_panel_mime))) {
            event->ignore();
            return false;
        }
        event->setDropAction(Qt::MoveAction);
        event->accept();
        return true;
    }

    QHBoxLayout *layout_;
    std::function<void(std::int64_t)> drop_;
};

/// Shows one Operation Panel per revealed job, floating by default. Where a
/// panel sits is session-only presentation state.
class OperationPanels final : public QObject {
  public:
    OperationPanels(OperationsModel *ops, OperationPanelStrip *strip, QWidget *workspace, const bool *quitting) : ops_(ops), strip_(strip), workspace_(workspace), quitting_(quitting) {
        strip_->setDropHandler([this](std::int64_t job) { dock(job); });
        QObject::connect(ops_, &OperationsModel::panelsRevisionChanged, this, [this] { reconcile(); });
    }

    /// The docked panel holding `widget`, if any.
    [[nodiscard]] auto docked_panel(const QWidget *widget) const -> OperationPanel * {
        for (const auto &placed : panels_)
            if (placed.window == nullptr && inside(widget, placed.panel))
                return placed.panel;
        return nullptr;
    }

  private:
    struct Placed {
        OperationPanel *panel = nullptr;
        FloatingPanelWindow *window = nullptr;
    };

    static auto inside(const QWidget *widget, const QWidget *ancestor) -> bool { return widget != nullptr && (widget == ancestor || ancestor->isAncestorOf(widget)); }

    void reconcile() {
        QSet<std::int64_t> present;
        for (int index = 0; index < ops_->panelCount(); ++index) {
            const auto job = ops_->panelId(index);
            present.insert(job);
            if (!panels_.contains(job)) {
                // A new panel is filled before its window is sized.
                auto *panel = new OperationPanel(ops_, job, [this](OperationPanel *dragged) { drag(dragged); }, nullptr);
                panel->update(index);
                float_panel(panel, std::nullopt);
            }
            const auto placed = panels_.value(job);
            placed.panel->update(index);
            if (placed.window != nullptr)
                placed.window->sync();
        }
        for (auto it = panels_.begin(); it != panels_.end();) {
            if (present.contains(it.key())) {
                ++it;
                continue;
            }
            if (it->window != nullptr) {
                it->window->hide();
                it->window->deleteLater();
            } else {
                strip_->undock(it->panel);
                it->panel->deleteLater();
            }
            it = panels_.erase(it);
        }
    }
    void float_panel(OperationPanel *panel, std::optional<QPoint> at) {
        auto *window = new FloatingPanelWindow(panel, ops_, quitting_, workspace_);
        panels_.insert(panel->job(), Placed{panel, window});
        window->sync();
        window->adjustSize();
        if (at) {
            window->move(*at);
        } else {
            // New panels cascade from the workspace window's lower right.
            const auto corner = workspace_->geometry().bottomRight();
            const int offset = 24 * static_cast<int>(panels_.size() - 1);
            window->move(corner.x() - window->width() - 24 - offset, corner.y() - window->height() - 24 - offset);
        }
        window->show();
    }
    void dock(std::int64_t job) {
        auto it = panels_.find(job);
        if (it == panels_.end() || it->window == nullptr)
            return;
        auto *panel = it->window->release();
        it->window->hide();
        it->window->deleteLater();
        it->window = nullptr;
        strip_->dock(panel);
    }
    /// Starts a drag: a drop on the strip docks the panel, and a docked panel
    /// dropped anywhere else floats where it was dropped.
    void drag(OperationPanel *panel) {
        const auto job = panel->job();
        auto *mime = new QMimeData();
        mime->setData(QString::fromLatin1(operation_panel_mime), QByteArray::number(static_cast<qlonglong>(job)));
        auto *drag = new QDrag(panel);
        drag->setMimeData(mime);
        drag->setPixmap(panel->grab().scaledToWidth(qMin(panel->width(), 240), Qt::SmoothTransformation));
        const auto action = drag->exec(Qt::MoveAction);
        auto it = panels_.find(job);
        if (action == Qt::IgnoreAction && it != panels_.end() && it->window == nullptr) {
            strip_->undock(it->panel);
            float_panel(it->panel, QCursor::pos());
        }
    }

    OperationsModel *ops_;
    OperationPanelStrip *strip_;
    QWidget *workspace_;
    const bool *quitting_;
    QHash<std::int64_t, Placed> panels_;
};

/// One Permanent Delete Confirmation Window per pending deletion. It shows
/// only the frozen target count, defaults to Cancel, and stays bound to its
/// job whatever the Browsers do meanwhile.
class DeleteConfirmations final : public QObject {
  public:
    DeleteConfirmations(OperationsModel *ops, QWidget *workspace) : ops_(ops), workspace_(workspace) {
        QObject::connect(ops_, &OperationsModel::confirmationsRevisionChanged, this, [this] { reconcile(); });
    }

  private:
    void reconcile() {
        QSet<std::int64_t> present;
        for (int index = 0; index < ops_->confirmationCount(); ++index) {
            const auto job = ops_->confirmationId(index);
            present.insert(job);
            if (!boxes_.contains(job))
                open(job, ops_->confirmationTargets(index));
        }
        for (auto it = boxes_.begin(); it != boxes_.end();) {
            if (present.contains(it.key())) {
                ++it;
                continue;
            }
            if (it.value() != nullptr) {
                QObject::disconnect(it.value(), nullptr, this, nullptr);
                it.value()->close();
            }
            it = boxes_.erase(it);
        }
    }
    void open(std::int64_t job, int targets) {
        const auto count = targets == 1 ? QStringLiteral("1 item") : QStringLiteral("%1 items").arg(targets);
        auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Delete Permanently"), QStringLiteral("Delete %1 permanently?").arg(count), QMessageBox::NoButton, workspace_);
        box->setObjectName(QStringLiteral("permanentDeleteConfirmationWindow"));
        box->setAccessibleName(QStringLiteral("Permanent Delete Confirmation Window"));
        box->setInformativeText(QStringLiteral("This can’t be undone."));
        auto *remove = box->addButton(QStringLiteral("Delete Permanently"), QMessageBox::DestructiveRole);
        auto *cancel = box->addButton(QMessageBox::Cancel);
        box->setDefaultButton(cancel);
        box->setEscapeButton(cancel);
        box->setWindowModality(Qt::NonModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, this, [this, box, remove, job, targets] {
            boxes_.remove(job);
            if (box->clickedButton() == remove)
                ops_->confirmDelete(job, targets);
            else
                ops_->cancelOperation(job);
        });
        boxes_.insert(job, box);
        box->show();
    }

    OperationsModel *ops_;
    QWidget *workspace_;
    QHash<std::int64_t, QPointer<QMessageBox>> boxes_;
};

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_OPERATION_PANELS_HPP
