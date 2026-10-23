#ifndef dual_pane_DESKTOP_TABS_STRIP_HPP
#define dual_pane_DESKTOP_TABS_STRIP_HPP

#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

/// One Browser Tab. It does not own activation: clicks and keys request it.
class BrowserTabButton final : public QAbstractButton {
  public:
    BrowserTabButton(std::uint64_t id, QWidget *parent) : QAbstractButton(parent), id_(id), close_(new QToolButton(this)) {
        setObjectName(QStringLiteral("browserTab"));
        setFocusPolicy(Qt::TabFocus);
        setMouseTracking(true);
        setAcceptDrops(true);
        close_->setObjectName(QStringLiteral("closeTabButton"));
        close_->setText(QStringLiteral("×"));
        close_->setToolTip(QStringLiteral("Close Tab"));
        close_->setAccessibleName(QStringLiteral("Close Tab Button"));
        close_->setFocusPolicy(Qt::NoFocus);
        close_->hide();
    }

    [[nodiscard]] auto id() const -> std::uint64_t { return id_; }
    [[nodiscard]] auto closeButton() const -> QToolButton * { return close_; }
    void setTab(const QString &label, const QString &path, bool active) {
        label_ = label;
        active_ = active;
        setToolTip(path);
        setAccessibleName(label);
        setAccessibleDescription(path);
        setCheckable(true);
        setChecked(active);
        updateGeometry();
        update();
    }

    [[nodiscard]] auto sizeHint() const -> QSize override { return {qMin(fontMetrics().horizontalAdvance(label_) + 40, 200), 26}; }
    [[nodiscard]] auto minimumSizeHint() const -> QSize override { return {36, 26}; }

  protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        const bool browser_active = property("browserActive").toBool() && property("windowActive").toBool();
        const char *background = inactive_surface_color;
        if (active_)
            background = browser_active ? surface_color : inactive_surface_color;
        else if (underMouse())
            background = hover_color;
        else if (browser_active)
            background = window_color;
        painter.fillRect(rect(), QColor(QString::fromLatin1(background)));
        painter.setPen(QColor(QString::fromLatin1(border_color)));
        painter.drawLine(rect().topRight(), rect().bottomRight());
        if (active_)
            painter.fillRect(QRect(0, 0, width(), 2), QColor(QString::fromLatin1(active_color)));
        if (hasFocus()) {
            painter.setPen(QColor(QString::fromLatin1(active_color)));
            painter.drawRect(rect().adjusted(1, 1, -2, -2));
        }
        const int drop_count = property("fileDropCount").toInt();
        if (drop_count > 0) {
            painter.setPen(QPen(QColor(QString::fromLatin1(active_color)), 2));
            painter.drawRect(rect().adjusted(2, 2, -3, -3));
            const auto action = property("fileDropCopy").toBool() ? QStringLiteral("Copy") : QStringLiteral("Move");
            painter.setPen(QColor(QString::fromLatin1(text_color)));
            painter.drawText(rect().adjusted(4, 0, -4, 0), Qt::AlignCenter, QStringLiteral("%1 %2").arg(action).arg(drop_count));
            return;
        }
        painter.setPen(QColor(QString::fromLatin1(browser_active ? text_color : inactive_text_color)));
        const auto text_rect = rect().adjusted(8, 0, -22, 0);
        painter.drawText(text_rect, Qt::AlignVCenter | Qt::AlignLeft, fontMetrics().elidedText(label_, Qt::ElideRight, text_rect.width()));
    }
    void resizeEvent(QResizeEvent *event) override {
        QAbstractButton::resizeEvent(event);
        close_->setGeometry(width() - 20, (height() - 16) / 2, 16, 16);
    }
    void enterEvent(QEnterEvent *event) override {
        close_->show();
        QAbstractButton::enterEvent(event);
    }
    void leaveEvent(QEvent *event) override {
        close_->hide();
        QAbstractButton::leaveEvent(event);
    }
    void keyPressEvent(QKeyEvent *event) override {
        const auto key = event->key();
        if (key == Qt::Key_Return || key == Qt::Key_Enter) {
            click();
            event->accept();
            return;
        }
        if ((key == Qt::Key_Left || key == Qt::Key_Right) && user_modifiers(event->modifiers()) == Qt::NoModifier) {
            std::vector<QAbstractButton *> ordered;
            for (auto *sibling : parentWidget()->findChildren<QAbstractButton *>(Qt::FindDirectChildrenOnly))
                if (sibling->objectName() == objectName())
                    ordered.push_back(sibling);
            std::sort(ordered.begin(), ordered.end(), [](auto *a, auto *b) { return a->x() < b->x(); });
            const auto here = std::find(ordered.begin(), ordered.end(), this);
            if (key == Qt::Key_Left && here != ordered.begin())
                (*std::prev(here))->setFocus(Qt::TabFocusReason);
            else if (key == Qt::Key_Right && std::next(here) != ordered.end())
                (*std::next(here))->setFocus(Qt::TabFocusReason);
            event->accept();
            return;
        }
        QAbstractButton::keyPressEvent(event);
    }

  private:
    std::uint64_t id_;
    QToolButton *close_;
    QString label_;
    bool active_ = false;
};

