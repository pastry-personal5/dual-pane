#ifndef dual_pane_DESKTOP_FOLDER_PANE_HPP
#define dual_pane_DESKTOP_FOLDER_PANE_HPP

#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/quick_look_preview.hpp"
#include "dual_pane_desktop/tabs_strip.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

class ThinSplitterHandle final : public QSplitterHandle {
  public:
    ThinSplitterHandle(Qt::Orientation orientation, QSplitter *parent, const QString &accessible_name, std::function<void()> settled) : QSplitterHandle(orientation, parent), settled_(std::move(settled)) {
        setAccessibleName(accessible_name);
        setFocusPolicy(orientation == Qt::Vertical ? Qt::TabFocus : Qt::NoFocus);
    }

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
            if (settled_)
                settled_();
        }
    }

    void keyPressEvent(QKeyEvent *event) override {
        if (orientation() != Qt::Vertical || (event->key() != Qt::Key_Up && event->key() != Qt::Key_Down)) {
            QSplitterHandle::keyPressEvent(event);
            return;
        }
        const int step = event->modifiers().testFlag(Qt::ShiftModifier) ? 1 : 10;
        const int direction = event->key() == Qt::Key_Up ? -1 : 1;
        moveSplitter(splitter()->sizes().value(0) + (direction * step));
        if (settled_)
            settled_();
        event->accept();
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
    std::function<void()> settled_;
};

enum class SplitterKind : std::uint8_t { Sidebar,
                                         Browser,
                                         Preview,
};

class ThinSplitter final : public QSplitter {
  public:
    explicit ThinSplitter(SplitterKind kind, QWidget *parent = nullptr) : QSplitter(kind == SplitterKind::Preview ? Qt::Vertical : Qt::Horizontal, parent), handle_accessible_name_(accessible_name(kind)) {
        setObjectName(object_name(kind));
        setHandleWidth(splitter_width);
        setChildrenCollapsible(false);
    }
    void set_settled_handler(std::function<void()> handler) { settled_ = std::move(handler); }

  protected:
    auto createHandle() -> QSplitterHandle * override {
        return new ThinSplitterHandle(orientation(), this, handle_accessible_name_, [this] { if (settled_) settled_(); });
    }

  private:
    static auto accessible_name(SplitterKind kind) -> QString {
        switch (kind) {
        case SplitterKind::Sidebar:
            return QStringLiteral("Sidebar Splitter");
        case SplitterKind::Browser:
            return QStringLiteral("Browser Divider");
        case SplitterKind::Preview:
            return QStringLiteral("Preview Pane Splitter");
        }
        return {};
    }
    static auto object_name(SplitterKind kind) -> QString {
        switch (kind) {
        case SplitterKind::Sidebar:
            return QStringLiteral("sidebarSplitter");
        case SplitterKind::Browser:
            return QStringLiteral("browserDivider");
        case SplitterKind::Preview:
            return QStringLiteral("previewPaneSplitter");
        }
        return {};
    }
    QString handle_accessible_name_;
    std::function<void()> settled_;
};

/// Loads native item icons on one low-priority worker thread. Results return
/// to the requesting object on the GUI thread; a request whose generation is
/// stale when it starts is skipped.
class IconLoader final {
  public:
    IconLoader() {
        thread_.setObjectName(QStringLiteral("item-icons"));
        worker_.moveToThread(&thread_);
        thread_.start(QThread::LowPriority);
    }
    ~IconLoader() {
        thread_.quit();
        thread_.wait();
    }
    IconLoader(const IconLoader &) = delete;
    auto operator=(const IconLoader &) -> IconLoader & = delete;

    void request(QObject *receiver, const QString &path, qreal ratio, std::shared_ptr<std::atomic_int> generation, int expected, std::function<void(const QImage &)> deliver) {
        QMetaObject::invokeMethod(
            &worker_, [receiver = QPointer<QObject>(receiver), path, ratio, generation = std::move(generation), expected, deliver = std::move(deliver)] {
                QImage image;
                // Cocoa supports pixmaps off the GUI thread, so the native
                // icon is rendered here rather than while painting.
                if (generation->load() == expected)
                    image = QAbstractFileIconProvider().icon(QFileInfo(path)).pixmap(QSize(16, 16), ratio).toImage();
                if (receiver)
                    QMetaObject::invokeMethod(receiver, [deliver, image] { deliver(image); }, Qt::QueuedConnection);
            },
            Qt::QueuedConnection);
    }

  private:
    QThread thread_;
    QObject worker_;
};

/// Paints the Item Icon Column from a bounded per-Browser cache, requesting a
/// missing icon only for a row that is painted.
class ItemIconDelegate final : public QStyledItemDelegate {
  public:
    ItemIconDelegate(IconLoader *loader, QAbstractItemView *view) : QStyledItemDelegate(view), loader_(loader), view_(view), generation_(std::make_shared<std::atomic_int>(0)) { cache_.setMaxCost(icon_cache_size); }

    /// Drops requests for the previous location. Cached icons stay.
    void invalidate() {
        generation_->fetch_add(1);
        pending_.clear();
    }

    void paint(QPainter *painter, const QStyleOptionViewItem &option, const QModelIndex &index) const override {
        auto cell_option = option;
        const bool pending_cut = index.data(pending_cut_role).toBool();
        if (pending_cut) {
            auto text = cell_option.palette.color(QPalette::Text);
            auto highlighted = cell_option.palette.color(QPalette::HighlightedText);
            text.setAlphaF(0.45);
            highlighted.setAlphaF(0.55);
            cell_option.palette.setColor(QPalette::Text, text);
            cell_option.palette.setColor(QPalette::HighlightedText, highlighted);
        }
        if (index.column() == 3 || index.column() == 4) {
            auto date_font = cell_option.font;
            date_font.setPointSizeF(date_font.pointSizeF() - 2.0);
            cell_option.font = std::move(date_font);
        }
        if (index.column() == 4) {
            auto fixed_font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
            fixed_font.setPointSizeF(cell_option.font.pointSizeF());
            cell_option.font = std::move(fixed_font);
        }
        if (index.column() == 3 || index.column() == 5)
            cell_option.displayAlignment = Qt::AlignRight | Qt::AlignVCenter;
        if (index.column() == 3) {
            // Let Qt paint the row selection and focus first. The inset keeps
            // those states visible around the age-colored text background.
            initStyleOption(&cell_option, index);
            const auto date_text = cell_option.text;
            cell_option.text.clear();
            const auto *widget = cell_option.widget;
            auto *style = widget != nullptr ? widget->style() : QApplication::style();
            style->drawControl(QStyle::CE_ItemViewItem, &cell_option, painter, widget);
            const auto age = index.data(relative_age_role);
            const auto position = std::clamp(age.toDouble() / 1'000'000.0, 0.0, 1.0);
            const bool active = view_->property("browserActive").toBool() && view_->property("windowActive").toBool();
            const auto background = age.isValid() ? QColor::fromHslF(static_cast<float>((10.0 + (250.0 * position)) / 360.0), active ? 0.55F : 0.20F, active ? 0.8F : 0.60F) : QColor(active ? QStringLiteral("#D0D0D0") : QStringLiteral("#888888"));
            const auto swatch = option.rect.adjusted(2, 2, -2, -2);
            painter->save();
            painter->fillRect(swatch, background);
            painter->setClipRect(swatch);
            auto date_color = QColor(Qt::black);
            if (pending_cut)
                date_color.setAlphaF(0.45);
            painter->setPen(date_color);
            painter->setFont(cell_option.font);
            painter->drawText(swatch.adjusted(2, 0, -2, 0), Qt::AlignRight | Qt::AlignVCenter, date_text);
            painter->restore();
            return;
        }
        QStyledItemDelegate::paint(painter, cell_option, index);
        if (index.column() != 0)
            return;
        const auto path = index.data(path_role).toString();
        if (path.isEmpty())
            return;
        if (const auto *pixmap = cache_.object(path)) {
            if (!pixmap->isNull()) {
                const auto size = pixmap->deviceIndependentSize().toSize();
                const QRect target(option.rect.x() + ((option.rect.width() - size.width()) / 2), option.rect.y() + ((option.rect.height() - size.height()) / 2), size.width(), size.height());
                painter->save();
                if (pending_cut)
                    painter->setOpacity(0.45);
                painter->drawPixmap(target, *pixmap);
                painter->restore();
            }
            return;
        }
        if (pending_.contains(path) || pending_.size() >= max_pending_icons)
            return;
        pending_.insert(path, QPersistentModelIndex(index));
        auto *self = const_cast<ItemIconDelegate *>(this); // NOLINT(cppcoreguidelines-pro-type-const-cast)
        const int expected = generation_->load();
        loader_->request(self, path, view_->devicePixelRatioF(), generation_, expected, [self, path, expected](const QImage &image) { self->deliver(path, expected, image); });
    }

    auto sizeHint(const QStyleOptionViewItem &option, const QModelIndex &index) const -> QSize override {
        auto size = QStyledItemDelegate::sizeHint(option, index);
        if (index.column() == 0)
            size.setWidth(icon_column_width);
        return size.expandedTo(QSize(0, 20));
    }

  private:
    void deliver(const QString &path, int expected, const QImage &image) {
        if (expected != generation_->load())
            return;
        const auto index = pending_.take(path);
        // A null image is cached too, so an icon that cannot load is not
        // requested again on every paint.
        auto *pixmap = new QPixmap(QPixmap::fromImage(image));
        pixmap->setDevicePixelRatio(view_->devicePixelRatioF());
        cache_.insert(path, pixmap);
        if (index.isValid())
            view_->viewport()->update(view_->visualRect(index));
    }

