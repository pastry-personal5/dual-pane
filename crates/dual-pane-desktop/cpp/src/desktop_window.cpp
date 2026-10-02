#include "dual_pane_desktop/desktop_window.hpp"
#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/settings_glyph.hpp"

#include <QtCore/QCache>
#include <QtCore/QEvent>
#include <QtCore/QFileInfo>
#include <QtCore/QHash>
#include <QtCore/QItemSelection>
#include <QtCore/QItemSelectionModel>
#include <QtCore/QMetaObject>
#include <QtCore/QMimeData>
#include <QtCore/QPersistentModelIndex>
#include <QtCore/QPointer>
#include <QtCore/QStorageInfo>
#include <QtCore/QString>
#include <QtCore/QThread>
#include <QtCore/QTimer>
#include <QtCore/QVariant>
#include <QtGui/QAbstractFileIconProvider>
#include <QtGui/QColor>
#include <QtGui/QDrag>
#include <QtGui/QFocusEvent>
#include <QtGui/QFontDatabase>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QPaintEvent>
#include <QtGui/QPainter>
#include <QtGui/QPen>
#include <QtGui/QPixmap>
#include <QtGui/QShortcut>
#include <QtWidgets/QAbstractButton>
#include <QtWidgets/QAbstractItemView>
#include <QtWidgets/QApplication>
#include <QtWidgets/QFrame>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QHeaderView>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QMenu>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QScrollArea>
#include <QtWidgets/QScrollBar>
#include <QtWidgets/QSplitter>
#include <QtWidgets/QSplitterHandle>
#include <QtWidgets/QStyle>
#include <QtWidgets/QStyleOptionViewItem>
#include <QtWidgets/QStyledItemDelegate>
#include <QtWidgets/QToolButton>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>