/// The ordered tabs of one Browser and its New Tab Button. Dragging a tab
/// reorders it within this strip; releasing outside the Browser leaves it.
class BrowserTabsStrip final : public QWidget {
  public:
    BrowserTabsStrip(FolderItemsListModel *model, QWidget *browser, std::function<void()> focus_list) : QWidget(browser), model_(model), browser_(browser), focus_list_(std::move(focus_list)), tabs_(new QHBoxLayout()), new_tab_(new QToolButton(this)), indicator_(new QWidget(this)) {
        setObjectName(QStringLiteral("browserTabsStrip"));
        setAccessibleName(QStringLiteral("Browser Tabs Strip"));
        setAttribute(Qt::WA_StyledBackground);
        setMinimumHeight(26);
        auto *layout = new QHBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        tabs_->setContentsMargins(0, 0, 0, 0);
        tabs_->setSpacing(0);
        layout->addLayout(tabs_, 1);
        new_tab_->setObjectName(QStringLiteral("newTabButton"));
        new_tab_->setText(QStringLiteral("+"));
        new_tab_->setAccessibleName(QStringLiteral("New Tab Button"));
        new_tab_->setFocusPolicy(Qt::TabFocus);
        layout->addWidget(new_tab_);
        indicator_->setStyleSheet(QStringLiteral("background:%1;").arg(QString::fromLatin1(active_color)));
        indicator_->hide();
        QObject::connect(new_tab_, &QToolButton::clicked, this, [this] {
            focus_list_();
            model_->newTab();
        });
        QObject::connect(model_, &FolderItemsListModel::tabsRevisionChanged, this, [this] { rebuild(); });
        QObject::connect(model_, &FolderItemsListModel::toolbarStateChanged, this, [this] { update_new_tab(); });
        rebuild();
    }