    IconLoader *loader_;
    QAbstractItemView *view_;
    std::shared_ptr<std::atomic_int> generation_;
    mutable QCache<QString, QPixmap> cache_;
    mutable QHash<QString, QPersistentModelIndex> pending_;
};

/// Column minimum widths and the order in which narrowing hides them: Size,
/// Exact Date, Relative Date, Type, then the Item Icon Column.
inline constexpr std::array<int, column_count> column_minimum = {icon_column_width, 120, type_column_width, relative_date_column_width, 118, 72};
inline constexpr std::array<int, column_count - 1> hide_order = {5, 4, 3, 2, 0};

class FolderItemsList final : public QTreeView {
  public:
    FolderItemsList(FolderItemsListModel *model, IconLoader *loader) : model_(model), icons_(new ItemIconDelegate(loader, this)), quick_look_(std::make_unique<QuickLookPreviewController>(this)), quick_look_gesture_(quick_look_.get(), [this] {
                                                                                                                                                                                                                 if (quick_look_failure_handler_)
                                                                                                                                                                                                                     quick_look_failure_handler_(); }, [this](QuickLookGesture::Release release) {
                                                                                                                                                                                                                 stop_quick_look_timer();
                                                                                                                                                                                                                 if (release == QuickLookGesture::Release::Dismiss)
                                                                                                                                                                                                                     quick_look_->dismiss(); }, [this] { stop_quick_look_timer(); }) {
        setModel(model_);
        setItemDelegate(icons_);
        auto folder_font = font();
        folder_font.setPointSize(12);
        setFont(folder_font);
        setFocusPolicy(Qt::StrongFocus);
        setSelectionMode(QAbstractItemView::ExtendedSelection);
        setSelectionBehavior(QAbstractItemView::SelectRows);
        setEditTriggers(QAbstractItemView::NoEditTriggers);
        setMouseTracking(true);
        setAcceptDrops(true);
        viewport()->setAcceptDrops(true);
        setProperty("dualPaneFolderItemsSource", true);
        setProperty("dualPaneBrowser", model_->browserSide());
        viewport()->setMouseTracking(true);
        setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
        setUniformRowHeights(true);
        setRootIsDecorated(false);
        setItemsExpandable(false);
        setHeaderHidden(true);
        auto *columns = header();
        columns->setStretchLastSection(false);
        columns->setSectionsMovable(false);
        columns->setMinimumSectionSize(16);
        for (int column = 0; column < column_count; ++column)
            columns->setSectionResizeMode(column, QHeaderView::ResizeToContents);
        columns->setSectionResizeMode(0, QHeaderView::Fixed);
        columns->resizeSection(0, column_minimum.at(0));
        columns->setSectionResizeMode(2, QHeaderView::Fixed);
        columns->resizeSection(2, type_column_width);
        columns->setSectionResizeMode(3, QHeaderView::Fixed);
        columns->resizeSection(3, relative_date_column_width);
        columns->setSectionResizeMode(1, QHeaderView::Stretch);
        verticalScrollBar()->installEventFilter(this);
        horizontalScrollBar()->installEventFilter(this);
        connect(model_, &FolderItemsListModel::selectionRevisionChanged, this, [this] { synchronize_selection(); });
        connect(model_, &QAbstractItemModel::modelAboutToBeReset, this, [this] {
            icons_->invalidate();
            if (quick_look_gesture_.active()) {
                quick_look_gesture_.interrupted(quick_look_gesture_.generation());
                stop_quick_look_timer();
                quick_look_->dismiss();
            }
        });
        connect(model_, &QAbstractItemModel::modelReset, this, [this] {
            synchronize_selection();
            restore_scroll();
        });
        connect(verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { report_scroll(); });
    }

    void setColumnsHandler(std::function<void(const std::array<bool, column_count> &)> handler) { columns_handler_ = std::move(handler); }
    /// Shows the Folder Items Context Menu at a global point for `row`, or
    /// for empty space when `row` is -1. `comprehensive` is true only for an
    /// exact Shift invocation.
    void setMenuHandler(std::function<void(const QPoint &, int, bool)> handler) { menu_handler_ = std::move(handler); }
    void setQuickLookFailureHandler(std::function<void()> handler) { quick_look_failure_handler_ = std::move(handler); }
    void setTypeToFilterHandler(std::function<void(const QString &)> handler) { type_to_filter_handler_ = std::move(handler); }
    void setInputMethodHandler(std::function<void(const QString &, const QString &, const QList<QInputMethodEvent::Attribute> &)> handler) { input_method_handler_ = std::move(handler); }

    /// Moves keyboard focus here, which activates this Browser.
    void focusList() {
        if (!hasFocus())
            setFocus(Qt::OtherFocusReason);
    }

  protected:
    void focusInEvent(QFocusEvent *event) override {
        QTreeView::focusInEvent(event);
        update();
    }
    void paintEvent(QPaintEvent *event) override {
        QTreeView::paintEvent(event);
        if (hasFocus()) {
            QPainter painter(viewport());
            painter.setPen(QColor(QString::fromLatin1(active_color)));
            painter.drawRect(viewport()->rect().adjusted(0, 0, -1, -1));
        }
        if (drop_count_ > 0) {
            QPainter painter(viewport());
            painter.setPen(QPen(QColor(QString::fromLatin1(active_color)), 2));
            const auto target = drop_row_ >= 0 ? visualRect(model_->index(drop_row_, 0)).adjusted(1, 1, viewport()->width() - visualRect(model_->index(drop_row_, 0)).right() - 2, -2) : viewport()->rect().adjusted(2, 2, -3, -3);
            painter.drawRect(target);
            const auto label = QStringLiteral("%1 %2 %3").arg(drop_copy_ ? QStringLiteral("Copy") : QStringLiteral("Move")).arg(drop_count_).arg(drop_count_ == 1 ? QStringLiteral("Item") : QStringLiteral("Items"));
            const auto label_rect = QRect(target.left() + 5, target.top() + 4, fontMetrics().horizontalAdvance(label) + 12, fontMetrics().height() + 6);
            painter.fillRect(label_rect, QColor(QString::fromLatin1(active_color)));
            painter.setPen(QColor(QString::fromLatin1(text_color)));
            painter.drawText(label_rect, Qt::AlignCenter, label);
        }
    }
    auto selectionCommand(const QModelIndex &, const QEvent *) const -> QItemSelectionModel::SelectionFlags override { return QItemSelectionModel::NoUpdate; }

