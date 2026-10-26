#ifndef dual_pane_DESKTOP_FAVORITES_PANEL_HPP
#define dual_pane_DESKTOP_FAVORITES_PANEL_HPP

#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/sidebar_symbols.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

/// An inline name editor. Return commits, Escape cancels, and focus loss
/// commits valid text or silently cancels. While it has focus, only Quit
/// stays bound.
class InlineNameEditor final : public QLineEdit {
  public:
    /// `commit` returns inline error text, empty when the edit is done.
    InlineNameEditor(const QString &object_name, const QString &accessible_name, const QString &text, std::function<QString(const QString &)> commit, std::function<void()> finish, QLabel *error, QWidget *parent) : QLineEdit(text, parent), commit_(std::move(commit)), finish_(std::move(finish)), error_(error) {
        setObjectName(object_name);
        setAccessibleName(accessible_name);
        selectAll();
    }

  protected:
    void keyPressEvent(QKeyEvent *event) override {
        if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
            event->accept();
            const auto error = commit_(text());
            if (error.isEmpty())
                close_editor();
            else
                show_error(error);
            return;
        }
        if (event->key() == Qt::Key_Escape) {
            event->accept();
            close_editor();
            return;
        }
        QLineEdit::keyPressEvent(event);
    }
    void focusOutEvent(QFocusEvent *event) override {
        QLineEdit::focusOutEvent(event);
        if (!closed_ && event->reason() != Qt::ActiveWindowFocusReason && event->reason() != Qt::PopupFocusReason) {
            commit_(text());
            close_editor();
        }
    }

  private:
    void show_error(const QString &error) {
        error_->setText(error);
        error_->show();
        setFocus();
    }
    void close_editor() {
        if (closed_)
            return;
        closed_ = true;
        QTimer::singleShot(0, this, [finish = finish_] { finish(); });
    }

    std::function<QString(const QString &)> commit_;
    std::function<void()> finish_;
    QLabel *error_;
    bool closed_ = false;
};

/// A Favorite Item row: a single click opens it, right-click shows its
/// context menu, and dragging moves it.
class FavoriteItemRow final : public QLabel {
  public:
    FavoriteItemRow(std::int64_t id, const QString &alias, const QString &path, const QString &home, QWidget *parent) : QLabel(alias, parent), id_(id), symbol_(favorite_symbol_image(favorite_symbol_name(path, home))) {
        if (!symbol_.isNull()) {
            symbol_hover_ = symbol_.copy();
            symbol_focus_ = symbol_.copy();
            tint_symbol(symbol_, inactive_text_color);
            tint_symbol(symbol_hover_, text_color);
            tint_symbol(symbol_focus_, active_color);
        }
        setObjectName(QStringLiteral("favoriteItem"));
        setAccessibleName(alias);
        setAccessibleDescription(path);
        setToolTip(path);
        setFocusPolicy(Qt::TabFocus);
        setIndent(22);
        setContentsMargins(favorite_row_horizontal_padding, favorite_row_vertical_padding, favorite_row_horizontal_padding, favorite_row_vertical_padding);
        setMouseTracking(true);
    }
    [[nodiscard]] auto id() const -> std::int64_t { return id_; }
    void setOpenHandler(std::function<void()> handler) { on_open_ = std::move(handler); }
    void setMenuHandler(std::function<void(const QPoint &)> handler) { on_menu_ = std::move(handler); }
    /// Moves focus by `step` Items, for arrow keys under Full Keyboard Access.
    void setStepHandler(std::function<void(int)> handler) { on_step_ = std::move(handler); }