  private:
    static void clear_file_drop(BrowserTabButton *button) {
        button->setProperty("fileDropCount", 0);
        button->setProperty("fileDropCopy", false);
        button->setAccessibleDescription(button->toolTip());
        button->update();
    }
    void rebuild() {
        if (dragging_)
            cancel_drag();
        const int count = model_->tabCount();
        // Reuse buttons by stable ID so focus and hover survive a relabel.
        QHash<std::uint64_t, BrowserTabButton *> existing;
        for (auto *button : buttons_)
            existing.insert(button->id(), button);
        std::vector<BrowserTabButton *> ordered;
        for (int index = 0; index < count; ++index) {
            const auto id = model_->tabId(index);
            auto *button = existing.take(id);
            if (button == nullptr) {
                button = new BrowserTabButton(id, this);
                button->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
                button->installEventFilter(this);
                QObject::connect(button, &QAbstractButton::clicked, this, [this, id] {
                    focus_list_();
                    model_->activateTab(id);
                });
                QObject::connect(button->closeButton(), &QToolButton::clicked, this, [this, id] {
                    focus_list_();
                    model_->closeTab(id);
                });
            }
            button->setTab(model_->tabLabel(index), model_->tabPath(index), model_->tabIsActive(index));
            tabs_->removeWidget(button);
            tabs_->insertWidget(index, button);
            ordered.push_back(button);
        }
        for (auto *stale : existing)
            stale->deleteLater();
        buttons_ = std::move(ordered);
        update_new_tab();
    }
    void update_new_tab() {
        new_tab_->setEnabled(has_flag(model_->getToolbarState(), toolbar_new_tab));
        new_tab_->setToolTip(model_->tabCount() >= tab_limit ? QStringLiteral("Tab limit reached: a Browser holds at most %1 tabs").arg(tab_limit) : QStringLiteral("New Tab"));
    }
    /// The insertion slot, among the current tabs, under strip position `x`.
    [[nodiscard]] auto slot_at(int x) const -> int {
        int slot = 0;
        for (const auto *button : buttons_)
            if (button->geometry().center().x() < x)
                ++slot;
        return slot;
    }
    void cancel_drag() {
        dragging_ = false;
        pressed_ = nullptr;
        indicator_->hide();
    }
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        auto *button = dynamic_cast<BrowserTabButton *>(watched);
        if (button == nullptr)
            return QWidget::eventFilter(watched, event);
        if (event->type() == QEvent::DragLeave) {
            clear_file_drop(button);
            event->accept();
            return true;
        }
        if (event->type() == QEvent::DragEnter || event->type() == QEvent::DragMove || event->type() == QEvent::Drop) {
            auto *drop = dynamic_cast<QDropEvent *>(event);
            const auto drag = internal_file_drag(drop);
            const int source_browser = drop == nullptr || drop->source() == nullptr ? -1 : drop->source()->property("dualPaneBrowser").toInt();
            const bool supported = drop != nullptr && drop_modifiers_supported(drop->modifiers());
            const bool copy = supported && drop_is_copy(drop->modifiers());
            const int count = drag && supported ? model_->previewFileDrop(*drag, source_browser, 2, button->id(), -1, copy) : -1;
            if (event->type() == QEvent::Drop) {
                clear_file_drop(button);
                if (drag && count > 0 && model_->acceptFileDrop(*drag, source_browser, 2, button->id(), -1, copy)) {
                    drop->setDropAction(copy ? Qt::CopyAction : Qt::MoveAction);
                    drop->accept();
                } else if (drop != nullptr) {
                    drop->ignore();
                }
                return true;
            }
            if (drop == nullptr || count <= 0) {
                clear_file_drop(button);
                if (drop != nullptr)
                    drop->ignore();
                return true;
            }
            button->setProperty("fileDropCount", count);
            button->setProperty("fileDropCopy", copy);
            const auto description = QStringLiteral("%1 %2 %3 into %4").arg(copy ? QStringLiteral("Copy") : QStringLiteral("Move")).arg(count).arg(count == 1 ? QStringLiteral("Item") : QStringLiteral("Items")).arg(button->toolTip());
            const bool changed = button->accessibleDescription() != description;
            button->setAccessibleDescription(description);
            if (changed) {
                QAccessibleAnnouncementEvent announcement(button, description);
                QAccessible::updateAccessibility(&announcement);
            }
            button->update();
            drop->setDropAction(copy ? Qt::CopyAction : Qt::MoveAction);
            drop->accept();
            return true;
        }
        const auto *mouse = dynamic_cast<QMouseEvent *>(event);
        if (event->type() == QEvent::MouseButtonPress && mouse != nullptr && mouse->button() == Qt::LeftButton) {
            pressed_ = button;
            press_position_ = mouse->globalPosition().toPoint();
        } else if (event->type() == QEvent::MouseMove && mouse != nullptr && pressed_ == button) {
            if (!dragging_ && (mouse->globalPosition().toPoint() - press_position_).manhattanLength() >= QApplication::startDragDistance())
                dragging_ = true;
            if (dragging_) {
                const int x = mapFromGlobal(mouse->globalPosition().toPoint()).x();
                const int slot = slot_at(x);
                int indicator_x = 0;
                if (slot < static_cast<int>(buttons_.size()))
                    indicator_x = buttons_.at(slot)->x();
                else if (!buttons_.empty())
                    indicator_x = buttons_.back()->geometry().right();
                indicator_->setGeometry(qMax(0, indicator_x - 1), 0, 2, height());
                indicator_->show();
                indicator_->raise();
                return true;
            }
        } else if (event->type() == QEvent::MouseButtonRelease && mouse != nullptr && pressed_ == button) {
            const bool dragged = dragging_;
            const auto global = mouse->globalPosition().toPoint();
            cancel_drag();
            if (dragged) {
                button->setDown(false);
                // A tab released outside its Browser stays where it was.
                if (browser_->rect().contains(browser_->mapFromGlobal(global))) {
                    const auto current = static_cast<int>(std::find(buttons_.begin(), buttons_.end(), button) - buttons_.begin());
                    int slot = slot_at(mapFromGlobal(global).x());
                    if (slot > current)
                        --slot;
                    model_->reorderTab(button->id(), slot);
                }
                return true;
            }
        }
        return QWidget::eventFilter(watched, event);
    }

    FolderItemsListModel *model_;
    QWidget *browser_;
    std::function<void()> focus_list_;
    QHBoxLayout *tabs_;
    QToolButton *new_tab_;
    QWidget *indicator_;
    std::vector<BrowserTabButton *> buttons_;
    BrowserTabButton *pressed_ = nullptr;
    QPoint press_position_;
    bool dragging_ = false;
};

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_TABS_STRIP_HPP