    void mousePressEvent(QMouseEvent *event) override {
        event->accept();
        const auto index = indexAt(event->position().toPoint());
        if (event->button() == Qt::LeftButton) {
            focusList();
            // Qt maps ControlModifier to the Command key on macOS.
            const auto modifiers = user_modifiers(event->modifiers());
            drag_candidate_ = index.isValid() && modifiers == Qt::NoModifier;
            drag_press_ = event->position().toPoint();
            drag_tab_ = model_->activeTabId();
            drag_revision_ = model_->visibleRevision();
            if (!index.isValid())
                model_->clearSelection();
            else if (modifiers == Qt::ControlModifier)
                model_->toggleRow(index.row());
            else if (modifiers == Qt::ShiftModifier)
                model_->extendToRow(index.row());
            else if (!model_->rowIsSelected(index.row()))
                model_->selectRow(index.row());
        } else if (event->button() == Qt::RightButton) {
            focusList();
        }
    }
    void mouseReleaseEvent(QMouseEvent *event) override {
        drag_candidate_ = false;
        event->accept();
    }
    void mouseMoveEvent(QMouseEvent *event) override {
        event->accept();
        if (!drag_candidate_ || !event->buttons().testFlag(Qt::LeftButton) || (event->position().toPoint() - drag_press_).manhattanLength() < QApplication::startDragDistance())
            return;
        drag_candidate_ = false;
        const auto drag_id = model_->beginFileDrag(drag_tab_, drag_revision_);
        if (drag_id == 0)
            return;
        auto *mime = new QMimeData();
        mime->setData(QString::fromLatin1(file_drag_mime), file_drag_payload(drag_id));
        QDrag drag(this);
        drag.setMimeData(mime);
        drag.exec(Qt::CopyAction | Qt::MoveAction, Qt::MoveAction);
        model_->cancelFileDrag(drag_id);
    }
    void dragEnterEvent(QDragEnterEvent *event) override { update_drop(event); }
    void dragMoveEvent(QDragMoveEvent *event) override { update_drop(event); }
    void dragLeaveEvent(QDragLeaveEvent *event) override {
        clear_drop();
        event->accept();
    }
    void dropEvent(QDropEvent *event) override {
        const auto drag = internal_file_drag(event);
        const auto index = indexAt(event->position().toPoint());
        const int row = index.isValid() ? index.row() : -1;
        const int kind = row >= 0 ? 1 : 0;
        if (!drop_modifiers_supported(event->modifiers())) {
            clear_drop();
            event->ignore();
            return;
        }
        const bool copy = drop_is_copy(event->modifiers());
        clear_drop();
        const int source_browser = event->source() == nullptr ? -1 : event->source()->property("dualPaneBrowser").toInt();
        if (drag && model_->acceptFileDrop(*drag, source_browser, kind, model_->activeTabId(), row, copy)) {
            event->setDropAction(copy ? Qt::CopyAction : Qt::MoveAction);
            event->accept();
        } else {
            event->ignore();
        }
    }
    void mouseDoubleClickEvent(QMouseEvent *event) override {
        event->accept();
        if (event->button() != Qt::LeftButton || user_modifiers(event->modifiers()) != Qt::NoModifier)
            return;
        if (const auto index = indexAt(event->position().toPoint()); index.isValid()) {
            model_->selectRow(index.row());
            model_->activateRow(index.row());
        }
    }
    void contextMenuEvent(QContextMenuEvent *event) override {
        event->accept();
        focusList();
        const auto index = indexAt(event->pos());
        const auto modifiers = user_modifiers(event->modifiers());
        const bool comprehensive = modifiers == Qt::ShiftModifier;
        QPoint at = event->globalPos();
        int row = index.isValid() ? index.row() : -1;
        if (event->reason() != QContextMenuEvent::Mouse) {
            row = model_->getCursorRow();
            if (row >= 0) {
                const auto cursor = model_->index(row, 0, QModelIndex());
                if (cursor.isValid())
                    at = viewport()->mapToGlobal(visualRect(cursor).center());
                else
                    row = -1;
            }
            if (row < 0)
                at = viewport()->mapToGlobal(QPoint(fontMetrics().height(), fontMetrics().height()));
        }
        if (menu_handler_)
            menu_handler_(at, row, comprehensive);
    }
    void keyPressEvent(QKeyEvent *event) override {
        event->accept();
        const auto modifiers = user_modifiers(event->modifiers());
        const auto key = event->key();
        if (key == Qt::Key_Space) {
            if (modifiers == Qt::NoModifier && !event->isAutoRepeat())
                start_quick_look();
            return;
        }
        if (modifiers == Qt::ControlModifier && key == Qt::Key_A) {
            model_->selectAll();
            return;
        }
        if (const auto movement = movement_for(key); movement && (modifiers == Qt::NoModifier || modifiers == Qt::ShiftModifier)) {
            const int page = qMax(1, viewport()->height() / qMax(1, sizeHintForRow(0)));
            model_->moveCursor(static_cast<int>(*movement), page, modifiers == Qt::ShiftModifier);
            scroll_to_cursor();
            return;
        }
        if (modifiers != Qt::NoModifier)
            return;
        if (key == Qt::Key_Left)
            model_->goToParent();
        else if (key == Qt::Key_Right)
            model_->activateSelected();
        else if (!event->text().isEmpty() && std::all_of(event->text().cbegin(), event->text().cend(), [](QChar character) { return character.isPrint(); }) && type_to_filter_handler_)
            type_to_filter_handler_(event->text());
    }
    void inputMethodEvent(QInputMethodEvent *event) override {
        event->accept();
        if (input_method_handler_)
            input_method_handler_(event->preeditString(), event->commitString(), event->attributes());
    }
    void keyReleaseEvent(QKeyEvent *event) override {
        if (event->key() != Qt::Key_Space) {
            QTreeView::keyReleaseEvent(event);
            return;
        }
        event->accept();
        if (quick_look_gesture_.active() && (model_->activeTabId() != quick_look_tab_ || model_->visibleRevision() != quick_look_revision_)) {
            quick_look_gesture_.interrupted(quick_look_gesture_.generation());
            stop_quick_look_timer();
            quick_look_->dismiss();
            return;
        }
        const auto release = quick_look_gesture_.released(quick_look_gesture_.generation(), event->isAutoRepeat());
        if (release != QuickLookGesture::Release::Ignored)
            stop_quick_look_timer();
        if (release == QuickLookGesture::Release::Dismiss)
            quick_look_->dismiss();
    }
    void focusOutEvent(QFocusEvent *event) override {
        if (quick_look_timer_ != nullptr && quick_look_gesture_.held()) {
            quick_look_gesture_.interrupted(quick_look_gesture_.generation());
            stop_quick_look_timer();
        }
        QTreeView::focusOutEvent(event);
        update();
    }
    void resizeEvent(QResizeEvent *event) override {
        QTreeView::resizeEvent(event);
        update_columns();
    }

  private:
    template <typename Event>
    void update_drop(Event *event) {
        const auto drag = internal_file_drag(event);
        const auto index = indexAt(event->position().toPoint());
        const int row = index.isValid() ? index.row() : -1;
        const int kind = row >= 0 ? 1 : 0;
        if (!drop_modifiers_supported(event->modifiers())) {
            clear_drop();
            event->ignore();
            return;
        }
        const bool copy = drop_is_copy(event->modifiers());
        const int source_browser = event->source() == nullptr ? -1 : event->source()->property("dualPaneBrowser").toInt();
        const int count = drag ? model_->previewFileDrop(*drag, source_browser, kind, model_->activeTabId(), row, copy) : -1;
        if (count <= 0) {
            clear_drop();
            event->ignore();
            return;
        }
        drop_row_ = row;
        drop_count_ = count;
        drop_copy_ = copy;
        const auto destination = row >= 0 ? model_->index(row, 1).data(Qt::DisplayRole).toString() : model_->getPathText();
        const auto description = QStringLiteral("%1 %2 %3 into %4").arg(copy ? QStringLiteral("Copy") : QStringLiteral("Move")).arg(count).arg(count == 1 ? QStringLiteral("Item") : QStringLiteral("Items")).arg(destination);
        const bool changed = accessibleDescription() != description;
        setAccessibleDescription(description);
        if (changed) {
            QAccessibleAnnouncementEvent announcement(this, description);
            QAccessible::updateAccessibility(&announcement);
        }
        viewport()->update();
        event->setDropAction(copy ? Qt::CopyAction : Qt::MoveAction);
        event->accept();
    }
    void clear_drop() {
        if (drop_count_ <= 0)
            return;
        drop_row_ = -1;
        drop_count_ = 0;
        drop_copy_ = false;
        setAccessibleDescription(QString());
        viewport()->update();
    }
    void start_quick_look() {
        const int cursor = model_->getCursorRow();
        if (cursor < 0)
            return;
        const auto index = model_->index(cursor, 0);
        const QuickLookItem item{index.data(native_path_role).toByteArray(), index.data(native_directory_role).toBool()};
        if (quick_look_gesture_.press(item, true, false) && quick_look_gesture_.held()) {
            const auto generation = quick_look_gesture_.generation();
            const auto observed_tab = model_->activeTabId();
            const auto observed_revision = model_->visibleRevision();
            quick_look_tab_ = observed_tab;
            quick_look_revision_ = observed_revision;
            auto *timer = new QTimer(this);
            timer->setSingleShot(true);
            timer->setTimerType(Qt::PreciseTimer);
            connect(timer, &QTimer::timeout, this, [this, timer, generation, observed_tab, observed_revision] {
                if (quick_look_timer_ == timer)
                    quick_look_timer_ = nullptr;
                timer->deleteLater();
                if (model_->activeTabId() != observed_tab || model_->visibleRevision() != observed_revision) {
                    quick_look_gesture_.interrupted(generation);
                    return;
                }
                (void)quick_look_gesture_.hold_elapsed(generation);
            });
            quick_look_timer_ = timer;
            timer->start(quick_look_hold_milliseconds);
        }
    }
    void stop_quick_look_timer() {
        if (quick_look_timer_ != nullptr) {
            quick_look_timer_->stop();
            quick_look_timer_->deleteLater();
            quick_look_timer_ = nullptr;
        }
    }
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if ((watched == verticalScrollBar() || watched == horizontalScrollBar()) && event->type() == QEvent::MouseButtonPress) {
            const auto *mouse_event = dynamic_cast<QMouseEvent *>(event);
            if (mouse_event != nullptr && mouse_event->button() == Qt::LeftButton)
                focusList();
        }
        return QTreeView::eventFilter(watched, event);
    }
    /// Keeps the cursor row a keyboard movement chose visible.
    void scroll_to_cursor() {
        if (const int cursor = model_->getCursorRow(); cursor >= 0)
            scrollTo(model_->index(cursor, 0));
    }
    /// Shows the application's selection and cursor without emitting a command:
    /// the view never reacts to its own selection model.
    void synchronize_selection() {
        QItemSelection selection;
        const auto ranges = model_->selectedRanges();
        for (qsizetype pair = 0; pair + 1 < ranges.size(); pair += 2)
            selection.select(model_->index(ranges.at(pair), 0), model_->index(ranges.at(pair + 1), column_count - 1));
        selectionModel()->select(selection, QItemSelectionModel::ClearAndSelect);
        const int cursor = model_->getCursorRow();
        selectionModel()->setCurrentIndex(cursor >= 0 ? model_->index(cursor, 0) : QModelIndex(), QItemSelectionModel::NoUpdate);
    }
    /// After a tab switch, history step, or new folder, scrolls to the
    /// remembered anchor row and offset.
    void restore_scroll() {
        restoring_ = true;
        const int row = model_->scrollHintRow();
        if (row >= 0) {
            scrollTo(model_->index(row, 0), QAbstractItemView::PositionAtTop);
            verticalScrollBar()->setValue(verticalScrollBar()->value() + model_->scrollHintOffset());
        } else {
            scrollToTop();
        }
        restoring_ = false;
    }
    void report_scroll() {
        if (restoring_)
            return;
        const auto top = indexAt(QPoint(0, 0));
        model_->updateScroll(top.isValid() ? top.row() : -1, top.isValid() ? -visualRect(top).top() : 0);
    }
    /// Hides columns, in their specified order, until the visible columns'
    /// minimum widths fit; Name always stays.
    void update_columns() {
        std::array<bool, column_count> visible{};
        visible.fill(true);
        int needed = 0;
        for (const int width : column_minimum)
            needed += width;
        const int available = viewport()->width();
        for (const int column : hide_order) {
            if (needed <= available)
                break;
            visible.at(column) = false;
            needed -= column_minimum.at(column);
        }
        for (int column = 0; column < column_count; ++column)
            setColumnHidden(column, !visible.at(column));
        if (columns_handler_)
            columns_handler_(visible);
    }

    FolderItemsListModel *model_;
    ItemIconDelegate *icons_;
    std::function<void(const std::array<bool, column_count> &)> columns_handler_;
    std::function<void(const QPoint &, int, bool)> menu_handler_;
    std::function<void()> quick_look_failure_handler_;
    std::function<void(const QString &)> type_to_filter_handler_;
    std::function<void(const QString &, const QString &, const QList<QInputMethodEvent::Attribute> &)> input_method_handler_;
    std::unique_ptr<QuickLookPreviewController> quick_look_;
    QuickLookGesture quick_look_gesture_;
    QPointer<QTimer> quick_look_timer_;
    std::uint64_t quick_look_tab_ = 0;
    std::uint64_t quick_look_revision_ = 0;
    bool restoring_ = false;
    bool drag_candidate_ = false;
    QPoint drag_press_;
    std::uint64_t drag_tab_ = 0;
    std::uint64_t drag_revision_ = 0;
    int drop_row_ = -1;
    int drop_count_ = 0;
    bool drop_copy_ = false;
};