  protected:
    void enterEvent(QEnterEvent *event) override {
        hovered_ = true;
        update();
        QLabel::enterEvent(event);
    }
    void leaveEvent(QEvent *event) override {
        hovered_ = false;
        update();
        QLabel::leaveEvent(event);
    }
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton)
            press_ = event->position().toPoint();
        event->accept();
    }
    void mouseMoveEvent(QMouseEvent *event) override {
        if (!press_ || (event->position().toPoint() - *press_).manhattanLength() < QApplication::startDragDistance())
            return;
        press_.reset();
        auto *data = new QMimeData;
        data->setData(QString::fromLatin1(favorite_item_mime), QByteArray::number(static_cast<qlonglong>(id_)));
        auto *drag = new QDrag(this);
        drag->setMimeData(data);
        drag->exec(Qt::MoveAction);
    }
    void mouseReleaseEvent(QMouseEvent *event) override {
        event->accept();
        if (event->button() == Qt::LeftButton && press_ && rect().contains(event->position().toPoint()) && on_open_)
            on_open_();
        press_.reset();
    }
    void contextMenuEvent(QContextMenuEvent *event) override {
        if (on_menu_)
            on_menu_(event->globalPos());
    }
    void keyPressEvent(QKeyEvent *event) override {
        if ((event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) && on_open_) {
            on_open_();
        } else if ((event->key() == Qt::Key_Up || event->key() == Qt::Key_Down) && on_step_) {
            on_step_(event->key() == Qt::Key_Up ? -1 : 1);
        } else {
            QLabel::keyPressEvent(event);
            return;
        }
        event->accept();
    }
    void paintEvent(QPaintEvent *event) override {
        if (hovered_ && !hasFocus()) {
            QPainter painter(this);
            painter.fillRect(rect(), QColor(QString::fromLatin1(hover_color)));
        }
        QLabel::paintEvent(event);
        if (!symbol_.isNull()) {
            QPainter painter(this);
            const QImage *image = &symbol_;
            if (hasFocus())
                image = &symbol_focus_;
            else if (hovered_)
                image = &symbol_hover_;
            painter.drawImage(QRect(favorite_row_horizontal_padding, (height() - 16) / 2, 16, 16), *image);
        }
        if (hasFocus()) {
            QPainter painter(this);
            painter.setPen(QColor(QString::fromLatin1(active_color)));
            painter.drawRect(rect().adjusted(0, 0, -1, -1));
        }
    }

  private:
    static void tint_symbol(QImage &image, const char *color) {
        QPainter painter(&image);
        painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
        painter.fillRect(image.rect(), QColor(QString::fromLatin1(color)));
    }

    std::int64_t id_;
    QImage symbol_;
    QImage symbol_hover_;
    QImage symbol_focus_;
    std::optional<QPoint> press_;
    bool hovered_ = false;
    std::function<void()> on_open_;
    std::function<void(const QPoint &)> on_menu_;
    std::function<void(int)> on_step_;
};

/// A Favorite Group row that keeps its neutral hover separate from its
/// blue-accent child command buttons.
class FavoriteGroupRow final : public QWidget {
  public:
    explicit FavoriteGroupRow(QWidget *parent) : QWidget(parent) { setMouseTracking(true); }

    void addCommandButton(QToolButton *button) { button->installEventFilter(this); }

  protected:
    void enterEvent(QEnterEvent *event) override {
        hovered_ = true;
        update();
        QWidget::enterEvent(event);
    }
    void leaveEvent(QEvent *event) override {
        hovered_ = false;
        update();
        QWidget::leaveEvent(event);
    }
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if (qobject_cast<QToolButton *>(watched) != nullptr && (event->type() == QEvent::Enter || event->type() == QEvent::Leave)) {
            command_hovered_ = event->type() == QEvent::Enter;
            update();
        }
        return QWidget::eventFilter(watched, event);
    }
    void paintEvent(QPaintEvent *event) override {
        if (hovered_ && !command_hovered_) {
            QPainter painter(this);
            painter.fillRect(rect(), QColor(QString::fromLatin1(hover_color)));
        }
        QWidget::paintEvent(event);
    }

  private:
    bool hovered_ = false;
    bool command_hovered_ = false;
};

/// The Sidebar's Favorite Groups. It renders the application-owned order and
/// submits every edit by stable ID; it never reorders rows itself.
class FavoritesPanel final : public QWidget {
  public:
    FavoritesPanel(WorkspaceBridge *bridge, std::function<void()> focus_active_list, QWidget *parent) : QWidget(parent), bridge_(bridge), focus_active_list_(std::move(focus_active_list)), layout_(new QVBoxLayout(this)), indicator_(new QWidget(this)) {
        setObjectName(QStringLiteral("favoritesGroups"));
        setAccessibleName(QStringLiteral("Favorites Groups"));
        setAcceptDrops(true);
        layout_->setContentsMargins(0, 0, 0, 0);
        layout_->setSpacing(0);
        indicator_->setStyleSheet(QStringLiteral("background:%1;").arg(QString::fromLatin1(active_color)));
        indicator_->hide();
        QObject::connect(bridge_, &WorkspaceBridge::favoritesRevisionChanged, this, [this] { rebuild(); });
        QObject::connect(bridge_, &WorkspaceBridge::favoritesReadyChanged, this, [this] { rebuild(); });
        rebuild();
    }