#include <algorithm>
#include <array>
#include <atomic>
#include <cstdint>
#include <functional>
#include <iterator>
#include <memory>
#include <mutex>
#include <optional>
#include <pthread.h>
#include <utility>
#include <vector>

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop {
namespace {
constexpr int initial_window_width = 1100, initial_window_height = 680;
constexpr int splitter_width = 3;
constexpr int narrow_browser_width = 280;
constexpr int column_count = 6;
constexpr int icon_column_width = 18;
constexpr int relative_date_column_width = 44;
constexpr int type_column_width = 44;
constexpr int path_role = Qt::UserRole;
constexpr int relative_age_role = Qt::UserRole + 1;
constexpr int max_pending_icons = 128;
constexpr int icon_cache_size = 1024;
constexpr int tab_limit = 8;
constexpr auto favorite_item_mime = "application/x-dual-pane-favorite-item";
constexpr auto window_color = "#1B1D21", surface_color = "#23262B", text_color = "#ECEFF3", border_color = "#3A4048", active_color = "#2F6D9A", inactive_browser_color = "#1E4668", divider_hover_border_color = "#737A84", error_color = "#E5737A";

// `toolbarState` flags from the Folder Items model.
constexpr unsigned toolbar_new_tab = 1U, toolbar_back = 2U, toolbar_forward = 4U, toolbar_up = 8U;

constexpr auto has_flag(int state, unsigned flag) -> bool { return (static_cast<unsigned>(state) & flag) != 0U; }

enum class Movement : std::uint8_t { Previous,
                                     Next,
                                     First,
                                     Last,
                                     PageUp,
                                     PageDown,
};

constexpr auto movement_for(int key) -> std::optional<Movement> {
    switch (key) {
    case Qt::Key_Up:
        return Movement::Previous;
    case Qt::Key_Down:
        return Movement::Next;
    case Qt::Key_Home:
        return Movement::First;
    case Qt::Key_End:
        return Movement::Last;
    case Qt::Key_PageUp:
        return Movement::PageUp;
    case Qt::Key_PageDown:
        return Movement::PageDown;
    default:
        return std::nullopt;
    }
}

constexpr auto user_modifiers(Qt::KeyboardModifiers modifiers) -> Qt::KeyboardModifiers {
    // Qt sets KeypadModifier for the physical arrow cluster on macOS. It
    // describes the key's origin, not a modifier held by the user.
    return modifiers & ~Qt::KeypadModifier;
}

static_assert(movement_for(Qt::Key_Up) == Movement::Previous);
static_assert(movement_for(Qt::Key_PageDown) == Movement::PageDown);
static_assert(!movement_for(Qt::Key_Left).has_value());
static_assert(user_modifiers(Qt::KeypadModifier) == Qt::NoModifier);
static_assert(user_modifiers(Qt::ShiftModifier | Qt::KeypadModifier) == Qt::ShiftModifier);

auto style_sheet() -> QString {
    return QStringLiteral(R"(
      QMainWindow, QWidget#browser, QWidget#sidebar, QWidget#favoritesGroups, QWidget#notices, QWidget#noticesList { background:%1; color:%3; }
      QLabel, QLineEdit { color:%3; background:%2; }
      QLineEdit { border:1px solid %4; padding:4px; }
      QLabel#favoriteError, QLabel#favoriteCue { color:%8; background:%1; }
      QFrame#folderPane { background:%1; border:1px solid %4; }
      QFrame#folderPane[browserActive="true"][windowActive="true"] { background:%2; border:1px solid %4; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] { background:%1; border:1px solid %4; }
      QFrame#folderPane[browserActive="true"][windowActive="true"] QTreeView { background:%2; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] QTreeView { background:%1; color:#A7ADB5; }
      QTreeView { background:%2; color:%3; border:0; outline:none; }
      QTreeView::item { padding-left:0; }
      QTreeView::item:selected { background:%6; color:white; }
      QTreeView[browserActive="true"][windowActive="true"]::item:selected { background:%5; color:white; }
      QToolButton { background:%2; color:%3; border:1px solid %4; padding:4px; }
      QToolButton:disabled { color:%7; }
      QLabel#folderPaneToolbarRow1, QWidget#folderPaneToolbarRow2, QWidget#folderPaneToolbarRow3, QWidget#folderPaneToolbarRow2 QLabel, QWidget#folderPaneToolbarRow2 QLineEdit, QWidget#folderPaneToolbarRow2 QToolButton, QWidget#folderPaneToolbarRow3 QLabel, QWidget#folderPaneToolbarRow3 QLineEdit, QWidget#folderPaneToolbarRow3 QToolButton { background:%1; }
      QLabel#folderPaneToolbarRow1[browserActive="true"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"], QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"] QToolButton, QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"] QToolButton { background:%2; }
      QLabel#folderPaneToolbarRow1[browserActive="false"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"], QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"] QToolButton, QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"] QToolButton { background:%1; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] QLabel, QFrame#folderPane[browserActive="false"][windowActive="true"] QLineEdit { background:%1; color:#A7ADB5; }
      QToolButton#sortControl { min-width:16px; max-width:16px; min-height:16px; max-height:16px; padding:0; background:%1; color:#A7ADB5; }
      QToolButton#sortControl:checked { color:#FFFFFF; }
      QToolButton#upButton, QToolButton#backButton, QToolButton#forwardButton, QToolButton#sortControl, QToolButton#newTabButton, QToolButton#closeTabButton, QToolButton#favoriteGroupMenuButton, QToolButton#addFavoriteItemButton, QToolButton#newGroupButton { border:none; }
      QWidget#browserTabsStrip { background:%1; }
      QLabel#expandOverlay { background:rgba(0, 0, 0, 160); color:white; }
      QSplitter::handle { background:%4; }
    )")
        .arg(window_color)
        .arg(surface_color)
        .arg(text_color)
        .arg(border_color)
        .arg(active_color)
        .arg(inactive_browser_color)
        .arg(divider_hover_border_color)
        .arg(error_color);
}

void repolish(QWidget *widget) {
    widget->style()->unpolish(widget);
    widget->style()->polish(widget);
    widget->update();
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
            const auto background = age.isValid() ? QColor::fromHslF(static_cast<float>(0.75 * position), active ? 0.55F : 0.25F, active ? 0.8F : 0.68F) : QColor(active ? QStringLiteral("#D0D0D0") : QStringLiteral("#989898"));
            const auto swatch = option.rect.adjusted(2, 2, -2, -2);
            painter->save();
            painter->fillRect(swatch, background);
            painter->setClipRect(swatch);
            painter->setPen(Qt::black);
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
                painter->drawPixmap(target, *pixmap);
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
constexpr std::array<int, column_count> column_minimum = {icon_column_width, 120, type_column_width, relative_date_column_width, 118, 72};
constexpr std::array<int, column_count - 1> hide_order = {5, 4, 3, 2, 0};

class FolderItemsList final : public QTreeView {
  public:
    FolderItemsList(FolderItemsListModel *model, IconLoader *loader) : model_(model), icons_(new ItemIconDelegate(loader, this)) {
        setModel(model_);
        setItemDelegate(icons_);
        auto folder_font = font();
        folder_font.setPointSize(12);
        setFont(folder_font);
        setFocusPolicy(Qt::StrongFocus);
        setSelectionMode(QAbstractItemView::ExtendedSelection);
        setSelectionBehavior(QAbstractItemView::SelectRows);
        setEditTriggers(QAbstractItemView::NoEditTriggers);
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
        connect(model_, &QAbstractItemModel::modelAboutToBeReset, this, [this] { icons_->invalidate(); });
        connect(model_, &QAbstractItemModel::modelReset, this, [this] {
            synchronize_selection();
            restore_scroll();
        });
        connect(verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { report_scroll(); });
    }

    void setColumnsHandler(std::function<void(const std::array<bool, column_count> &)> handler) { columns_handler_ = std::move(handler); }

    /// Moves keyboard focus here, which activates this Browser.
    void focusList() {
        if (!hasFocus())
            setFocus(Qt::OtherFocusReason);
    }

  protected:
    auto selectionCommand(const QModelIndex &, const QEvent *) const -> QItemSelectionModel::SelectionFlags override { return QItemSelectionModel::NoUpdate; }

    void mousePressEvent(QMouseEvent *event) override {
        event->accept();
        const auto index = indexAt(event->position().toPoint());
        if (event->button() == Qt::LeftButton) {
            focusList();
            // Qt maps ControlModifier to the Command key on macOS.
            const auto modifiers = user_modifiers(event->modifiers());
            if (!index.isValid())
                model_->clearSelection();
            else if (modifiers == Qt::ControlModifier)
                model_->toggleRow(index.row());
            else if (modifiers == Qt::ShiftModifier)
                model_->extendToRow(index.row());
            else
                model_->selectRow(index.row());
        } else if (event->button() == Qt::RightButton) {
            focusList();
            if (index.isValid())
                model_->secondaryRow(index.row());
        }
    }
    void mouseReleaseEvent(QMouseEvent *event) override { event->accept(); }
    void mouseMoveEvent(QMouseEvent *event) override { event->accept(); }
    void mouseDoubleClickEvent(QMouseEvent *event) override {
        event->accept();
        if (event->button() != Qt::LeftButton || user_modifiers(event->modifiers()) != Qt::NoModifier)
            return;
        if (const auto index = indexAt(event->position().toPoint()); index.isValid()) {
            model_->selectRow(index.row());
            model_->activateRow(index.row());
        }
    }
    void keyPressEvent(QKeyEvent *event) override {
        event->accept();
        const auto modifiers = user_modifiers(event->modifiers());
        const auto key = event->key();
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
        else if (key == Qt::Key_Right || key == Qt::Key_Return || key == Qt::Key_Enter)
            model_->activateSelected();
    }
    void resizeEvent(QResizeEvent *event) override {
        QTreeView::resizeEvent(event);
        update_columns();
    }

  private:
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
    bool restoring_ = false;
};

/// One Browser Tab. It does not own activation: clicks and keys request it.
class BrowserTabButton final : public QAbstractButton {
  public:
    BrowserTabButton(std::uint64_t id, QWidget *parent) : QAbstractButton(parent), id_(id), close_(new QToolButton(this)) {
        setObjectName(QStringLiteral("browserTab"));
        setFocusPolicy(Qt::TabFocus);
        setMouseTracking(true);
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
        painter.fillRect(rect(), QColor(QString::fromLatin1(active_ ? surface_color : window_color)));
        painter.setPen(QColor(QString::fromLatin1(border_color)));
        painter.drawLine(rect().topRight(), rect().bottomRight());
        if (active_)
            painter.fillRect(QRect(0, 0, width(), 2), QColor(QString::fromLatin1(active_color)));
        if (hasFocus()) {
            painter.setPen(QColor(QString::fromLatin1(active_color)));
            painter.drawRect(rect().adjusted(1, 1, -2, -2));
        }
        painter.setPen(QColor(QString::fromLatin1(text_color)));
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

auto command_button(const QString &object_name, const QString &text, const QString &tool_tip, const QString &accessible_name, QWidget *parent) -> QToolButton * {
    auto *button = new QToolButton(parent);
    button->setObjectName(object_name);
    button->setText(text);
    button->setToolTip(tool_tip);
    button->setAccessibleName(accessible_name);
    button->setFocusPolicy(Qt::NoFocus);
    return button;
}

auto sort_caret_icon(const bool descending, const bool focused) -> QIcon {
    auto icon = QIcon();
    const auto draw_caret = [descending](const QColor &color, const qreal scale) {
        QPixmap pixmap(QSize(static_cast<int>(16 * scale), static_cast<int>(16 * scale)));
        pixmap.setDevicePixelRatio(scale);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing);
        auto pen = QPen(color);
        pen.setWidth(2);
        pen.setCapStyle(Qt::RoundCap);
        pen.setJoinStyle(Qt::RoundJoin);
        painter.setPen(pen);
        if (descending) {
            painter.drawLine(QPointF(4, 6), QPointF(8, 10));
            painter.drawLine(QPointF(8, 10), QPointF(12, 6));
        } else {
            painter.drawLine(QPointF(4, 10), QPointF(8, 6));
            painter.drawLine(QPointF(8, 6), QPointF(12, 10));
        }
        return pixmap;
    };
    const auto add_variants = [&icon, &draw_caret](const QColor &color, const QIcon::Mode mode, const QIcon::State state) {
        icon.addPixmap(draw_caret(color, 1.0), mode, state);
        icon.addPixmap(draw_caret(color, 2.0), mode, state);
    };
    add_variants(QColor(focused ? QStringLiteral("#A7ADB5") : QStringLiteral("#737A84")), QIcon::Normal, QIcon::Off);
    add_variants(QColor(focused ? QStringLiteral("#FFFFFF") : QStringLiteral("#A7ADB5")), QIcon::Normal, QIcon::On);
    add_variants(QColor(QStringLiteral("#737A84")), QIcon::Disabled, QIcon::Off);
    add_variants(QColor(QStringLiteral("#737A84")), QIcon::Disabled, QIcon::On);
    return icon;
}

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
        view_->setObjectName(QStringLiteral("folderItemsList"));
        view_->setAccessibleName(QStringLiteral("Folder Items List"));
        auto *status = new QLabel(this);
        status->setObjectName(QStringLiteral("browserStatusBar"));
        status->setAccessibleName(QStringLiteral("Browser Status Bar"));
        status_ = status;
        layout->addWidget(title);
        layout->addWidget(summary);
        layout->addWidget(commands);
        layout->addWidget(view_, 1);
        layout->addWidget(status);
        QObject::connect(model, &FolderItemsListModel::folderNameChanged, this, [title, model] { title->setText(model->getFolderName()); });
        QObject::connect(model, &FolderItemsListModel::summaryCountTextChanged, this, [summary_count, model] { summary_count->setText(model->getSummaryCountText()); });
        QObject::connect(model, &FolderItemsListModel::summarySelectedTextChanged, this, [summary_selected, model] { summary_selected->setText(model->getSummarySelectedText()); });
        QObject::connect(model, &FolderItemsListModel::summarySizeTextChanged, this, [summary_size, model] { summary_size->setText(model->getSummarySizeText()); });
        QObject::connect(model, &FolderItemsListModel::statusTextChanged, this, [status, model] { status->setText(model->getStatusText()); });
        const auto update_toolbar = [back, forward, up, model] {
            const int state = model->getToolbarState();
            back->setEnabled(has_flag(state, toolbar_back));
            forward->setEnabled(has_flag(state, toolbar_forward));
            up->setEnabled(has_flag(state, toolbar_up));
        };
        update_toolbar();
        QObject::connect(model, &FolderItemsListModel::toolbarStateChanged, this, update_toolbar);
        QObject::connect(model, &FolderItemsListModel::sortChoiceChanged, this, [this, model] { update_sort(model); });
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
    [[nodiscard]] auto status() const -> QLabel * { return status_; }

  private:
    void update_sort(FolderItemsListModel *model) {
        const int choice = model->getSortChoice();
        for (int index = 0; index < static_cast<int>(sorts_.size()); ++index) {
            sorts_.at(index)->setEnabled(choice >= 0);
            sorts_.at(index)->setChecked(index == choice);
        }
    }

    FolderItemsList *view_;
    QLabel *status_ = nullptr;
    std::array<QWidget *, 4> sort_groups_{};
    std::array<QToolButton *, 8> sorts_{};
};

class Browser final : public QWidget {
  public:
    Browser(FolderItemsListModel *model, IconLoader *icons, QWidget *parent) : QWidget(parent), folder_(new FolderPane(model, icons, this)), overlay_(new QLabel(QStringLiteral("Expand"), this)) {
        setObjectName(QStringLiteral("browser"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        layout->addWidget(new BrowserTabsStrip(model, this, [this] { folder_->view()->focusList(); }));
        auto *path = new QLineEdit(this);
        path->setObjectName(QStringLiteral("pathEditControl"));
        path->setReadOnly(true);
        path->setFocusPolicy(Qt::NoFocus);
        path->setAttribute(Qt::WA_TransparentForMouseEvents);
        path->setAccessibleName(QStringLiteral("Path Edit Control"));
        layout->addWidget(path);
        layout->addWidget(folder_, 1);
        QObject::connect(model, &FolderItemsListModel::pathTextChanged, this, [path, model] { path->setText(model->getPathText()); });
        overlay_->setObjectName(QStringLiteral("expandOverlay"));
        overlay_->setAccessibleName(QStringLiteral("Expand"));
        overlay_->setAlignment(Qt::AlignCenter);
        overlay_->setAttribute(Qt::WA_TransparentForMouseEvents);
        auto font = overlay_->font();
        font.setPointSize(28);
        font.setBold(true);
        overlay_->setFont(font);
        overlay_->hide();
    }

    [[nodiscard]] auto view() const -> FolderItemsList * { return folder_->view(); }
    [[nodiscard]] auto folder() const -> FolderPane * { return folder_; }

  protected:
    /// Below the usable width, an instruction covers only this Browser; it
    /// passes pointer input through and takes no keyboard commands.
    void resizeEvent(QResizeEvent *event) override {
        QWidget::resizeEvent(event);
        overlay_->setGeometry(rect());
        overlay_->setVisible(width() < narrow_browser_width);
        overlay_->raise();
    }

  private:
    FolderPane *folder_;
    QLabel *overlay_;
};

/// An inline name editor. Return commits, Escape cancels, and focus loss
/// commits valid text or silently cancels. While it has focus, only Quit
/// stays bound.
class InlineNameEditor final : public QLineEdit {
  public:
    /// `commit` returns inline error text, empty when the edit is done.
    InlineNameEditor(const QString &object_name, const QString &accessible_name, const QString &text, std::function<QString(const QString &)> commit, std::function<void()> finish, QLabel *error, QWidget *parent) : QLineEdit(text, parent), commit_(std::move(commit)), finish_(std::move(finish)), error_(error) {
        setObjectName(object_name);
        setAccessibleName(accessible_name);
        setProperty("inlineEditor", true);
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
    FavoriteItemRow(std::int64_t id, const QString &alias, const QString &path, QWidget *parent) : QLabel(alias, parent), id_(id) {
        setObjectName(QStringLiteral("favoriteItem"));
        setAccessibleName(alias);
        setAccessibleDescription(path);
        setToolTip(path);
        setFocusPolicy(Qt::TabFocus);
        setIndent(18);
    }
    [[nodiscard]] auto id() const -> std::int64_t { return id_; }
    void setOpenHandler(std::function<void()> handler) { on_open_ = std::move(handler); }
    void setMenuHandler(std::function<void(const QPoint &)> handler) { on_menu_ = std::move(handler); }
    /// Moves focus by `step` Items, for arrow keys under Full Keyboard Access.
    void setStepHandler(std::function<void(int)> handler) { on_step_ = std::move(handler); }

  protected:
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
        QLabel::paintEvent(event);
        if (hasFocus()) {
            QPainter painter(this);
            painter.setPen(QColor(QString::fromLatin1(active_color)));
            painter.drawRect(rect().adjusted(0, 0, -1, -1));
        }
    }

  private:
    std::int64_t id_;
    std::optional<QPoint> press_;
    std::function<void()> on_open_;
    std::function<void(const QPoint &)> on_menu_;
    std::function<void(int)> on_step_;
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
        layout_->setSpacing(2);
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
            const auto header = section.header->geometry();
            if (position.y() < header.top() && target.group >= 0)
                break;
            target = {section.group, 0, header.bottom() + 1};
            for (const auto *item : section.items) {
                if (position.y() < item->geometry().center().y())
                    return target;
                target.slot += 1;
                target.y = item->geometry().bottom() + 1;
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
            auto *header = new QWidget(this);
            header->setObjectName(QStringLiteral("favoriteGroup"));
            auto *row = new QHBoxLayout(header);
            row->setContentsMargins(4, 4, 0, 0);
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
            QObject::connect(add, &QToolButton::clicked, this, [this, id] {
                const auto error = bridge_->addItem(id);
                if (!error.isEmpty())
                    show_cue(id, error);
            });
            row->addWidget(add);
            layout_->addWidget(header);
            auto *cue = new QLabel(this);
            cue->setObjectName(QStringLiteral("favoriteCue"));
            cue->hide();
            layout_->addWidget(cue);
            cues_.insert(id, cue);
            Section section{id, header, {}};
            for (int item = 0; item < bridge_->itemCount(group); ++item) {
                const auto item_id = bridge_->itemId(group, item);
                const auto alias = bridge_->itemAlias(group, item);
                if (editing_kind_ == EditKind::Item && editing_id_ == item_id) {
                    auto *holder = new QWidget(this);
                    auto *holder_layout = new QHBoxLayout(holder);
                    holder_layout->setContentsMargins(16, 0, 0, 0);
                    add_editor(holder_layout, holder, QStringLiteral("favoriteItemAliasEditor"), QStringLiteral("Favorite Item Alias Editor"), alias, [this, item_id](const QString &text) { return bridge_->renameItem(item_id, text); });
                    layout_->addWidget(holder);
                    continue;
                }
                auto *row_widget = new FavoriteItemRow(item_id, alias, bridge_->itemPath(group, item), this);
                row_widget->setOpenHandler([this, item_id] {
                    bridge_->openItem(item_id);
                    focus_active_list_();
                });
                row_widget->setMenuHandler([this, item_id](const QPoint &at) { show_item_menu(item_id, at); });
                layout_->addWidget(row_widget);
                section.items.push_back(row_widget);
                all_items.push_back(row_widget);
            }
            sections_.push_back(std::move(section));
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
            holder_layout->setContentsMargins(4, 0, 0, 0);
            add_editor(holder_layout, holder, QStringLiteral("favoriteGroupNameEditor"), QStringLiteral("Favorite Group Name Editor"), QString(), [this](const QString &text) { return bridge_->createGroup(text); });
            layout_->addWidget(holder);
        }
        auto *new_group = command_button(QStringLiteral("newGroupButton"), QStringLiteral("New Group"), QStringLiteral("New Group"), QStringLiteral("New Group Button"), this);
        new_group->setFocusPolicy(Qt::TabFocus);
        new_group->setEnabled(ready && !editing_);
        QObject::connect(new_group, &QToolButton::clicked, this, [this] { begin_edit(EditKind::Draft, -1); });
        layout_->addWidget(new_group, 0, Qt::AlignLeft);
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
};

/// The auxiliary Notices window. It never blocks browsing; storage errors
/// offer Reset Settings behind an explicit confirmation.
class NoticesWindow final : public QWidget {
  public:
    NoticesWindow(WorkspaceBridge *bridge, QWidget *owner) : QWidget(owner, Qt::Window), bridge_(bridge), list_(new QWidget()) {
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
        }
        if (bridge_->noticeCount() == 0)
            layout->addWidget(new QLabel(QStringLiteral("No notices."), list_));
        dynamic_cast<QVBoxLayout *>(layout)->addStretch();
    }
    void confirm_reset() {
        auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Reset Settings"), QStringLiteral("Reset Dual Pane’s settings?"), QMessageBox::NoButton, this);
        box->setInformativeText(QStringLiteral("Your settings and Favorites, including changes made in this session, will be replaced with defaults. The current settings database is kept as a backup."));
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
};

class MainToolbar final : public QWidget {
  public:
    MainToolbar(QWidget *parent, const std::function<void()> &show_notices) : QWidget(parent) {
        setObjectName(QStringLiteral("mainToolbar"));
        setAccessibleName(QStringLiteral("Main Toolbar"));
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
        auto *notices = new QToolButton(this);
        notices->setObjectName(QStringLiteral("noticesButton"));
        notices->setText(QStringLiteral("Notices"));
        notices->setToolTip(QStringLiteral("Notices"));
        notices->setAccessibleName(QStringLiteral("Notices Button"));
        notices->setFocusPolicy(Qt::TabFocus);
        QObject::connect(notices, &QToolButton::clicked, this, show_notices);
        layout->addWidget(notices);
        layout->addStretch();
    }
};

class Sidebar final : public QWidget {
  public:
    Sidebar(WorkspaceBridge *bridge, const std::function<void()> &focus_active_list, const std::function<void()> &show_notices, QWidget *parent) : QWidget(parent) {
        setObjectName(QStringLiteral("sidebar"));
        setAccessibleName(QStringLiteral("Sidebar"));
        auto *layout = new QVBoxLayout(this);
        auto *scroll = new QScrollArea(this);
        scroll->setWidgetResizable(true);
        scroll->setFrameShape(QFrame::NoFrame);
        auto *content = new QWidget(scroll);
        content->setObjectName(QStringLiteral("favoritesGroups"));
        auto *content_layout = new QVBoxLayout(content);
        content_layout->setContentsMargins(0, 0, 0, 0);
        content_layout->addWidget(new FavoritesPanel(bridge, focus_active_list, content));
        content_layout->addStretch(1);
        scroll->setWidget(content);
        layout->addWidget(scroll, 1);
        layout->addWidget(new MainToolbar(this, show_notices));
    }
};

class BrowserHighlightController final : public QObject {
  public:
    BrowserHighlightController(QWidget *window, std::array<QFrame *, 2> folders, std::array<FolderItemsList *, 2> views) : window_(window), folders_(folders), views_(views) {
        window_->installEventFilter(this);
        apply(window_->isActiveWindow());
    }

    void activate(int browser) {
        active_ = browser;
        apply(window_->isActiveWindow());
    }

  private:
    auto eventFilter(QObject *watched, QEvent *event) -> bool override {
        if (watched == window_ && event->type() == QEvent::WindowActivate)
            apply(true);
        else if (watched == window_ && event->type() == QEvent::WindowDeactivate)
            apply(false);
        return QObject::eventFilter(watched, event);
    }

    void apply(bool window_active) {
        for (int browser = 0; browser < 2; ++browser) {
            const auto apply_state = [this, browser, window_active](QWidget *widget) {
                widget->setProperty("browserActive", browser == active_);
                widget->setProperty("windowActive", window_active);
                if (auto *sort = dynamic_cast<QToolButton *>(widget); sort != nullptr && sort->objectName() == QStringLiteral("sortControl"))
                    sort->setIcon(sort_caret_icon(sort->property("sortDescending").toBool(), browser == active_ && window_active));
                repolish(widget);
            };
            auto *folder = static_cast<QWidget *>(folders_.at(browser));
            apply_state(folder);
            for (QWidget *child : folder->findChildren<QWidget *>())
                apply_state(child);
            apply_state(views_.at(browser));
            views_.at(browser)->viewport()->update();
        }
    }

    QWidget *window_;
    std::array<QFrame *, 2> folders_;
    std::array<FolderItemsList *, 2> views_;
    int active_ = 0;
};

/// Binds every delivered action to its effective shortcut from settings and
/// rebinds after a load or reset. While an inline editor has focus, only Quit
/// stays active.
class ShortcutBinder final {
  public:
    using Handler = std::function<void()>;
    ShortcutBinder(QWidget *window, WorkspaceBridge *bridge, QHash<QString, Handler> handlers) : window_(window), bridge_(bridge), handlers_(std::move(handlers)) {}

    void rebind() {
        for (auto *shortcut : shortcuts_) {
            // deleteLater leaves the old binding alive until the event loop
            // resumes; it must stop matching keys before replacements exist.
            shortcut->setEnabled(false);
            shortcut->deleteLater();
        }
        shortcuts_.clear();
        for (int index = 0; index < bridge_->bindingCount(); ++index) {
            const auto action = bridge_->bindingAction(index);
            const auto sequence = bridge_->bindingSequence(index);
            const auto handler = handlers_.value(action);
            if (sequence.isEmpty() || !handler)
                continue;
            auto *shortcut = new QShortcut(QKeySequence::fromString(sequence, QKeySequence::PortableText), window_);
            const bool quit = action == QStringLiteral("QuitApplication");
            // Quit works from every window, including Notices; workspace
            // actions belong to the main window only.
            shortcut->setContext(quit ? Qt::ApplicationShortcut : Qt::WindowShortcut);
            shortcut->setProperty("quit", quit);
            QObject::connect(shortcut, &QShortcut::activated, window_, handler);
            shortcuts_.push_back(shortcut);
        }
        apply();
    }
    void setEditing(bool editing) {
        editing_ = editing;
        apply();
    }

  private:
    void apply() {
        for (auto *shortcut : shortcuts_)
            shortcut->setEnabled(!editing_ || shortcut->property("quit").toBool());
    }

    QWidget *window_;
    WorkspaceBridge *bridge_;
    QHash<QString, Handler> handlers_;
    std::vector<QShortcut *> shortcuts_;
    bool editing_ = false;
};

/// Refreshes every binder after any call that may have changed the session.
/// A refresh that triggers another call refreshes again instead of nesting.
class RefreshCoordinator final {
  public:
    RefreshCoordinator(FolderItemsListModel *left, FolderItemsListModel *right, WorkspaceBridge *bridge) : left_(left), right_(right), bridge_(bridge) {}
    void refresh() {
        if (refreshing_) {
            again_ = true;
            return;
        }
        refreshing_ = true;
        again_ = true;
        while (again_) {
            again_ = false;
            left_->refresh();
            right_->refresh();
            bridge_->refresh();
        }
        refreshing_ = false;
    }

  private:
    FolderItemsListModel *left_;
    FolderItemsListModel *right_;
    WorkspaceBridge *bridge_;
    bool refreshing_ = false;
    bool again_ = false;
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
auto scheduler_state() -> SchedulerState & {
    static SchedulerState state;
    return state;
}
auto has_run() -> std::atomic_bool & {
    static std::atomic_bool value = false;
    return value;
}

/// Whether `widget` is `ancestor` or inside it.
auto inside(const QWidget *widget, const QWidget *ancestor) -> bool {
    return widget != nullptr && (widget == ancestor || ancestor->isAncestorOf(widget));
}
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
    right_model.setRightBrowser();
    WorkspaceBridge bridge;
    RefreshCoordinator coordinator(&left_model, &right_model, &bridge);
    QObject::connect(&left_model, &FolderItemsListModel::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
    QObject::connect(&right_model, &FolderItemsListModel::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
    QObject::connect(&bridge, &WorkspaceBridge::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
    DrainScheduler scheduler(&bridge);
    IconLoader icons;

    ThinSplitter standard_layout(SplitterKind::Sidebar);
    auto *split = new ThinSplitter(SplitterKind::Browser);
    auto *left_browser = new Browser(&left_model, &icons, split);
    auto *right_browser = new Browser(&right_model, &icons, split);
    const std::array<Browser *, 2> browsers = {left_browser, right_browser};
    const std::array<FolderItemsListModel *, 2> models = {&left_model, &right_model};
    const auto focus_active_list = [&bridge, browsers] { browsers.at(bridge.getActiveBrowser() == 0 ? 0 : 1)->view()->focusList(); };
    auto *notices = new NoticesWindow(&bridge, &window);
    const auto show_notices = [notices] {
        notices->show();
        notices->raise();
        notices->activateWindow();
    };
    auto *sidebar = new Sidebar(&bridge, focus_active_list, show_notices, &standard_layout);
    split->addWidget(left_browser);
    split->addWidget(right_browser);
    standard_layout.addWidget(sidebar);
    standard_layout.addWidget(split);
    window.setCentralWidget(&standard_layout);
    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.resize(initial_window_width, initial_window_height);
    window.setStyleSheet(style_sheet());
    BrowserHighlightController browser_highlighter(&window, {left_browser->folder(), right_browser->folder()}, {left_browser->view(), right_browser->view()});
    QObject::connect(&bridge, &WorkspaceBridge::activeBrowserChanged, &window, [&bridge, &browser_highlighter] { browser_highlighter.activate(bridge.getActiveBrowser()); });
    QObject::connect(&bridge, &WorkspaceBridge::startupFailureChanged, &window, [&bridge, browsers] {
        for (auto *browser : browsers)
            browser->folder()->status()->setText(bridge.getStartupFailure());
    });
    // An actionable storage error opens Notices, even at launch, without
    // taking focus from browsing.
    QObject::connect(&bridge, &WorkspaceBridge::noticesOpenRequestsChanged, &window, [notices] {
        notices->setAttribute(Qt::WA_ShowWithoutActivating, !notices->isVisible());
        notices->show();
        notices->raise();
        notices->setAttribute(Qt::WA_ShowWithoutActivating, false);
    });

    const auto active_model = [&bridge, models] { return models.at(bridge.getActiveBrowser() == 0 ? 0 : 1); };
    QHash<QString, ShortcutBinder::Handler> handlers;
    handlers.insert(QStringLiteral("FocusOtherBrowser"), [&bridge, browsers] { browsers.at(bridge.getActiveBrowser() == 0 ? 1 : 0)->view()->focusList(); });
    handlers.insert(QStringLiteral("NavigateParent"), [active_model] { active_model()->goToParent(); });
    handlers.insert(QStringLiteral("CloseWindow"), [&window] { window.close(); });
    handlers.insert(QStringLiteral("QuitApplication"), [] { QApplication::quit(); });
    handlers.insert(QStringLiteral("NewTab"), [active_model] { active_model()->newTab(); });
    handlers.insert(QStringLiteral("CloseTab"), [active_model] { active_model()->closeActiveTab(); });
    handlers.insert(QStringLiteral("NavigateBack"), [active_model] { active_model()->goBack(); });
    handlers.insert(QStringLiteral("NavigateForward"), [active_model] { active_model()->goForward(); });
    handlers.insert(QStringLiteral("RefreshFolder"), [active_model] { active_model()->refreshFolder(); });
    const std::array<QString, 8> sorts = {QStringLiteral("SortByNameAscending"), QStringLiteral("SortByNameDescending"), QStringLiteral("SortByTypeAscending"), QStringLiteral("SortByTypeDescending"), QStringLiteral("SortByDateAscending"), QStringLiteral("SortByDateDescending"), QStringLiteral("SortBySizeAscending"), QStringLiteral("SortBySizeDescending")};
    for (int choice = 0; choice < static_cast<int>(sorts.size()); ++choice)
        handlers.insert(sorts.at(choice), [active_model, choice] { active_model()->setSort(choice); });
    ShortcutBinder shortcuts(&window, &bridge, handlers);
    QObject::connect(&bridge, &WorkspaceBridge::bindingsRevisionChanged, &window, [&shortcuts] { shortcuts.rebind(); });

    // Focus entering any control inside a Browser activates that Browser.
    QObject::connect(&app, &QApplication::focusChanged, &window, [browsers, models, &shortcuts](QWidget *, QWidget *now) {
        shortcuts.setEditing(now != nullptr && now->property("inlineEditor").toBool());
        for (int browser = 0; browser < 2; ++browser)
            if (inside(now, browsers.at(browser)))
                models.at(browser)->activateBrowser();
    });

    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = &scheduler;
    }
    bridge.start(std::move(startup));
    shortcuts.rebind();
    // The root volume's name labels root tabs; reading it may touch the
    // disk, so it happens off the GUI thread.
    std::unique_ptr<QThread> volume_reader(QThread::create([&bridge] {
        const auto name = QStorageInfo::root().displayName();
        QMetaObject::invokeMethod(&bridge, [&bridge, name] { bridge.setRootVolumeName(name); }, Qt::QueuedConnection);
    }));
    volume_reader->start(QThread::LowPriority);
    window.show();
    left_browser->view()->setFocus(Qt::OtherFocusReason);
    const auto result = QApplication::exec();
    // The lambdas above capture stack objects that are destroyed in reverse
    // order below; focus changes during that teardown must not reach them.
    QObject::disconnect(&app, nullptr, &window, nullptr);
    for (QObject *source : std::initializer_list<QObject *>{&bridge, &left_model, &right_model})
        QObject::disconnect(source, nullptr, &window, nullptr);
    volume_reader->wait();
    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = nullptr;
    }
    shutdown_desktop();
    return result;
}
} // namespace dual_pane_desktop
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