class ElidedStatusLabel final : public QLabel {
  public:
    explicit ElidedStatusLabel(QWidget *parent) : QLabel(parent) {
        setObjectName(QStringLiteral("browserStatusBar"));
        setAccessibleName(QStringLiteral("Browser Status Bar"));
        setWordWrap(false);
        setContentsMargins(4, 2, 4, 2);
        setFixedHeight(fontMetrics().height() + 4);
    }

    void set_full_text(const QString &text) {
        full_text_ = text;
        setAccessibleDescription(text);
        update_elision();
    }

  protected:
    void resizeEvent(QResizeEvent *event) override {
        QLabel::resizeEvent(event);
        update_elision();
    }

    void changeEvent(QEvent *event) override {
        QLabel::changeEvent(event);
        if (event->type() == QEvent::FontChange) {
            setFixedHeight(fontMetrics().height() + 4);
            update_elision();
        }
    }

  private:
    void update_elision() { QLabel::setText(fontMetrics().elidedText(full_text_, Qt::ElideRight, qMax(0, contentsRect().width()))); }
    QString full_text_;
};

class FolderFilterTextEdit final : public QLineEdit {
  public:
    explicit FolderFilterTextEdit(QWidget *parent) : QLineEdit(parent) {}
    void setEscapeHandler(std::function<void()> handler) { escape_handler_ = std::move(handler); }
    void setFocusLossHandler(std::function<void()> handler) { focus_loss_handler_ = std::move(handler); }

  protected:
    void keyPressEvent(QKeyEvent *event) override {
        if (event->key() == Qt::Key_Escape && event->modifiers() == Qt::NoModifier && escape_handler_) {
            event->accept();
            escape_handler_();
            return;
        }
        QLineEdit::keyPressEvent(event);
    }
    void focusOutEvent(QFocusEvent *event) override {
        QLineEdit::focusOutEvent(event);
        if (focus_loss_handler_)
            QTimer::singleShot(0, this, [this] { if (!hasFocus()) focus_loss_handler_(); });
    }

  private:
    std::function<void()> escape_handler_;
    std::function<void()> focus_loss_handler_;
};

/// The Browser's Path Edit Control. It shows the confirmed path while it has
/// no focus, and focus always means editing, so a control that is not editing
/// never holds focus. Return submits, Escape discards, and a pending or failed
/// submission keeps its text until it ends. The application owns every result.
class PathEditControl final : public QLineEdit {
  public:
    PathEditControl(FolderItemsListModel *model, std::function<void()> focus_list, QWidget *parent) : QLineEdit(parent), model_(model), focus_list_(std::move(focus_list)), accepted_seen_(model->getPathAcceptedCount()) {
        setObjectName(QStringLiteral("pathEditControl"));
        setAccessibleName(QStringLiteral("Path Edit Control"));
        setFocusPolicy(Qt::StrongFocus);
        QObject::connect(this, &QLineEdit::textEdited, this, [this] {
            edited_ = true;
            // Editing the failed text clears its error.
            if (model_->getPathEntryState() == path_failed)
                model_->discardPathEntry();
        });
        QObject::connect(model, &FolderItemsListModel::pathTextChanged, this, [this] {
            if (!editing_)
                setText(model_->getPathText());
        });
        QObject::connect(model, &FolderItemsListModel::pathEntryRevisionChanged, this, [this] { sync(); });
        QObject::connect(model, &FolderItemsListModel::tabsRevisionChanged, this, [this] {
            // A tab switch discards the draft; a running check finishes in its own tab.
            if (editing_ && model_->activeTabId() != observed_tab_)
                end_editing();
        });
    }

    /// `EditPath`: moves focus here, which starts editing and selects all.
    void edit() { setFocus(Qt::ShortcutFocusReason); }
    [[nodiscard]] auto editing() const -> bool { return editing_; }

  protected:
    void focusInEvent(QFocusEvent *event) override {
        QLineEdit::focusInEvent(event);
        if (event->reason() == Qt::PopupFocusReason)
            return;
        lost_focus_during_pending_ = false;
        if (editing_)
            return;
        model_->activateBrowser();
        model_->beginPathEntry();
        editing_ = true;
        edited_ = false;
        observed_tab_ = model_->activeTabId();
        accepted_seen_ = model_->getPathAcceptedCount();
        QTimer::singleShot(0, this, [this] {
            if (editing_ && !edited_)
                selectAll();
        });
    }
    void focusOutEvent(QFocusEvent *event) override {
        QLineEdit::focusOutEvent(event);
        if (event->reason() == Qt::PopupFocusReason)
            return;
        // Mark this synchronously so a resolver result delivered before the
        // queued focus check cannot expose or announce an abandoned failure.
        if (model_->getPathEntryState() == path_pending)
            lost_focus_during_pending_ = true;
        QTimer::singleShot(0, this, [this] {
            if (hasFocus()) {
                lost_focus_during_pending_ = false;
                return;
            }
            if (!editing_)
                return;
            // A running check finishes on its own; anything else ends here.
            if (model_->getPathEntryState() != path_pending)
                end_editing();
        });
    }
    void keyPressEvent(QKeyEvent *event) override {
        if (event->modifiers() == Qt::NoModifier || event->modifiers() == Qt::KeypadModifier) {
            if (event->key() == Qt::Key_Escape) {
                event->accept();
                escape();
                return;
            }
            if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
                event->accept();
                submit();
                return;
            }
        }
        QLineEdit::keyPressEvent(event);
    }

  private:
    static constexpr int path_pending = 1;
    static constexpr int path_failed = 2;

    void submit() {
        if (model_->getPathEntryState() == path_pending)
            return;
        // Empty text acts like Escape and is never submitted.
        if (text().isEmpty()) {
            escape();
            return;
        }
        model_->submitPath(observed_tab_, text(), edited_);
    }
    void escape() {
        if (model_->getPathEntryState() == path_pending)
            model_->cancelPathEntry(observed_tab_);
        end_editing();
        focus_list_();
    }
    /// Restores display mode with the confirmed path.
    void end_editing() {
        editing_ = false;
        edited_ = false;
        lost_focus_during_pending_ = false;
        if (model_->getPathEntryState() != 0)
            model_->discardPathEntry();
        apply_state(0, QString());
        setText(model_->getPathText());
        deselect();
        if (hasFocus())
            focus_list_();
    }
    void apply_state(int state, const QString &error) {
        const bool pending = state == path_pending;
        const bool failed = state == path_failed;
        setReadOnly(pending);
        setProperty("pathPending", pending);
        setProperty("pathError", failed);
        repolish(this);
        QString description;
        if (pending)
            description = QStringLiteral("Checking path");
        else if (failed)
            description = error;
        else if (editing_)
            description = QStringLiteral("Editing");
        setAccessibleDescription(description);
    }
    /// Applies the application's submission state to the editor.
    void sync() {
        const int state = model_->getPathEntryState();
        const auto error = model_->getPathError();
        if ((!editing_ || lost_focus_during_pending_) && state == path_failed && !hasFocus()) {
            // The result arrived after focus left: nothing to keep.
            model_->discardPathEntry();
            return;
        }
        if (state == 0) {
            const bool accepted = model_->getPathAcceptedCount() != accepted_seen_;
            accepted_seen_ = model_->getPathAcceptedCount();
            if (editing_ || property("pathPending").toBool() || property("pathError").toBool()) {
                const bool had_focus = hasFocus();
                end_editing_quiet();
                if (accepted && had_focus)
                    focus_list_();
            }
            return;
        }
        apply_state(state, error);
        if (state == path_failed) {
            if (!editing_ && hasFocus())
                editing_ = true;
            QAccessibleAnnouncementEvent announcement(this, error);
            QAccessible::updateAccessibility(&announcement);
        }
    }
    void end_editing_quiet() {
        editing_ = false;
        edited_ = false;
        lost_focus_during_pending_ = false;
        apply_state(0, QString());
        setText(model_->getPathText());
        deselect();
    }

    FolderItemsListModel *model_;
    std::function<void()> focus_list_;
    bool editing_ = false;
    bool edited_ = false;
    bool lost_focus_during_pending_ = false;
    quint64 observed_tab_ = 0;
    int accepted_seen_ = 0;
};