  protected:
    void dragEnterEvent(QDragEnterEvent *event) override { accept_drag(event); }
    void dragMoveEvent(QDragMoveEvent *event) override {
        if (!accept_drag(event))
            return;
        const auto target = drop_target(event->position().toPoint());
        indicator_->setGeometry(0, target.y - 1, width(), 2);
        indicator_->show();
        indicator_->raise();
    }
    void dragLeaveEvent(QDragLeaveEvent *) override { indicator_->hide(); }
    void dropEvent(QDropEvent *event) override {
        indicator_->hide();
        if (!accept_drag(event))
            return;
        const auto id = event->mimeData()->data(QString::fromLatin1(favorite_item_mime)).toLongLong();
        const auto target = drop_target(event->position().toPoint());
        if (target.group < 0)
            return;
        const auto error = bridge_->dropItem(id, target.group, target.slot);
        if (!error.isEmpty())
            show_cue(target.group, error);
    }

  private:
    struct Section {
        std::int64_t group;
        QWidget *header;
        std::vector<FavoriteItemRow *> items;
    };
    struct DropTarget {
        std::int64_t group = -1;
        int slot = 0;
        int y = 0;
    };
    enum class EditKind : std::uint8_t { Group,
                                         Item,
                                         Draft,
    };

    /// Accepts only Favorite Items dragged from this panel.
    auto accept_drag(QDropEvent *event) -> bool {
        const auto *source = dynamic_cast<FavoriteItemRow *>(event->source());
        if (source == nullptr || !isAncestorOf(source) || !event->mimeData()->hasFormat(QString::fromLatin1(favorite_item_mime)) || !bridge_->getFavoritesReady()) {
            event->ignore();
            return false;
        }
        event->setDropAction(Qt::MoveAction);
        event->accept();
        return true;
    }
    [[nodiscard]] auto drop_target(QPoint position) const -> DropTarget {
        DropTarget target;
        for (const auto &section : sections_) {
            const auto header = QRect(section.header->mapTo(this, QPoint()), section.header->size());
            if (position.y() < header.top() && target.group >= 0)
                break;
            target = {section.group, 0, header.bottom() + 1};
            for (const auto *item : section.items) {
                const auto item_rect = QRect(item->mapTo(this, QPoint()), item->size());
                if (position.y() < item_rect.center().y())
                    return target;
                target.slot += 1;
                target.y = item_rect.bottom() + 1;
            }
        }
        return target;
    }