/// Native state that belongs to the one temporary filter row in a Browser.
/// It preserves the tab identity shown to the person until their event has
/// crossed the CXX-Qt boundary.
class FilterRowState final {
  public:
    /// Records a coherently rendered filter view and resets an empty row when
    /// it belongs to a different tab. Returns whether the rendered tab moved.
    [[nodiscard]] bool render(std::uint64_t tab) {
        const bool changed = !has_rendered_tab_ || tab != rendered_tab_;
        if (changed)
            show_empty_row_ = false;
        rendered_tab_ = tab;
        has_rendered_tab_ = true;
        return changed;
    }
    void open_empty_row() { show_empty_row_ = true; }
    void record_user_edit(const QString &text) { show_empty_row_ = text.isEmpty(); }
    void dismiss() { show_empty_row_ = false; }
    [[nodiscard]] auto rendered_tab() const -> std::uint64_t { return rendered_tab_; }
    [[nodiscard]] auto keeps_empty_row() const -> bool { return show_empty_row_; }

  private:
    std::uint64_t rendered_tab_ = 0;
    bool has_rendered_tab_ = false;
    bool show_empty_row_ = false;
};

auto filter_status_text(int status, bool no_matches, const QString &text, int count) -> QString;

class FolderPane final : public QFrame {
  public:
    FolderPane(FolderItemsListModel *model, IconLoader *icons, QWidget *parent) : QFrame(parent), view_(new FolderItemsList(model, icons)) {
        setObjectName(QStringLiteral("folderPane"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        auto *title = new QLabel(this);
        title->setObjectName(QStringLiteral("folderPaneToolbarRow1"));
        title->setAccessibleName(QStringLiteral("Folder Pane Toolbar Row #1"));
        auto *summary = new QWidget(this);
        summary->setObjectName(QStringLiteral("folderPaneToolbarRow2"));
        summary->setAccessibleName(QStringLiteral("Folder Pane Toolbar Row #2"));
        summary->setMinimumHeight(20);
        auto *summary_layout = new QHBoxLayout(summary);
        summary_layout->setContentsMargins(6, 0, 6, 0);
        summary_layout->setSpacing(8);
        auto *summary_count = new QLabel(summary);
        auto *summary_selected = new QLabel(summary);
        summary_selected->setObjectName(QStringLiteral("summarySelected"));
        summary_selected->setStyleSheet(QStringLiteral("color: #5B7FA8;"));
        auto *summary_size = new QLabel(summary);
        summary_size->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        summary_layout->addWidget(summary_count);
        summary_layout->addWidget(summary_selected);
        summary_layout->addStretch();
        summary_layout->addWidget(summary_size);
        auto *commands = new QWidget(this);
        commands->setObjectName(QStringLiteral("folderPaneToolbarRow3"));
        commands->setAccessibleName(QStringLiteral("Folder Pane Toolbar Row #3"));
        auto *command_layout = new QHBoxLayout(commands);
        command_layout->setContentsMargins(0, 0, 0, 0);
        command_layout->setSpacing(0);
        auto *back = command_button(QStringLiteral("backButton"), QStringLiteral("←"), QStringLiteral("Back"), QStringLiteral("Back Button"), commands);
        auto *forward = command_button(QStringLiteral("forwardButton"), QStringLiteral("→"), QStringLiteral("Forward"), QStringLiteral("Forward Button"), commands);
        auto *up = command_button(QStringLiteral("upButton"), QStringLiteral("Up"), QStringLiteral("Up"), QStringLiteral("Up Button"), commands);
        back->setIcon(navigation_icon(NavigationIcon::Back, true));
        forward->setIcon(navigation_icon(NavigationIcon::Forward, true));
        up->setIcon(navigation_icon(NavigationIcon::Up, true));
        for (auto *button : {back, forward, up}) {
            button->setText(QString());
            button->setIconSize(QSize(16, 16));
            button->setToolButtonStyle(Qt::ToolButtonIconOnly);
        }
        command_layout->addWidget(back);
        command_layout->addWidget(forward);
        command_layout->addWidget(up);
        const auto sort_label = [](const int field, const bool descending) {
            static const std::array<std::array<const char *, 2>, 4> labels = {{{"name (A-Z)", "name (Z-A)"}, {"type (A-Z)", "type (Z-A)"}, {"date (oldest first)", "date (newest first)"}, {"size (smallest first)", "size (largest first)"}}};
            return QStringLiteral("Sort items by %1").arg(QString::fromLatin1(labels.at(field).at(descending ? 1 : 0)));
        };
        std::array<QWidget *, 4> sort_groups{};
        for (int field = 0; field < 4; ++field) {
            auto *sort_group = new QWidget(commands);
            auto *sort_group_layout = new QHBoxLayout(sort_group);
            sort_group_layout->setContentsMargins(field == 0 ? 0 : 4, 0, 0, 0);
            sort_group_layout->setSpacing(0);
            sort_group_layout->setAlignment(Qt::AlignLeft);
            sort_groups.at(field) = sort_group;
            if (field == 1)
                sort_group->setFixedWidth(type_column_width);
            else if (field == 2)
                sort_group->setFixedWidth(relative_date_column_width + column_minimum.at(4));
            else if (field == 3)
                sort_group->setFixedWidth(column_minimum.at(5));
            for (const bool descending : {false, true}) {
                auto *sort = command_button(QStringLiteral("sortControl"), descending ? QStringLiteral("v") : QStringLiteral("^"), sort_label(field, descending), sort_label(field, descending), sort_group);
                sort->setCheckable(true);
                const int choice = (field * 2) + (descending ? 1 : 0);
                sorts_.at(choice) = sort;
                sort->setText(QString());
                sort->setProperty("sortDescending", descending);
                sort->setIcon(sort_caret_icon(descending, true));
                sort->setIconSize(QSize(16, 16));
                sort->setToolButtonStyle(Qt::ToolButtonIconOnly);
                sort_group_layout->addWidget(sort);
                QObject::connect(sort, &QToolButton::clicked, this, [this, model, choice] {
                    view_->focusList();
                    model->setSort(choice);
                    update_sort(model);
                });
            }
        }
        command_layout->addWidget(sort_groups.at(0));
        command_layout->addStretch();
        command_layout->addWidget(sort_groups.at(1));
        command_layout->addWidget(sort_groups.at(2));
        command_layout->addWidget(sort_groups.at(3));
        filter_row_ = new QWidget(this);
        filter_row_->setObjectName(QStringLiteral("folderItemsFilterRow"));
        filter_row_->setAccessibleName(QStringLiteral("Folder Items Filter Row"));
        auto *filter_layout = new QHBoxLayout(filter_row_);
        filter_layout->setContentsMargins(6, 2, 4, 2);
        filter_layout->setSpacing(6);
        auto *filter_prefix = new QLabel(QStringLiteral("Starts with:"), filter_row_);
        filter_prefix->setObjectName(QStringLiteral("filterPrefix"));
        filter_input_ = new FolderFilterTextEdit(filter_row_);
        filter_input_->setObjectName(QStringLiteral("folderItemsFilterText"));
        filter_input_->setAccessibleName(QStringLiteral("Folder Items Filter Text"));
        filter_input_->setPlaceholderText(QStringLiteral("Type a name prefix"));
        filter_input_->setClearButtonEnabled(false);
        filter_input_->setMinimumWidth(40);
        filter_input_->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
        filter_status_ = new QLabel(filter_row_);
        filter_status_->setObjectName(QStringLiteral("filterResultStatus"));
        filter_status_->setAccessibleName(QStringLiteral("Filter Results"));
        auto *close_filter = new QToolButton(filter_row_);
        close_filter->setObjectName(QStringLiteral("closeFilterButton"));
        close_filter->setText(QStringLiteral("×"));
        close_filter->setToolTip(QStringLiteral("Close Filter"));
        close_filter->setAccessibleName(QStringLiteral("Close Filter Button"));
        close_filter->setFixedSize(24, 22);
        filter_layout->addWidget(filter_prefix);
        filter_layout->addWidget(filter_input_, 1);
        filter_layout->addWidget(filter_status_);
        filter_layout->addWidget(close_filter);
        filter_row_->hide();
        view_->setObjectName(QStringLiteral("folderItemsList"));
        view_->setAccessibleName(QStringLiteral("Folder Items List"));
        auto *status = new ElidedStatusLabel(this);
        status_ = status;
        // The New Folder Name Editor is a temporary row pinned at the top of
        // the listing.
        new_folder_row_ = new QWidget(this);
        new_folder_row_->setObjectName(QStringLiteral("newFolderRow"));
        auto *new_folder_layout = new QVBoxLayout(new_folder_row_);
        new_folder_layout->setContentsMargins(icon_column_width, 1, 4, 1);
        new_folder_layout->setSpacing(1);
        new_folder_row_->hide();
        layout->addWidget(title);
        layout->addWidget(summary);
        layout->addWidget(commands);
        layout->addWidget(filter_row_);
        layout->addWidget(new_folder_row_);
        layout->addWidget(view_, 1);
        QObject::connect(model, &FolderItemsListModel::folderNameChanged, this, [title, model] { title->setText(model->getFolderName()); });
        auto *quick_look_failure_timer = new QTimer(this);
        quick_look_failure_timer->setSingleShot(true);
        const auto update_status = [status, model, quick_look_failure_timer] {
            if (quick_look_failure_timer->isActive())
                return;
            const auto &status_text = model->getStatusText();
            const auto &count = model->getSummaryCountText();
            const auto &selected = model->getSummarySelectedText();
            // A location path was the old steady-state status. Mirror the
            // concise summary there, but leave loading and error feedback.
            if (status_text != model->getPathText()) {
                status->set_full_text(status_text);
            } else if (count.isEmpty()) {
                status->set_full_text(QString());
            } else {
                status->set_full_text(QStringLiteral("%1 %2").arg(count, selected));
            }
        };
        QObject::connect(model, &FolderItemsListModel::summaryCountTextChanged, this, [summary_count, update_status, model] {
            summary_count->setText(model->getSummaryCountText());
            update_status();
        });
        QObject::connect(model, &FolderItemsListModel::summarySelectedTextChanged, this, [summary_selected, update_status, model] {
            summary_selected->setText(model->getSummarySelectedText());
            update_status();
        });
        QObject::connect(model, &FolderItemsListModel::summarySizeTextChanged, this, [summary_size, model] { summary_size->setText(model->getSummarySizeText()); });
        QObject::connect(model, &FolderItemsListModel::statusTextChanged, this, update_status);
        QObject::connect(quick_look_failure_timer, &QTimer::timeout, this, update_status);
        view_->setQuickLookFailureHandler([status, quick_look_failure_timer] {
            status->set_full_text(QStringLiteral("Quick Look couldn’t open this item."));
            quick_look_failure_timer->start(status_reason_milliseconds);
        });
        update_status();
        const auto update_toolbar = [back, forward, up, model] {
            const int state = model->getToolbarState();
            back->setEnabled(has_flag(state, toolbar_back));
            forward->setEnabled(has_flag(state, toolbar_forward));
            up->setEnabled(has_flag(state, toolbar_up));
        };
        update_toolbar();
        QObject::connect(model, &FolderItemsListModel::toolbarStateChanged, this, update_toolbar);
        QObject::connect(model, &FolderItemsListModel::sortChoiceChanged, this, [this, model] { update_sort(model); });
        const auto refresh_filter = [this, model] {
            const auto tab = model->activeTabId();
            const bool tab_changed = filter_row_state_.render(tab);
            const auto &text = model->getFilterText();
            {
                const QSignalBlocker blocker(filter_input_);
                filter_input_->setText(text);
            }
            if (!text.isEmpty())
                filter_row_->show();
            else if (tab_changed || !filter_row_state_.keeps_empty_row())
                filter_row_->hide();
            filter_status_->setText(filter_status_text(model->getFilterStatus(), model->getNoFilterMatches(), text, model->getFilterResultCount()));
            filter_input_->setAccessibleDescription(QStringLiteral("Starts with %1. %2").arg(text, filter_status_->text()));
            filter_status_->setAccessibleDescription(filter_status_->text());
            const auto announcement_text = QStringLiteral("Starts with %1. %2").arg(text, filter_status_->text());
            if (filter_row_->isVisible() && announcement_text != last_filter_announcement_) {
                last_filter_announcement_ = announcement_text;
                QAccessibleAnnouncementEvent announcement(filter_row_, announcement_text);
                QAccessible::updateAccessibility(&announcement);
            }
        };
        QObject::connect(model, &FolderItemsListModel::filterViewChanged, this, refresh_filter);
        QObject::connect(filter_input_, &QLineEdit::textChanged, this, [this, model](const QString &text) {
            // This is a user edit: preserve the tab whose text field produced
            // it. A queued edit from the previous tab must be rejected rather
            // than applied to whichever tab the model now shows.
            filter_row_state_.record_user_edit(text);
            model->editFilterText(filter_row_state_.rendered_tab(), text);
        });
        QObject::connect(close_filter, &QToolButton::clicked, this, [this, model] {
            const QString empty;
            filter_row_state_.dismiss();
            model->editFilterText(filter_row_state_.rendered_tab(), empty);
            filter_row_->hide();
            view_->focusList();
        });
        filter_input_->setFocusLossHandler([this] {
            if (filter_input_->text().isEmpty() && !filter_row_->isAncestorOf(QApplication::focusWidget())) {
                filter_row_state_.dismiss();
                filter_row_->hide();
            }
        });
        filter_input_->setEscapeHandler([this, model] {
            const QString empty;
            filter_row_state_.dismiss();
            model->editFilterText(filter_row_state_.rendered_tab(), empty);
            filter_row_->hide();
            view_->focusList();
        });
        view_->setTypeToFilterHandler([this, model](const QString &text) {
            const auto observed_tab = model->activeTabId();
            filter_row_state_.record_user_edit(text);
            model->editFilterText(observed_tab, model->getFilterText() + text);
            filter_row_->show();
            filter_input_->setFocus(Qt::OtherFocusReason);
        });
        view_->setInputMethodHandler([this](const QString &preedit, const QString &commit, const QList<QInputMethodEvent::Attribute> &attributes) {
            filter_row_->show();
            filter_input_->setFocus(Qt::OtherFocusReason);
            QInputMethodEvent forwarded(preedit, attributes);
            if (!commit.isEmpty())
                forwarded.setCommitString(commit);
            QApplication::sendEvent(filter_input_, &forwarded);
        });
        refresh_filter();
        update_sort(model);
        QObject::connect(back, &QToolButton::clicked, model, [this, model] {
            view_->focusList();
            model->goBack();
        });
        QObject::connect(forward, &QToolButton::clicked, model, [this, model] {
            view_->focusList();
            model->goForward();
        });
        QObject::connect(up, &QToolButton::clicked, model, [this, model] {
            view_->focusList();
            model->goToParent();
        });
        // A field's sort pair is shown only while its field is; both date
        // columns share the Date pair.
        view_->setColumnsHandler([this](const std::array<bool, column_count> &visible) {
            const std::array<bool, 4> fields_visible = {true, visible.at(2), visible.at(3) || visible.at(4), visible.at(5)};
            // Once Exact Date hides, the Date pair only occupies the
            // remaining Relative Date column's width.
            sort_groups_.at(2)->setFixedWidth(relative_date_column_width + (visible.at(4) ? column_minimum.at(4) : 0));
            for (int field = 0; field < static_cast<int>(sort_groups_.size()); ++field)
                sort_groups_.at(field)->setVisible(fields_visible.at(field));
            for (int choice = 0; choice < static_cast<int>(sorts_.size()); ++choice)
                sorts_.at(choice)->setVisible(fields_visible.at(choice / 2));
        });
        sort_groups_ = sort_groups;
    }

    [[nodiscard]] auto view() const -> FolderItemsList * { return view_; }
    [[nodiscard]] auto status() const -> ElidedStatusLabel * { return status_; }
    [[nodiscard]] auto new_folder_row() const -> QWidget * { return new_folder_row_; }
    void focus_filter() {
        filter_row_state_.open_empty_row();
        filter_row_->show();
        filter_input_->setFocus(Qt::ShortcutFocusReason);
    }

  private:
    void update_sort(FolderItemsListModel *model) {
        const int choice = model->getSortChoice();
        for (int index = 0; index < static_cast<int>(sorts_.size()); ++index) {
            sorts_.at(index)->setEnabled(choice >= 0);
            sorts_.at(index)->setChecked(index == choice);
        }
    }

    FolderItemsList *view_;
    QWidget *filter_row_ = nullptr;
    FolderFilterTextEdit *filter_input_ = nullptr;
    QLabel *filter_status_ = nullptr;
    FilterRowState filter_row_state_;
    QString last_filter_announcement_;
    ElidedStatusLabel *status_ = nullptr;
    QWidget *new_folder_row_ = nullptr;
    std::array<QWidget *, 4> sort_groups_{};
    std::array<QToolButton *, 8> sorts_{};
};

/// The Rename Item Editor or the New Folder Name Editor. Return or focus loss
/// commits and Escape abandons; while a commit waits for its job the text
/// stays and cannot change.
class FileNameEditor final : public QLineEdit {
  public:
    FileNameEditor(const QString &object_name, const QString &accessible_name, const QString &text, std::function<void()> commit, std::function<void()> cancel, QWidget *parent) : QLineEdit(text, parent), commit_(std::move(commit)), cancel_(std::move(cancel)) {
        setObjectName(object_name);
        setAccessibleName(accessible_name);
        selectAll();
    }
    void abandon() { abandoned_ = true; }

  protected:
    void keyPressEvent(QKeyEvent *event) override {
        if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
            event->accept();
            if (!isReadOnly())
                commit_();
            return;
        }
        if (event->key() == Qt::Key_Escape) {
            event->accept();
            cancel_();
            return;
        }
        QLineEdit::keyPressEvent(event);
    }
    void focusOutEvent(QFocusEvent *event) override {
        QLineEdit::focusOutEvent(event);
        if (!abandoned_ && !isReadOnly() && event->reason() != Qt::ActiveWindowFocusReason && event->reason() != Qt::PopupFocusReason)
            commit_();
    }

  private:
    std::function<void()> commit_;
    std::function<void()> cancel_;
    bool abandoned_ = false;
};

/// Drives one Browser's inline name editor through its job: a refused name
/// keeps or reopens the editor with the typed text and the error, success
/// closes it, and any other failure closes it so the job's panel shows it.
class NameEditController final : public QObject {
  public:
    NameEditController(FolderItemsListModel *model, FolderItemsList *view, QWidget *new_folder_row) : model_(model), view_(view), new_folder_row_(new_folder_row) {
        QObject::connect(model_, &FolderItemsListModel::editorRevisionChanged, this, [this] { open(); });
    }

    /// Follows the pending job after the session changed.
    void update() {
        if (pending_ < 0 || editor_ == nullptr)
            return;
        switch (model_->editorOutcome(pending_)) {
        case 0:
            return;
        case 1:
            rejected_ = pending_;
            pending_ = -1;
            editor_->setReadOnly(false);
            show_error(model_->editorOutcomeText(rejected_));
            editor_->setFocus(Qt::OtherFocusReason);
            return;
        default:
            pending_ = -1;
            close();
        }
    }

  private:
    void open() {
        const int command = model_->editorCommand();
        if (command != command_rename && command != command_new_folder)
            return;
        if (pending_ >= 0)
            return;
        close();
        command_ = command;
        const bool rename = command == command_rename;
        QWidget *host = rename ? view_->viewport() : new_folder_row_;
        editor_ = new FileNameEditor(rename ? QStringLiteral("renameItemEditor") : QStringLiteral("newFolderNameEditor"), rename ? QStringLiteral("Rename Item Editor") : QStringLiteral("New Folder Name Editor"), model_->editorText(), [this] { commit(); }, [this] { cancel(); }, host);
        error_ = new QLabel(host);
        error_->setObjectName(QStringLiteral("nameEditorError"));
        error_->setWordWrap(true);
        error_->hide();
        if (rename) {
            const int row = model_->editorRow();
            const auto rect = view_->visualRect(model_->index(row, 1, QModelIndex()));
            if (!rect.isValid()) {
                close();
                return;
            }
            editor_->setGeometry(rect.left(), rect.top(), qMax(rect.width(), 160), rect.height() + 6);
            error_->setGeometry(rect.left(), rect.bottom() + 7, view_->viewport()->width() - rect.left(), 20);
            // Only the name before its extension starts selected.
            const auto text = editor_->text();
            const auto dot = text.lastIndexOf(QLatin1Char('.'));
            editor_->setSelection(0, static_cast<int>(dot > 0 ? dot : text.size()));
        } else {
            new_folder_row_->layout()->addWidget(editor_);
            new_folder_row_->layout()->addWidget(error_);
            new_folder_row_->show();
        }
        editor_->show();
        editor_->setFocus(Qt::OtherFocusReason);
    }
    void commit() {
        if (editor_ == nullptr || pending_ >= 0)
            return;
        const auto job = model_->commitName(command_, editor_->text(), rejected_);
        if (job == -1) {
            close();
        } else if (job == -2) {
            // The refused job stays, so a later retry keeps its target.
            show_error(model_->nameError());
        } else {
            // The retry closed the refused job.
            rejected_ = -1;
            pending_ = job;
            error_->hide();
            editor_->setReadOnly(true);
            update();
        }
    }
    void cancel() {
        if (pending_ < 0)
            close();
    }
    void show_error(const QString &text) {
        if (error_ == nullptr)
            return;
        error_->setText(text);
        error_->setAccessibleName(text);
        error_->show();
        error_->raise();
    }
    /// Closes the editor. A job whose name was refused closes with it, so
    /// nothing waits for a name that will never come.
    void close() {
        if (rejected_ >= 0) {
            const auto rejected = rejected_;
            rejected_ = -1;
            model_->cancelOperation(rejected);
        }
        const bool had_focus = editor_ != nullptr && editor_->hasFocus();
        if (editor_ != nullptr) {
            editor_->abandon();
            editor_->hide();
            editor_->deleteLater();
        }
        if (error_ != nullptr)
            error_->deleteLater();
        editor_ = nullptr;
        error_ = nullptr;
        new_folder_row_->hide();
        if (had_focus)
            view_->focusList();
    }