    /// Recreates every row. While an editor is open, a rebuild waits until it
    /// closes unless the edit itself asks for one.
    void rebuild(bool force = false) {
        if (editing_ && !force)
            return;
        sections_.clear();
        cues_.clear();
        while (auto *child = layout_->takeAt(0)) {
            if (auto *widget = child->widget())
                widget->deleteLater();
            delete child;
        }
        const bool ready = bridge_->getFavoritesReady();
        std::vector<FavoriteItemRow *> all_items;
        for (int group = 0; group < bridge_->groupCount(); ++group) {
            const auto id = bridge_->groupId(group);
            const auto name = bridge_->groupName(group);
            auto *container = new QWidget(this);
            auto *container_layout = new QVBoxLayout(container);
            container_layout->setContentsMargins(favorite_group_margin, favorite_group_margin, favorite_group_margin, favorite_group_margin);
            container_layout->setSpacing(0);
            auto *box = new QFrame(container);
            box->setObjectName(QStringLiteral("favoriteGroupBox"));
            auto *box_layout = new QVBoxLayout(box);
            box_layout->setContentsMargins(0, 0, 0, 0);
            box_layout->setSpacing(0);
            container_layout->addWidget(box);
            auto *header = new FavoriteGroupRow(box);
            header->setObjectName(QStringLiteral("favoriteGroup"));
            auto *row = new QHBoxLayout(header);
            row->setContentsMargins(favorite_row_horizontal_padding, favorite_row_vertical_padding, favorite_row_horizontal_padding, favorite_row_vertical_padding);
            row->setSpacing(0);
            if (editing_kind_ == EditKind::Group && editing_id_ == id) {
                add_editor(row, header, QStringLiteral("favoriteGroupNameEditor"), QStringLiteral("Favorite Group Name Editor"), name, [this, id](const QString &text) { return bridge_->renameGroup(id, text); });
            } else {
                auto *label = new QLabel(name, header);
                label->setObjectName(QStringLiteral("favoriteGroupName"));
                label->setAccessibleName(name);
                row->addWidget(label, 1);
            }
            auto *menu_button = command_button(QStringLiteral("favoriteGroupMenuButton"), QStringLiteral("⋯"), QStringLiteral("Group Actions"), QStringLiteral("Favorite Group Menu Button"), header);
            menu_button->setFocusPolicy(Qt::TabFocus);
            menu_button->setPopupMode(QToolButton::InstantPopup);
            menu_button->setStyleSheet(QStringLiteral("QToolButton::menu-indicator { image:none; width:0px; }"));
            menu_button->setEnabled(ready);
            header->addCommandButton(menu_button);
            auto *menu = new QMenu(menu_button);
            menu->setObjectName(QStringLiteral("favoriteGroupMenu"));
            menu->setAccessibleName(QStringLiteral("Favorite Group Menu"));
            menu->addAction(QStringLiteral("Rename"), this, [this, id] { begin_edit(EditKind::Group, id); });
            menu->addSeparator();
            menu->addAction(QStringLiteral("Move Up"), this, [this, id] { bridge_->moveGroup(id, 0); });
            menu->addAction(QStringLiteral("Move Down"), this, [this, id] { bridge_->moveGroup(id, 1); });
            menu->addAction(QStringLiteral("Move to Top"), this, [this, id] { bridge_->moveGroup(id, 2); });
            menu->addAction(QStringLiteral("Move to Bottom"), this, [this, id] { bridge_->moveGroup(id, 3); });
            menu->addSeparator();
            menu->addAction(QStringLiteral("Delete"), this, [this, id] { bridge_->deleteGroup(id); });
            menu_button->setMenu(menu);
            row->addWidget(menu_button);
            auto *add = command_button(QStringLiteral("addFavoriteItemButton"), QStringLiteral("+"), QStringLiteral("Add the current folder"), QStringLiteral("Add Favorite Item Button"), header);
            add->setFocusPolicy(Qt::TabFocus);
            add->setEnabled(ready);
            header->addCommandButton(add);
            QObject::connect(add, &QToolButton::clicked, this, [this, id] {
                const auto error = bridge_->addItem(id);
                if (!error.isEmpty())
                    show_cue(id, error);
            });
            row->addWidget(add);
            box_layout->addWidget(header);
            auto *cue = new QLabel(box);
            cue->setObjectName(QStringLiteral("favoriteCue"));
            cue->hide();
            box_layout->addWidget(cue);
            cues_.insert(id, cue);
            Section section{id, header, {}};
            for (int item = 0; item < bridge_->itemCount(group); ++item) {
                const auto item_id = bridge_->itemId(group, item);
                const auto alias = bridge_->itemAlias(group, item);
                if (editing_kind_ == EditKind::Item && editing_id_ == item_id) {
                    auto *holder = new QWidget(box);
                    auto *holder_layout = new QHBoxLayout(holder);
                    holder_layout->setContentsMargins(favorite_row_horizontal_padding, favorite_row_vertical_padding, favorite_row_horizontal_padding, favorite_row_vertical_padding);
                    holder_layout->addSpacing(22);
                    add_editor(holder_layout, holder, QStringLiteral("favoriteItemAliasEditor"), QStringLiteral("Favorite Item Alias Editor"), alias, [this, item_id](const QString &text) { return bridge_->renameItem(item_id, text); });
                    box_layout->addWidget(holder);
                    continue;
                }
                auto *row_widget = new FavoriteItemRow(item_id, alias, bridge_->itemPath(group, item), bridge_->getFavoritesHome(), box);
                row_widget->setOpenHandler([this, item_id] {
                    bridge_->openItem(item_id);
                    focus_active_list_();
                });
                row_widget->setMenuHandler([this, item_id](const QPoint &at) { show_item_menu(item_id, at); });
                box_layout->addWidget(row_widget);
                section.items.push_back(row_widget);
                all_items.push_back(row_widget);
            }
            sections_.push_back(std::move(section));
            layout_->addWidget(container);
        }
        for (std::size_t index = 0; index < all_items.size(); ++index) {
            all_items.at(index)->setStepHandler([all_items, index](int step) {
                const auto next = static_cast<std::ptrdiff_t>(index) + step;
                if (next >= 0 && next < static_cast<std::ptrdiff_t>(all_items.size()))
                    all_items.at(static_cast<std::size_t>(next))->setFocus(Qt::TabFocusReason);
            });
        }
        if (editing_kind_ == EditKind::Draft) {
            auto *holder = new QWidget(this);
            auto *holder_layout = new QHBoxLayout(holder);
            holder_layout->setContentsMargins(favorite_row_horizontal_padding, favorite_row_vertical_padding, favorite_row_horizontal_padding, favorite_row_vertical_padding);
            add_editor(holder_layout, holder, QStringLiteral("favoriteGroupNameEditor"), QStringLiteral("Favorite Group Name Editor"), QString(), [this](const QString &text) { return bridge_->createGroup(text); });
            layout_->addWidget(holder);
        }
        auto *new_group_holder = new QWidget(this);
        auto *new_group_layout = new QHBoxLayout(new_group_holder);
        new_group_layout->setContentsMargins(favorite_group_margin, 0, favorite_group_margin, favorite_group_margin);
        auto *new_group = command_button(QStringLiteral("newGroupButton"), QStringLiteral("New Group"), QStringLiteral("New Group"), QStringLiteral("New Group Button"), new_group_holder);
        new_group->setFocusPolicy(Qt::TabFocus);
        new_group->setEnabled(ready && !editing_);
        QObject::connect(new_group, &QToolButton::clicked, this, [this] { begin_edit(EditKind::Draft, -1); });
        new_group_layout->addWidget(new_group);
        layout_->addWidget(new_group_holder);
        if (editor_ != nullptr)
            editor_->setFocus(Qt::OtherFocusReason);
    }
    void begin_edit(EditKind kind, std::int64_t id) {
        if (!bridge_->getFavoritesReady())
            return;
        editing_kind_ = kind;
        editing_id_ = id;
        editing_ = true;
        editor_ = nullptr;
        rebuild(true);
        // The Group or Item may be gone by now; then there is nothing to edit.
        if (editor_ == nullptr) {
            editing_ = false;
            editing_kind_.reset();
            editing_id_ = -1;
            rebuild();
        }
    }
    void add_editor(QHBoxLayout *row, QWidget *holder, const QString &object_name, const QString &accessible_name, const QString &text, std::function<QString(const QString &)> commit) {
        auto *column = new QVBoxLayout();
        column->setContentsMargins(0, 0, 0, 0);
        auto *error = new QLabel(holder);
        error->setObjectName(QStringLiteral("favoriteError"));
        error->hide();
        editor_ = new InlineNameEditor(object_name, accessible_name, text, std::move(commit), [this] { end_edit(); }, error, holder);
        column->addWidget(editor_);
        column->addWidget(error);
        row->addLayout(column, 1);
    }
    void end_edit() {
        editing_ = false;
        editing_kind_.reset();
        editing_id_ = -1;
        editor_ = nullptr;
        rebuild();
    }
    void show_item_menu(std::int64_t id, const QPoint &at) {
        QMenu menu(this);
        menu.setObjectName(QStringLiteral("favoriteItemContextMenu"));
        menu.setAccessibleName(QStringLiteral("Favorite Item Context Menu"));
        auto *remove = menu.addAction(QStringLiteral("Remove from Group"));
        auto *rename = menu.addAction(QStringLiteral("Change Alias"));
        remove->setEnabled(bridge_->getFavoritesReady());
        rename->setEnabled(bridge_->getFavoritesReady());
        const auto *chosen = menu.exec(at);
        if (chosen == remove)
            bridge_->removeItem(id);
        else if (chosen == rename)
            begin_edit(EditKind::Item, id);
    }
    /// Shows brief inline feedback under a Group row, without a Notice.
    void show_cue(std::int64_t group, const QString &text) {
        auto *cue = cues_.value(group);
        if (cue == nullptr)
            return;
        cue->setText(text);
        cue->show();
        QTimer::singleShot(2500, cue, [cue] { cue->hide(); });
    }

    WorkspaceBridge *bridge_;
    std::function<void()> focus_active_list_;
    QVBoxLayout *layout_;
    QWidget *indicator_;
    std::vector<Section> sections_;
    QHash<std::int64_t, QLabel *> cues_;
    InlineNameEditor *editor_ = nullptr;
    std::optional<EditKind> editing_kind_;
    std::int64_t editing_id_ = -1;
    bool editing_ = false;
    bool capture_ = false;
};

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_FAVORITES_PANEL_HPP