    FolderItemsListModel *model_;
    FolderItemsList *view_;
    QWidget *new_folder_row_;
    FileNameEditor *editor_ = nullptr;
    QLabel *error_ = nullptr;
    int command_ = -1;
    std::int64_t pending_ = -1;
    std::int64_t rejected_ = -1;
};

class PreviewPane final : public QWidget {
  public:
    explicit PreviewPane(QWidget *parent) : QWidget(parent), split_(new QSplitter(Qt::Horizontal, this)), content_(new QLabel(split_)), details_(new QLabel(split_)) {
        setObjectName(QStringLiteral("previewPane"));
        setMinimumHeight(0);
        setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Ignored);
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        split_->setObjectName(QStringLiteral("previewContentDivider"));
        split_->setHandleWidth(splitter_width);
        content_->setObjectName(QStringLiteral("itemContentPreview"));
        content_->setAccessibleName(QStringLiteral("Item Content Preview"));
        content_->setAlignment(Qt::AlignCenter);
        content_->setMinimumSize(0, 0);
        content_->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Ignored);
        content_->installEventFilter(this);
        details_->setObjectName(QStringLiteral("itemDetailsPreview"));
        details_->setAccessibleName(QStringLiteral("Item Details Preview"));
        details_->setAlignment(Qt::AlignTop | Qt::AlignLeft);
        details_->setMargin(8);
        details_->setMinimumSize(0, 0);
        details_->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Ignored);
        details_->setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::TextSelectableByKeyboard);
        details_->setWordWrap(true);
        details_->setText(QStringLiteral("Select one item to see its details."));
        split_->addWidget(content_);
        split_->addWidget(details_);
        split_->setStretchFactor(0, 2);
        split_->setStretchFactor(1, 1);
        split_->setSizes({2, 1});
        layout->addWidget(split_);
        QObject::connect(split_, &QSplitter::splitterMoved, this, [this](int, int) { queue_target_change(); });
    }

    [[nodiscard]] auto content_size() const -> QSize { return content_->size(); }
    void set_target_changed_handler(std::function<void()> handler) { target_changed_ = std::move(handler); }
    void refresh(FolderItemsListModel *model) {
        details_->setText(model->previewDetails());
        image_ = QImage();
        if (model->previewKind() == 7) {
            const auto bytes = model->previewImagePixels();
            const int width = model->previewImageWidth();
            const int height = model->previewImageHeight();
            if (width > 0 && height > 0 && bytes.size() == static_cast<qsizetype>(width) * height * 4) {
                QImage decoded(width, height, QImage::Format_RGBA8888_Premultiplied);
                if (!decoded.isNull() && decoded.sizeInBytes() == bytes.size()) {
                    std::memcpy(decoded.bits(), bytes.constData(), static_cast<std::size_t>(bytes.size()));
                    image_ = std::move(decoded);
                }
            }
        }
        render_image();
    }

  protected:
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if (watched == content_ && event->type() == QEvent::Resize) {
            queue_target_change();
            QTimer::singleShot(0, this, [this] { render_image(); });
        }
        return QWidget::eventFilter(watched, event);
    }

  private:
    void queue_target_change() {
        if (target_changed_ && !target_change_queued_) {
            target_change_queued_ = true;
            QTimer::singleShot(0, this, [this] {
                target_change_queued_ = false;
                if (target_changed_)
                    target_changed_();
            });
        }
    }
    void render_image() {
        if (image_.isNull()) {
            content_->clear();
            return;
        }
        const QSize target = image_.size().scaled(content_->size(), Qt::KeepAspectRatio);
        const QSize display = target.width() < image_.width() || target.height() < image_.height() ? target : image_.size();
        content_->setPixmap(QPixmap::fromImage(display == image_.size() ? image_ : image_.scaled(display, Qt::KeepAspectRatio, Qt::SmoothTransformation)));
    }
    QSplitter *split_;
    QLabel *content_;
    QLabel *details_;
    QImage image_;
    std::function<void()> target_changed_;
    bool target_change_queued_ = false;
};

/// Owns one native Folder Items menu from popup until its queued completion.
/// Both signal orders are safe: an action is only submitted after the menu has
/// reported closure, and every exit path consumes or dismisses the snapshot at
/// most once.
class FolderItemsMenuSession final : public QObject {
  public:
    struct Row {
        QString label;
        QString shortcut;
        bool starts_section = false;
    };

    FolderItemsMenuSession(const QString &accessible_name, const std::vector<Row> &rows, std::function<void(std::optional<int>)> complete, QWidget *parent) : QObject(parent), complete_(std::move(complete)), menu_(new QMenu(parent)) {
        menu_->setObjectName(QStringLiteral("folderItemsContextMenu"));
        menu_->setAccessibleName(accessible_name);
        for (std::size_t index = 0; index < rows.size(); ++index) {
            const auto &row = rows.at(index);
            if (row.starts_section)
                menu_->addSeparator();
            auto *action = menu_->addAction(row.label);
            action->setData(static_cast<int>(index));
            const auto shortcut = QKeySequence::fromString(row.shortcut, QKeySequence::PortableText).toString(QKeySequence::NativeText);
            if (!shortcut.isEmpty())
                action->setText(action->text() + QStringLiteral("\t") + shortcut);
        }
        QObject::connect(menu_, &QMenu::triggered, this, [this](QAction *action) {
            if (!finished_ && action != nullptr)
                chosen_ = action->data().toInt();
            queue_finish();
        });
        QObject::connect(menu_, &QMenu::aboutToHide, this, [this] {
            closed_ = true;
            queue_finish();
        });
    }

    void popup(const QPoint &at) { menu_->popup(at); }
    [[nodiscard]] auto menu() const -> QMenu * { return menu_; }

    /// A newer request globally retires the older menu before obtaining its
    /// own application snapshot.
    void dismiss_and_close() {
        if (finished_)
            return;
        finished_ = true;
        complete_(std::nullopt);
        if (menu_ != nullptr) {
            menu_->close();
            menu_->deleteLater();
        }
        deleteLater();
    }

  private:
    void queue_finish() {
        if (finished_ || finish_queued_)
            return;
        finish_queued_ = true;
        QTimer::singleShot(0, this, [this] {
            finish_queued_ = false;
            if (finished_ || !closed_)
                return;
            finished_ = true;
            complete_(chosen_);
            if (menu_ != nullptr)
                menu_->deleteLater();
            deleteLater();
        });
    }

    std::function<void(std::optional<int>)> complete_;
    QPointer<QMenu> menu_;
    std::optional<int> chosen_;
    bool closed_ = false;
    bool finish_queued_ = false;
    bool finished_ = false;
};

/// Enforces the workspace-wide single live Folder Items menu contract across
/// the two Browser widgets.
class FolderItemsMenuCoordinator final : public QObject {
  public:
    explicit FolderItemsMenuCoordinator(QWidget *parent) : QObject(parent) {}

    void dismiss() {
        if (current_ != nullptr)
            current_->dismiss_and_close();
        current_.clear();
    }

    void show(FolderItemsListModel *model, const QPoint &at, int row, bool comprehensive) {
        dismiss();
        if (!model->requestContextMenu(comprehensive ? 1 : 0, row))
            return;
        std::vector<FolderItemsMenuSession::Row> rows;
        rows.reserve(static_cast<std::size_t>(model->contextMenuActionCount()));
        for (int index = 0; index < model->contextMenuActionCount(); ++index)
            rows.push_back({model->contextMenuActionLabel(index), model->contextMenuActionShortcut(index), model->contextMenuActionStartsSection(index)});
        const auto accessible_name = model->contextMenuKind() == 1 ? QStringLiteral("Comprehensive Folder Items Context Menu") : QStringLiteral("Normal Folder Items Context Menu");
        current_ = new FolderItemsMenuSession(accessible_name, rows, [model](std::optional<int> chosen) {
            if (chosen.has_value())
                model->invokeContextMenuAction(*chosen);
            else
                model->dismissContextMenu(); }, qobject_cast<QWidget *>(parent()));
        current_->popup(at);
    }

  private:
    QPointer<FolderItemsMenuSession> current_;
};

class Browser final : public QWidget {
  public:
    Browser(FolderItemsListModel *model, IconLoader *icons, FolderItemsMenuCoordinator *menus, QWidget *parent) : QWidget(parent), model_(model), folder_(new FolderPane(model, icons, this)), preview_splitter_(new ThinSplitter(SplitterKind::Preview, this)), preview_(new PreviewPane(preview_splitter_)), overlay_(new QLabel(QStringLiteral("Expand"), this)), missing_(new QLabel(QStringLiteral("Command+R to Refresh"), this)), editors_(new NameEditController(model, folder_->view(), folder_->new_folder_row())), preview_settle_(this) {
        editors_->setParent(this);
        setObjectName(QStringLiteral("browser"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        layout->addWidget(new BrowserTabsStrip(model, this, [this] { folder_->view()->focusList(); }));
        path_ = new PathEditControl(model, [this] { folder_->view()->focusList(); }, this);
        layout->addWidget(path_);
        preview_splitter_->addWidget(folder_);
        preview_splitter_->addWidget(preview_);
        preview_splitter_->setCollapsible(0, false);
        preview_splitter_->setCollapsible(1, true);
        layout->addWidget(preview_splitter_, 1);
        auto *status = folder_->status();
        status->setParent(this);
        layout->addWidget(status);
        preview_splitter_->setSizes({1, 0});
        preview_settle_.setSingleShot(true);
        preview_splitter_->set_settled_handler([this] {
            if (!applying_preview_height_)
                preview_settle_.start(0);
        });
        QObject::connect(&preview_settle_, &QTimer::timeout, this, [this] {
            const int moved = preview_splitter_->sizes().value(1);
            chosen_preview_height_ = moved > 0 && moved < qMax(80, fontMetrics().height() * 5) ? qMax(80, fontMetrics().height() * 5) : moved;
            apply_preview_height();
            if (preview_changed_)
                preview_changed_();
        });
        preview_->set_target_changed_handler([this] { send_preview_target(); });
        QObject::connect(model, &FolderItemsListModel::previewRevisionChanged, this, [this, model] { preview_->refresh(model); });
        QObject::connect(model, &FolderItemsListModel::tabsRevisionChanged, this, [this] {
            last_preview_target_ = QSize(-1, -1);
            QTimer::singleShot(0, this, [this] { send_preview_target(); });
        });
        preview_->refresh(model);
        overlay_->setObjectName(QStringLiteral("expandOverlay"));
        overlay_->setAccessibleName(QStringLiteral("Expand"));
        overlay_->setAlignment(Qt::AlignCenter);
        overlay_->setAttribute(Qt::WA_TransparentForMouseEvents);
        auto font = overlay_->font();
        font.setPointSize(28);
        font.setBold(true);
        overlay_->setFont(font);
        overlay_->hide();
        missing_->setObjectName(QStringLiteral("missingFolderOverlay"));
        missing_->setAccessibleName(QStringLiteral("Missing Folder Overlay"));
        missing_->setAlignment(Qt::AlignCenter);
        missing_->setAttribute(Qt::WA_TransparentForMouseEvents);
        auto missing_font = missing_->font();
        missing_font.setPointSize(18);
        missing_font.setBold(true);
        missing_->setFont(missing_font);
        missing_->hide();
        QObject::connect(model, &FolderItemsListModel::missingFolderChanged, this, [this] { update_overlays(); });
        // A refused file command's reason shows for a few seconds, then the
        // Status Bar returns to its loading, error, or path text.
        QObject::connect(model, &FolderItemsListModel::statusTokenChanged, this, [this, model] {
            const int token = model->getStatusToken();
            QAccessibleAnnouncementEvent announcement(folder_->status(), model->getStatusText());
            QAccessible::updateAccessibility(&announcement);
            QTimer::singleShot(status_reason_milliseconds, model, [model, token] { model->expireStatus(token); });
        });
        folder_->view()->setMenuHandler([menus, model](const QPoint &at, int row, bool comprehensive) { menus->show(model, at, row, comprehensive); });
    }

    [[nodiscard]] auto view() const -> FolderItemsList * { return folder_->view(); }
    [[nodiscard]] auto folder() const -> FolderPane * { return folder_; }
    [[nodiscard]] auto editors() const -> NameEditController * { return editors_; }
    void focus_filter() { folder_->focus_filter(); }
    void edit_path() { path_->edit(); }
    [[nodiscard]] auto path_control() const -> PathEditControl * { return path_; }
    [[nodiscard]] auto preview_splitter() const -> ThinSplitter * { return preview_splitter_; }
    [[nodiscard]] auto preview_height() const -> int { return chosen_preview_height_; }
    void set_preview_changed_handler(std::function<void()> handler) { preview_changed_ = std::move(handler); }
    void set_preview_height(int height) {
        preview_settle_.stop();
        chosen_preview_height_ = qBound(0, height, 32'767);
        apply_preview_height();
    }

  protected:
    /// Below the usable width, an instruction covers only this Browser; it
    /// passes pointer input through and takes no keyboard commands.
    void resizeEvent(QResizeEvent *event) override {
        QWidget::resizeEvent(event);
        QTimer::singleShot(0, this, [this] { apply_preview_height(); });
        update_overlays();
    }

  private:
    void apply_preview_height() {
        if (preview_splitter_->height() <= 0)
            return;
        const int minimum_folder = folder_->minimumSizeHint().height() + folder_->view()->fontMetrics().height();
        const int available = qMax(0, preview_splitter_->height() - preview_splitter_->handleWidth() - minimum_folder);
        const int useful = qMax(80, fontMetrics().height() * 5);
        const int effective = chosen_preview_height_ > 0 && available >= useful ? qMin(available, qMax(useful, chosen_preview_height_)) : 0;
        const int upper = qMax(0, preview_splitter_->height() - preview_splitter_->handleWidth() - effective);
        applying_preview_height_ = true;
        preview_splitter_->setSizes({upper, effective});
        applying_preview_height_ = false;
        QTimer::singleShot(0, this, [this] { send_preview_target(); });
    }
    void send_preview_target() {
        const int useful = qMax(80, fontMetrics().height() * 5);
        const QSize target = preview_->height() >= useful ? preview_->content_size() : QSize(0, 0);
        if (target == last_preview_target_)
            return;
        last_preview_target_ = target;
        model_->setPreviewTarget(qMax(0, target.width()), qMax(0, target.height()));
    }
    /// The narrow-Browser Expand overlay takes precedence over the Missing
    /// Folder Overlay, which covers only the dimmed Folder Items.
    void update_overlays() {
        overlay_->setGeometry(rect());
        const bool narrow = width() < narrow_browser_width;
        overlay_->setVisible(narrow);
        overlay_->raise();
        auto *view = folder_->view();
        const bool missing = model_->getMissingFolder();
        if (view->property("missingFolder").toBool() != missing) {
            view->setProperty("missingFolder", missing);
            repolish(view);
            view->viewport()->update();
        }
        missing_->setGeometry(QRect(view->viewport()->mapTo(this, QPoint(0, 0)), view->viewport()->size()));
        missing_->setVisible(missing && !narrow);
        if (missing && !narrow)
            missing_->raise();
    }
    FolderItemsListModel *model_;
    FolderPane *folder_;
    PathEditControl *path_ = nullptr;
    ThinSplitter *preview_splitter_;
    PreviewPane *preview_;
    int chosen_preview_height_ = 0;
    bool applying_preview_height_ = false;
    QTimer preview_settle_;
    std::function<void()> preview_changed_;
    QSize last_preview_target_;
    QLabel *overlay_;
    QLabel *missing_;
    NameEditController *editors_;
};

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_FOLDER_PANE_HPP
