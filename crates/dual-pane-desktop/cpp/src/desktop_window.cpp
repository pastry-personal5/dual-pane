#include "dual_pane_desktop/desktop_window.hpp"
#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual-pane-desktop/src/operations_bridge.cxxqt.h"
#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/keyboard_layout.hpp"
#include "dual_pane_desktop/notices_glyph.hpp"
#include "dual_pane_desktop/quick_look_preview.hpp"
#include "dual_pane_desktop/settings_glyph.hpp"

#include <QtCore/QCache>
#include <QtCore/QElapsedTimer>
#include <QtCore/QEvent>
#include <QtCore/QFileInfo>
#include <QtCore/QHash>
#include <QtCore/QItemSelection>
#include <QtCore/QItemSelectionModel>
#include <QtCore/QMetaObject>
#include <QtCore/QMimeData>
#include <QtCore/QPersistentModelIndex>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtCore/QSignalBlocker>
#include <QtCore/QStorageInfo>
#include <QtCore/QString>
#include <QtCore/QThread>
#include <QtCore/QTimer>
#include <QtCore/QVariant>
#include <QtGui/QAbstractFileIconProvider>
#include <QtGui/QAccessible>
#include <QtGui/QCloseEvent>
#include <QtGui/QColor>
#include <QtGui/QCursor>
#include <QtGui/QDrag>
#include <QtGui/QFocusEvent>
#include <QtGui/QFontDatabase>
#include <QtGui/QFontMetrics>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QInputMethodEvent>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QPaintEvent>
#include <QtGui/QPainter>
#include <QtGui/QPen>
#include <QtGui/QPixmap>
#include <QtGui/QScreen>
#include <QtGui/QShortcut>
#include <QtGui/QShowEvent>
#include <QtGui/QWindow>
#include <QtWidgets/QAbstractButton>
#include <QtWidgets/QAbstractItemView>
#include <QtWidgets/QApplication>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QDialog>
#include <QtWidgets/QFrame>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QHeaderView>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QListWidget>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QMenu>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QProgressBar>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QScrollArea>
#include <QtWidgets/QScrollBar>
#include <QtWidgets/QSizePolicy>
#include <QtWidgets/QSplitter>
#include <QtWidgets/QSplitterHandle>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QStyle>
#include <QtWidgets/QStyleOptionViewItem>
#include <QtWidgets/QStyledItemDelegate>
#include <QtWidgets/QToolButton>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>
#include <QtWidgets/QWidgetAction>

#include <algorithm>
#include <array>
#include <atomic>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iostream>
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

/// Moves a saved logical-point frame fully onto the display it overlaps most,
/// shrinking only when that display cannot contain it.
auto fit_saved_frame(QRect frame) -> QRect {
    QScreen *screen = QGuiApplication::primaryScreen();
    qint64 best_overlap = 0;
    for (QScreen *candidate : QGuiApplication::screens()) {
        const QRect overlap = frame.intersected(candidate->availableGeometry());
        const qint64 area = static_cast<qint64>(overlap.width()) * overlap.height();
        if (area > best_overlap) {
            best_overlap = area;
            screen = candidate;
        }
    }
    if (screen == nullptr)
        return frame;
    const QRect available = screen->availableGeometry();
    frame.setWidth(qMin(frame.width(), available.width()));
    frame.setHeight(qMin(frame.height(), available.height()));
    frame.moveLeft(qBound(available.left(), frame.left(), available.right() - frame.width() + 1));
    frame.moveTop(qBound(available.top(), frame.top(), available.bottom() - frame.height() + 1));
    return frame;
}
constexpr int splitter_width = 3;
constexpr int narrow_browser_width = 280;
constexpr int column_count = 6;
constexpr int icon_column_width = 18;
constexpr int relative_date_column_width = 44;
constexpr int type_column_width = 44;
constexpr int path_role = Qt::UserRole;
constexpr int native_path_role = Qt::UserRole + 2;
constexpr int native_directory_role = Qt::UserRole + 3;
constexpr int relative_age_role = Qt::UserRole + 1;
constexpr int max_pending_icons = 128;
constexpr int icon_cache_size = 1024;
constexpr int tab_limit = 8;
constexpr auto favorite_item_mime = "application/x-dual-pane-favorite-item";
constexpr auto operation_panel_mime = "application/x-dual-pane-operation-panel";
// File command codes of the Folder Items model.
constexpr int command_copy = 0, command_move = 1, command_rename = 2, command_new_folder = 3, command_trash = 4, command_delete = 5;
// Shortcut scope codes of the workspace bridge.
constexpr int scope_folder_items_list = 0, scope_application = 2;
constexpr int status_reason_milliseconds = 3000;
constexpr int quick_look_hold_milliseconds = 256;
constexpr auto window_color = "#1B1D21", surface_color = "#23262B", inactive_surface_color = "#17191D", text_color = "#ECEFF3", inactive_text_color = "#8A9099", border_color = "#3A4048", active_color = "#2F6D9A", inactive_browser_color = "#173A55", hover_color = "#2B3037", favorite_group_color = "#15171A", divider_hover_border_color = "#737A84", error_color = "#E5737A";
constexpr int favorite_group_margin = 8;
constexpr int favorite_row_horizontal_padding = 8;
constexpr int favorite_row_vertical_padding = 4;

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
      QLabel#favoriteError, QLabel#favoriteCue { color:%8; background:transparent; }
      QFrame#folderPane { background:%1; border:1px solid %4; }
      QFrame#folderPane[browserActive="true"][windowActive="true"] { background:%2; border:1px solid %4; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] { background:%9; border:1px solid %4; }
      QFrame#folderPane[browserActive="true"][windowActive="true"] QTreeView { background:%2; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] QTreeView { background:%9; color:%10; }
      QTreeView { background:%2; color:%3; border:0; outline:none; }
      QTreeView::item { padding-left:0; }
      QTreeView::item:selected { background:%6; color:white; }
      QTreeView[browserActive="true"][windowActive="true"]::item:selected { background:%5; color:white; }
      QTreeView::item:hover:!selected { background:%11; }
      QToolButton { background:%2; color:%3; border:1px solid %4; padding:4px; }
      QToolButton:disabled { color:%7; }
      QToolButton#backButton:hover:enabled, QToolButton#forwardButton:hover:enabled, QToolButton#upButton:hover:enabled, QToolButton#settings:hover:enabled, QToolButton#noticesButton:hover:enabled { background:%5; }
      QToolButton#favoriteGroupMenuButton, QToolButton#addFavoriteItemButton { color:#A7ADB5; background:#24282E; }
      QToolButton#favoriteGroupMenuButton:hover:enabled, QToolButton#addFavoriteItemButton:hover:enabled { color:#FFFFFF; background:#2F6D9A; }
      QToolButton#favoriteGroupMenuButton:pressed:enabled, QToolButton#addFavoriteItemButton:pressed:enabled { color:#FFFFFF; background:#245878; }
      QToolButton#favoriteGroupMenuButton:disabled, QToolButton#addFavoriteItemButton:disabled { color:#6F757D; background:transparent; }
      QWidget#sidebar[windowActive="false"] QToolButton#favoriteGroupMenuButton, QWidget#sidebar[windowActive="false"] QToolButton#addFavoriteItemButton { color:#8A9099; background:#1B1D21; }
      QLabel#applicationTitle { background:transparent; color:%3; font-size:24px; font-weight:600; padding:10px 16px; }
      QToolButton#sortControl:hover:enabled:!checked, QToolButton#newTabButton:hover:enabled, QToolButton#closeTabButton:hover:enabled, QToolButton#newGroupButton:hover:enabled, QFrame#operationPanel QToolButton:hover:enabled { background:%11; }
      QLabel#folderPaneToolbarRow1, QWidget#folderPaneToolbarRow2, QWidget#folderPaneToolbarRow3, QWidget#folderPaneToolbarRow2 QLabel, QWidget#folderPaneToolbarRow2 QLineEdit, QWidget#folderPaneToolbarRow2 QToolButton, QWidget#folderPaneToolbarRow3 QLabel, QWidget#folderPaneToolbarRow3 QLineEdit, QWidget#folderPaneToolbarRow3 QToolButton { background:%1; }
      QLabel#folderPaneToolbarRow1[browserActive="true"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"], QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow2[browserActive="true"][windowActive="true"] QToolButton, QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow3[browserActive="true"][windowActive="true"] QToolButton { background:%2; }
      QLabel#folderPaneToolbarRow1[browserActive="false"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"], QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"], QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow2[browserActive="false"][windowActive="true"] QToolButton, QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"] QLabel, QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"] QLineEdit, QWidget#folderPaneToolbarRow3[browserActive="false"][windowActive="true"] QToolButton { background:%9; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] QLabel, QFrame#folderPane[browserActive="false"][windowActive="true"] QLineEdit { background:%9; color:%10; }
      QToolButton#sortControl { min-width:16px; max-width:16px; min-height:16px; max-height:16px; padding:0; background:%1; color:#A7ADB5; }
      QToolButton#sortControl:checked { color:#FFFFFF; }
      QToolButton#upButton, QToolButton#backButton, QToolButton#forwardButton, QToolButton#sortControl, QToolButton#newTabButton, QToolButton#closeTabButton, QToolButton#favoriteGroupMenuButton, QToolButton#addFavoriteItemButton, QToolButton#newGroupButton { border:none; }
      QWidget#browserTabsStrip { background:%1; }
      QWidget#browser[browserActive="true"][windowActive="true"], QWidget#browserTabsStrip[browserActive="true"][windowActive="true"], QLineEdit#pathEditControl[browserActive="true"][windowActive="true"] { background:%2; }
      QWidget#browser[browserActive="false"][windowActive="true"], QLineEdit#pathEditControl[browserActive="false"][windowActive="true"] { background:%9; color:%10; }
      QWidget#browserTabsStrip[browserActive="false"][windowActive="true"] { background:%9; }
      QWidget#previewPane { background:%1; color:%3; }
      QWidget#previewPane[browserActive="true"][windowActive="true"] { background:%2; color:%3; }
      QWidget#previewPane[browserActive="false"][windowActive="true"] { background:%9; color:%10; }
      QWidget#previewPane[browserActive="false"][windowActive="true"] QLabel { background:%9; color:%10; }
      QFrame#favoriteGroupBox { background:%12; border:0; border-radius:6px; }
      QWidget#favoriteGroup, QLabel#favoriteGroupName, QLabel#favoriteItem { background:transparent; border:0; border-radius:0; }
      QLabel#favoriteGroupName { font-size:13px; font-weight:bold; }
      QLabel#favoriteItem { font-size:13px; }
      QLineEdit#favoriteItemAliasEditor, QLineEdit#favoriteGroupNameEditor { background:%2; border:1px solid %5; border-radius:0; }
      QToolButton#newGroupButton { padding-left:8px; text-align:left; }
      QToolButton#settings, QToolButton#noticesButton { min-width:24px; max-width:24px; min-height:24px; max-height:24px; margin:0; padding:0; border:0; }
      QWidget#mainToolbar { background:transparent; border:0; }
      QLabel#expandOverlay { background:rgba(0, 0, 0, 160); color:white; }
      QTreeView#folderItemsList[missingFolder="true"] { color:#6F757D; }
      QLabel#missingFolderOverlay { background:transparent; color:white; }
      QLabel#nameEditorError { color:%8; background:%1; }
      QWidget#newFolderRow { background:%2; }
      QWidget#operationPanelStrip { background:%1; border-top:1px solid %4; }
      QFrame#operationPanel, QFrame#operationPanel QLabel, QFrame#operationPanel QCheckBox { background:%2; color:%3; }
      QFrame#operationPanel { border:1px solid %4; }
      QFrame#operationPanel QPushButton { background:%1; color:%3; border:1px solid %4; border-radius:4px; padding:4px 8px; }
      QFrame#operationPanel QPushButton:hover:enabled { background:%11; }
      QFrame#operationPanel QPushButton:disabled { color:%7; }
      QProgressBar#operationProgressIndicator { background:%1; border:1px solid %4; border-radius:3px; min-height:6px; max-height:6px; }
      QProgressBar#operationProgressIndicator::chunk { background:%5; border-radius:2px; }
      QFrame#operationDecisionCard { background:%1; border:1px solid %8; }
      QFrame#operationDecisionCard QLabel, QFrame#operationDecisionCard QCheckBox { background:%1; color:%3; }
      QFrame#operationDecisionCard QPushButton { background:%2; color:%3; }
      QWidget#floatingOperationPanel { background:%1; color:%3; }
      QLabel#operationPanelTitle { font-weight:bold; }
      QDialog#settingsWindow { background:%1; color:%3; }
      QWidget#settingsTitleRow { background:%1; border-bottom:1px solid %4; }
      QWidget#settingsSidebar { background:%1; border-right:1px solid %4; }
      QLabel#settingsHeading { background:transparent; color:%3; }
      QListWidget#settingsCategoryList { background:%1; color:%3; border:0; outline:none; padding:8px; }
      QListWidget#settingsCategoryList::item { padding:10px 12px; margin:2px 0; border-radius:4px; }
      QListWidget#settingsCategoryList::item:hover:!selected { background:%11; }
      QListWidget#settingsCategoryList::item:selected { background:%5; color:white; }
      QWidget#settingsContent, QStackedWidget#settingsPages { background:%2; border:0; }
      QWidget#settingsPage { background:%2; color:%3; }
      QFrame#settingsCard { background:%1; border:1px solid %4; border-radius:6px; }
      QLabel#settingsCardHeading { background:transparent; color:%3; font-size:14px; font-weight:600; }
      QLabel#settingsCardDescription { background:transparent; color:#B7C0CA; }
      QWidget#settingsShortcutPage { background:%2; color:%3; }
      QWidget#settingsShortcutPage QLabel, QWidget#settingsShortcutPage QToolButton, QWidget#settingsShortcutPage QPushButton { color:%3; font-size:10px; }
      QWidget#shortcutTableHeader { background:%1; border-bottom:1px solid %4; }
      QLabel#shortcutColumnHeading { background:transparent; color:#B7C0CA; font-size:10px; font-weight:600; }
      QWidget#settingsShortcutList, QScrollArea#settingsShortcutScroll QWidget#qt_scrollarea_viewport { background:%2; color:%3; }
      QFrame#shortcutGroup { background:%2; border:1px solid %4; border-radius:4px; }
      QLabel#shortcutGroupHeading { background:%1; color:#B7C0CA; border-bottom:1px solid %4; padding:7px 10px; font-size:11px; font-weight:600; }
      QWidget#shortcutRow { background:%2; border-bottom:1px solid %4; }
      QWidget#shortcutRow[alternate="true"] { background:#272B31; }
      QWidget#shortcutRow:hover { background:%11; }
      QWidget#shortcutRow QWidget, QWidget#shortcutRow QLabel { background:transparent; border:0; color:%3; }
      QLabel#shortcutScope, QLabel#shortcutEmpty { color:#B7C0CA; }
      QWidget#shortcutRow QLabel#shortcutOriginal { color:#9CA5AF; font-size:9px; }
      QWidget#shortcutRow QToolButton { background:transparent; color:#9CA5AF; border:0; padding:0; }
      QWidget#shortcutRow QToolButton:hover:enabled, QWidget#shortcutRow QToolButton:focus { background:transparent; color:#A9D8FA; border:0; }
      QLabel#shortcutFeedback { background:%2; color:%8; font-size:10px; padding:2px 8px 4px; }
      QLabel#shortcutFeedback[capturePrompt="true"] { color:#A9D8FA; }
      QScrollArea#settingsShortcutScroll { background:%2; border:0; }
      QScrollArea#settingsShortcutScroll QScrollBar { background:%2; border:0; }
      QScrollArea#settingsShortcutScroll QScrollBar::handle { background:%4; border-radius:4px; min-height:24px; }
      QScrollArea#settingsShortcutScroll QScrollBar::add-line, QScrollArea#settingsShortcutScroll QScrollBar::sub-line { width:0; height:0; }
      QDialog#settingsWindow QCheckBox, QDialog#settingsWindow QPushButton { color:%3; }
      QDialog#settingsWindow QPushButton { background:%2; border:1px solid %4; border-radius:4px; padding:6px 12px; }
      QDialog#settingsWindow QPushButton:hover:enabled { background:%11; }
      QDialog#settingsWindow QPushButton:focus { border-color:%5; }
      QMenu#settingsInfoMenu { background:%1; color:%3; border:1px solid %4; }
      QMenu#settingsInfoMenu QLabel { background:transparent; color:%3; font-size:12px; }
      QMessageBox#settingsResetConfirmation { background:%1; color:%3; }
      QMessageBox#settingsResetConfirmation QLabel { background:transparent; color:%3; }
      QMessageBox#settingsResetConfirmation QPushButton { background:%2; color:%3; border:1px solid %4; border-radius:4px; padding:6px 12px; }
      QToolButton#settingsCloseButton { background:%2; border:1px solid %4; border-radius:4px; padding:0; }
      QToolButton#settingsCloseButton:hover:enabled { background:%5; }
      QToolButton#settingsCloseButton:focus { border-color:%5; }
      QSplitter::handle { background:%4; }
    )")
        .arg(window_color)
        .arg(surface_color)
        .arg(text_color)
        .arg(border_color)
        .arg(active_color)
        .arg(inactive_browser_color)
        .arg(divider_hover_border_color)
        .arg(error_color)
        .arg(inactive_surface_color)
        .arg(inactive_text_color)
        .arg(hover_color)
        .arg(favorite_group_color);
}

void repolish(QWidget *widget) {
    widget->style()->unpolish(widget);
    widget->style()->polish(widget);
    widget->update();
}

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
    /// for empty space when `row` is -1.
    void setMenuHandler(std::function<void(const QPoint &, int)> handler) { menu_handler_ = std::move(handler); }
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
    }
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
            if (menu_handler_)
                menu_handler_(event->globalPosition().toPoint(), index.isValid() ? index.row() : -1);
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
    std::function<void(const QPoint &, int)> menu_handler_;
    std::function<void()> quick_look_failure_handler_;
    std::function<void(const QString &)> type_to_filter_handler_;
    std::function<void(const QString &, const QString &, const QList<QInputMethodEvent::Attribute> &)> input_method_handler_;
    std::unique_ptr<QuickLookPreviewController> quick_look_;
    QuickLookGesture quick_look_gesture_;
    QPointer<QTimer> quick_look_timer_;
    std::uint64_t quick_look_tab_ = 0;
    std::uint64_t quick_look_revision_ = 0;
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

enum class NavigationIcon : std::uint8_t { Back,
                                           Forward,
                                           Up,
};

auto make_navigation_icon(const NavigationIcon direction, const bool focused) -> QIcon {
    auto icon = QIcon();
    const auto draw = [direction](const QColor &color, const qreal scale) {
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
        if (direction == NavigationIcon::Up) {
            painter.drawLine(QPointF(8, 12), QPointF(8, 4));
            painter.drawLine(QPointF(4.5, 7.5), QPointF(8, 4));
            painter.drawLine(QPointF(8, 4), QPointF(11.5, 7.5));
        } else {
            const bool back = direction == NavigationIcon::Back;
            const qreal point = back ? 4.0 : 12.0;
            const qreal tail = back ? 12.0 : 4.0;
            painter.drawLine(QPointF(tail, 8), QPointF(point, 8));
            painter.drawLine(QPointF(back ? 7.5 : 8.5, 4.5), QPointF(point, 8));
            painter.drawLine(QPointF(point, 8), QPointF(back ? 7.5 : 8.5, 11.5));
        }
        return pixmap;
    };
    const auto add_variants = [&icon, &draw](const QColor &color, const QIcon::Mode mode) {
        icon.addPixmap(draw(color, 1.0), mode, QIcon::Off);
        icon.addPixmap(draw(color, 2.0), mode, QIcon::Off);
    };
    add_variants(QColor(QString::fromLatin1(focused ? text_color : inactive_text_color)), QIcon::Normal);
    add_variants(QColor(QStringLiteral("#6F757D")), QIcon::Disabled);
    return icon;
}

auto navigation_icon(const NavigationIcon direction, const bool focused) -> const QIcon & {
    static const std::array<QIcon, 3> active = {make_navigation_icon(NavigationIcon::Back, true), make_navigation_icon(NavigationIcon::Forward, true), make_navigation_icon(NavigationIcon::Up, true)};
    static const std::array<QIcon, 3> inactive = {make_navigation_icon(NavigationIcon::Back, false), make_navigation_icon(NavigationIcon::Forward, false), make_navigation_icon(NavigationIcon::Up, false)};
    return (focused ? active : inactive).at(static_cast<std::size_t>(direction));
}

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
        filter_input_->setProperty("filterTextEditor", true);
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
        previous_filter_tab_ = model->activeTabId();
        const auto refresh_filter = [this, model] {
            const auto tab = model->activeTabId();
            const bool tab_changed = tab != previous_filter_tab_;
            previous_filter_tab_ = tab;
            const auto &text = model->getFilterText();
            {
                const QSignalBlocker blocker(filter_input_);
                filter_input_->setText(text);
            }
            if (!text.isEmpty())
                filter_row_->show();
            else if (tab_changed)
                filter_row_->hide();
            if (model->getFilterStatus() == 1)
                filter_status_->setText(QStringLiteral("Updating…"));
            else if (model->getFilterStatus() == 2)
                filter_status_->setText(QStringLiteral("Filter unavailable; edit to retry"));
            else if (model->getNoFilterMatches())
                filter_status_->setText(QStringLiteral("No matches"));
            else if (text.isEmpty())
                filter_status_->setText(QStringLiteral("%1 items").arg(model->getFilterResultCount()));
            else
                filter_status_->setText(QStringLiteral("%1 matches").arg(model->getFilterResultCount()));
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
            const auto observed_tab = model->activeTabId();
            model->editFilterText(observed_tab, text);
        });
        QObject::connect(close_filter, &QToolButton::clicked, this, [this, model] {
            const auto observed_tab = model->activeTabId();
            const QString empty;
            model->editFilterText(observed_tab, empty);
            filter_row_->hide();
            view_->focusList();
        });
        filter_input_->setFocusLossHandler([this, model] {
            if (model->getFilterText().isEmpty() && !filter_row_->isAncestorOf(QApplication::focusWidget()))
                filter_row_->hide();
        });
        filter_input_->setEscapeHandler([this, model] {
            const auto observed_tab = model->activeTabId();
            const QString empty;
            model->editFilterText(observed_tab, empty);
            filter_row_->hide();
            view_->focusList();
        });
        view_->setTypeToFilterHandler([this, model](const QString &text) {
            const auto observed_tab = model->activeTabId();
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
    std::uint64_t previous_filter_tab_ = 0;
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
        setProperty("inlineEditor", true);
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

class Browser final : public QWidget {
  public:
    Browser(FolderItemsListModel *model, IconLoader *icons, QWidget *parent) : QWidget(parent), model_(model), folder_(new FolderPane(model, icons, this)), preview_splitter_(new ThinSplitter(SplitterKind::Preview, this)), preview_(new PreviewPane(preview_splitter_)), overlay_(new QLabel(QStringLiteral("Expand"), this)), missing_(new QLabel(QStringLiteral("Command+R to Refresh"), this)), editors_(new NameEditController(model, folder_->view(), folder_->new_folder_row())), preview_settle_(this) {
        editors_->setParent(this);
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
        QObject::connect(model, &FolderItemsListModel::statusTokenChanged, this, [model] {
            const int token = model->getStatusToken();
            QTimer::singleShot(status_reason_milliseconds, model, [model, token] { model->expireStatus(token); });
        });
        folder_->view()->setMenuHandler([this](const QPoint &at, int row) { show_menu(at, row); });
    }

    [[nodiscard]] auto view() const -> FolderItemsList * { return folder_->view(); }
    [[nodiscard]] auto folder() const -> FolderPane * { return folder_; }
    [[nodiscard]] auto editors() const -> NameEditController * { return editors_; }
    void focus_filter() { folder_->focus_filter(); }
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
    /// The Folder Items Context Menu. Items the application refuses now stay
    /// disabled; empty space offers New Folder.
    void show_menu(const QPoint &at, int row) {
        QMenu menu(this);
        const auto observed_tab = model_->activeTabId();
        const auto observed_revision = model_->visibleRevision();
        menu.setObjectName(QStringLiteral("folderItemsContextMenu"));
        menu.setAccessibleName(QStringLiteral("Folder Items Context Menu"));
        const auto add = [this, &menu, observed_tab, observed_revision](const QString &text, int command) {
            auto *action = menu.addAction(text);
            action->setEnabled(model_->fileCommandAvailable(command));
            QObject::connect(action, &QAction::triggered, this, [this, command, observed_tab, observed_revision] { model_->startFileCommandObserved(command, observed_tab, observed_revision); });
        };
        if (row >= 0) {
            add(QStringLiteral("Copy to Other Browser"), command_copy);
            add(QStringLiteral("Move to Other Browser"), command_move);
            add(QStringLiteral("Rename Item"), command_rename);
            menu.addSeparator();
            add(QStringLiteral("New Folder"), command_new_folder);
            menu.addSeparator();
            add(QStringLiteral("Move to Trash"), command_trash);
            add(QStringLiteral("Delete Permanently"), command_delete);
            if (model_->rowIsPackage(row)) {
                menu.addSeparator();
                QObject::connect(menu.addAction(QStringLiteral("Show Package Contents")), &QAction::triggered, this, [this, row, observed_tab, observed_revision] { model_->showPackageContentsObserved(row, observed_tab, observed_revision); });
            }
        } else {
            add(QStringLiteral("New Folder"), command_new_folder);
        }
        menu.exec(at);
    }

    FolderItemsListModel *model_;
    FolderPane *folder_;
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
        setIndent(0);
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
        if (hasFocus()) {
            QPainter painter(this);
            painter.setPen(QColor(QString::fromLatin1(active_color)));
            painter.drawRect(rect().adjusted(0, 0, -1, -1));
        }
    }

  private:
    std::int64_t id_;
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
                    add_editor(holder_layout, holder, QStringLiteral("favoriteItemAliasEditor"), QStringLiteral("Favorite Item Alias Editor"), alias, [this, item_id](const QString &text) { return bridge_->renameItem(item_id, text); });
                    box_layout->addWidget(holder);
                    continue;
                }
                auto *row_widget = new FavoriteItemRow(item_id, alias, bridge_->itemPath(group, item), box);
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

enum class ShortcutGlyph : std::uint8_t { Info,
                                          Add,
                                          Remove,
                                          Restore,
};

auto shortcut_glyph_bitmap(ShortcutGlyph glyph, const QColor &foreground) -> QPixmap {
    constexpr int logical_size = 18;
    constexpr qreal bitmap_scale = 3.0;
    QPixmap bitmap(logical_size * static_cast<int>(bitmap_scale), logical_size * static_cast<int>(bitmap_scale));
    bitmap.setDevicePixelRatio(bitmap_scale);
    bitmap.fill(Qt::transparent);
    QPainter painter(&bitmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(foreground, 1.7, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    switch (glyph) {
    case ShortcutGlyph::Info: {
        painter.drawEllipse(QRectF(2, 2, 14, 14));
        QFont font = painter.font();
        font.setPointSize(10);
        font.setBold(true);
        painter.setFont(font);
        painter.drawText(QRect(0, 0, logical_size, logical_size), Qt::AlignCenter, QStringLiteral("i"));
        break;
    }
    case ShortcutGlyph::Add:
        painter.drawLine(QPointF(9, 3.5), QPointF(9, 14.5));
        painter.drawLine(QPointF(3.5, 9), QPointF(14.5, 9));
        break;
    case ShortcutGlyph::Remove:
        painter.drawLine(QPointF(4.5, 4.5), QPointF(13.5, 13.5));
        painter.drawLine(QPointF(13.5, 4.5), QPointF(4.5, 13.5));
        break;
    case ShortcutGlyph::Restore:
        painter.drawArc(QRectF(3.5, 3.5, 11, 11), 35 * 16, 290 * 16);
        painter.drawLine(QPointF(13.3, 4.2), QPointF(14.3, 7.2));
        painter.drawLine(QPointF(14.3, 7.2), QPointF(11.2, 6.7));
        break;
    }
    return bitmap;
}

class ShortcutGlyphButton : public QToolButton {
  public:
    ShortcutGlyphButton(QWidget *parent, ShortcutGlyph glyph)
        : QToolButton(parent), normal_icon_(shortcut_glyph_bitmap(glyph, QColor(156, 165, 175))), active_icon_(shortcut_glyph_bitmap(glyph, QColor(169, 216, 250))) {
        setObjectName(QStringLiteral("shortcutGlyphButton"));
        setIcon(normal_icon_);
        setIconSize(QSize(14, 14));
        setFixedSize(18, 18);
        setToolButtonStyle(Qt::ToolButtonIconOnly);
        setFocusPolicy(Qt::StrongFocus);
        setCursor(Qt::PointingHandCursor);
    }

  protected:
    bool event(QEvent *event) override {
        if (event->type() == QEvent::Enter)
            hovering_ = true;
        else if (event->type() == QEvent::Leave)
            hovering_ = false;
        const bool handled = QToolButton::event(event);
        if (event->type() == QEvent::Enter || event->type() == QEvent::Leave || event->type() == QEvent::FocusIn || event->type() == QEvent::FocusOut)
            setIcon(hovering_ || hasFocus() ? active_icon_ : normal_icon_);
        return handled;
    }

  private:
    QIcon normal_icon_;
    QIcon active_icon_;
    bool hovering_ = false;
};

/// A focused capture consumes shortcut events before the workspace can act.
class ShortcutCapture final : public ShortcutGlyphButton {
  public:
    ShortcutCapture(QWidget *parent, std::function<QString(int, int)> submit, std::function<void(bool)> active, std::function<void(QWidget *)> reveal, QLabel *feedback)
        : ShortcutGlyphButton(parent, ShortcutGlyph::Add), submit_(std::move(submit)), active_(std::move(active)), reveal_(std::move(reveal)), feedback_(feedback) {
        setToolTip(QStringLiteral("Add shortcut"));
        setAccessibleName(QStringLiteral("Add shortcut"));
        QObject::connect(this, &QToolButton::clicked, this, [this] {
            capturing_ = true;
            setAccessibleDescription(QStringLiteral("Press one supported key with Command, Control, or Option. Function keys may stand alone. Escape cancels; Tab moves focus."));
            feedback_->setProperty("capturePrompt", true);
            feedback_->setText(QStringLiteral("Press shortcut…"));
            feedback_->style()->unpolish(feedback_);
            feedback_->style()->polish(feedback_);
            feedback_->show();
            active_(true);
            setFocus();
        });
    }
    void finish() {
        if (!capturing_)
            return;
        capturing_ = false;
        if (feedback_->property("capturePrompt").toBool()) {
            feedback_->clear();
            feedback_->hide();
        }
        active_(false);
    }

  protected:
    bool event(QEvent *event) override {
        if (capturing_ && event->type() == QEvent::ShortcutOverride) {
            event->accept();
            return true;
        }
        return ShortcutGlyphButton::event(event);
    }
    void keyPressEvent(QKeyEvent *event) override {
        if (!capturing_) {
            ShortcutGlyphButton::keyPressEvent(event);
            return;
        }
        const auto held = event->modifiers();
        if (held == Qt::NoModifier && event->key() == Qt::Key_Escape) {
            finish();
            event->accept();
            return;
        }
        if (held == Qt::NoModifier && event->key() == Qt::Key_Tab) {
            finish();
            focusNextChild();
            event->accept();
            return;
        }
        const unsigned flags = (held.testFlag(Qt::MetaModifier) ? 1U : 0U) | (held.testFlag(Qt::ControlModifier) ? 2U : 0U) | (held.testFlag(Qt::AltModifier) ? 4U : 0U) | (held.testFlag(Qt::ShiftModifier) ? 8U : 0U);
        int key = event->key();
        if (key < Qt::Key_Escape) {
            const int unshifted = unshifted_key_character(event->nativeVirtualKey());
            if (unshifted != 0)
                key = unshifted;
        }
        const QString error = submit_(key, static_cast<int>(flags));
        feedback_->setProperty("capturePrompt", false);
        feedback_->style()->unpolish(feedback_);
        feedback_->style()->polish(feedback_);
        feedback_->setText(error);
        feedback_->setVisible(!error.isEmpty());
        if (!error.isEmpty())
            reveal_(feedback_);
        if (error.isEmpty())
            finish();
        event->accept();
    }
    void focusOutEvent(QFocusEvent *event) override {
        finish();
        ShortcutGlyphButton::focusOutEvent(event);
    }

  private:
    std::function<QString(int, int)> submit_;
    std::function<void(bool)> active_;
    std::function<void(QWidget *)> reveal_;
    QLabel *feedback_;
    bool capturing_ = false;
};

class SettingsTitleRow final : public QWidget {
  public:
    explicit SettingsTitleRow(QWidget *parent) : QWidget(parent) { setCursor(Qt::OpenHandCursor); }

  protected:
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() != Qt::LeftButton) {
            QWidget::mousePressEvent(event);
            return;
        }
        if (auto *handle = window()->windowHandle(); handle && handle->startSystemMove()) {
            event->accept();
            return;
        }
        dragging_ = true;
        last_global_ = event->globalPosition().toPoint();
        setCursor(Qt::ClosedHandCursor);
        event->accept();
    }
    void mouseMoveEvent(QMouseEvent *event) override {
        if (dragging_ && (event->buttons() & Qt::LeftButton)) {
            const QPoint current = event->globalPosition().toPoint();
            window()->move(window()->pos() + current - last_global_);
            last_global_ = current;
            event->accept();
            return;
        }
        QWidget::mouseMoveEvent(event);
    }
    void mouseReleaseEvent(QMouseEvent *event) override {
        if (dragging_ && event->button() == Qt::LeftButton) {
            dragging_ = false;
            setCursor(Qt::OpenHandCursor);
            event->accept();
            return;
        }
        QWidget::mouseReleaseEvent(event);
    }

  private:
    QPoint last_global_;
    bool dragging_ = false;
};

class SettingsWindow final : public QDialog {
  public:
    explicit SettingsWindow(QWidget *owner, WorkspaceBridge *bridge = nullptr, std::function<void(bool)> capture = {})
        : QDialog(owner, Qt::Dialog | Qt::FramelessWindowHint), bridge_(bridge), capture_(std::move(capture)), categories_(new QListWidget(this)), pages_(new QStackedWidget(this)) {
        setObjectName(QStringLiteral("settingsWindow"));
        setWindowTitle(QStringLiteral("Settings"));
        setAccessibleName(QStringLiteral("Settings Window"));
        setWindowModality(Qt::ApplicationModal);
        QObject::connect(this, &QDialog::finished, this, [this] { if (capture_) capture_(false); deleteLater(); });
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        auto *title_row = new SettingsTitleRow(this);
        title_row->setObjectName(QStringLiteral("settingsTitleRow"));
        title_row->setFixedHeight(48);
        auto *title_layout = new QHBoxLayout(title_row);
        title_layout->setContentsMargins(24, 8, 24, 8);
        title_layout->setSpacing(0);
        auto *heading = new QLabel(QStringLiteral("Settings"), title_row);
        heading->setObjectName(QStringLiteral("settingsHeading"));
        heading->setAttribute(Qt::WA_TransparentForMouseEvents);
        QFont heading_font = heading->font();
        heading_font.setPointSize(20);
        heading_font.setBold(true);
        heading->setFont(heading_font);
        auto *close = new QToolButton(title_row);
        close->setObjectName(QStringLiteral("settingsCloseButton"));
        QPixmap close_bitmap(18, 18);
        close_bitmap.fill(Qt::transparent);
        QPainter close_painter(&close_bitmap);
        close_painter.setRenderHint(QPainter::Antialiasing);
        close_painter.setPen(QPen(QColor(225, 231, 237), 1.8, Qt::SolidLine, Qt::RoundCap));
        close_painter.drawLine(QPointF(4, 4), QPointF(14, 14));
        close_painter.drawLine(QPointF(14, 4), QPointF(4, 14));
        close_painter.end();
        close->setIcon(QIcon(close_bitmap));
        close->setIconSize(QSize(18, 18));
        close->setFixedSize(28, 28);
        close->setToolTip(QStringLiteral("Close Settings"));
        close->setAccessibleName(QStringLiteral("Close Settings Window"));
        close->setFocusPolicy(Qt::TabFocus);
        QObject::connect(close, &QToolButton::clicked, this, &QDialog::reject);
        title_layout->addWidget(close);
        title_layout->addSpacing(12);
        title_layout->addWidget(heading);
        title_layout->addStretch();
        layout->addWidget(title_row);
        auto *body = new QHBoxLayout();
        body->setContentsMargins(0, 0, 0, 0);
        body->setSpacing(0);
        auto *sidebar = new QWidget(this);
        sidebar->setObjectName(QStringLiteral("settingsSidebar"));
        auto *sidebar_layout = new QVBoxLayout(sidebar);
        sidebar_layout->setContentsMargins(0, 0, 0, 0);
        sidebar_layout->setSpacing(0);
        categories_->setObjectName(QStringLiteral("settingsCategoryList"));
        categories_->setAccessibleName(QStringLiteral("Settings Category List"));
        categories_->setMinimumWidth(320);
        categories_->addItem(QStringLiteral("General Settings"));
        categories_->addItem(QStringLiteral("Keyboard Shortcuts Settings"));
        categories_->setCurrentRow(0);
        sidebar_layout->addWidget(categories_, 1);
        pages_->setObjectName(QStringLiteral("settingsPages"));
        pages_->addWidget(general_page());
        pages_->addWidget(shortcut_page());
        QObject::connect(categories_, &QListWidget::currentRowChanged, pages_, &QStackedWidget::setCurrentIndex);
        auto *content = new QWidget(this);
        content->setObjectName(QStringLiteral("settingsContent"));
        auto *content_layout = new QVBoxLayout(content);
        content_layout->setContentsMargins(0, 0, 0, 0);
        content_layout->setSpacing(0);
        content_layout->addWidget(pages_, 1);
        body->addWidget(sidebar);
        body->addWidget(content, 1);
        layout->addLayout(body, 1);
        setMinimumSize(1400, 900);
        resize(1400, 900);
        categories_->setFocus();
        if (bridge_)
            QObject::connect(bridge_, &WorkspaceBridge::bindingsRevisionChanged, this, [this] { QTimer::singleShot(0, this, [this] { rebuild_shortcuts(); }); });
    }

  private:
    void showEvent(QShowEvent *event) override {
        QDialog::showEvent(event);
        categories_->setFocus();
    }
    static auto action_name(const QString &id) -> QString {
        QString name;
        for (const QChar ch : id) {
            if (!name.isEmpty() && ch.isUpper())
                name += QLatin1Char(' ');
            name += ch;
        }
        return name;
    }
    static auto shortcut_group(const QString &id) -> int {
        if (id == QStringLiteral("FocusOtherBrowser") || id == QStringLiteral("FocusFolderItemsFilter") || id == QStringLiteral("NavigateParent") || id == QStringLiteral("NavigateBack") || id == QStringLiteral("NavigateForward") || id == QStringLiteral("RefreshFolder"))
            return 0;
        if (id == QStringLiteral("NewTab") || id == QStringLiteral("CloseTab") || id == QStringLiteral("CloseWindow") || id == QStringLiteral("QuitApplication"))
            return 1;
        if (id == QStringLiteral("NewFolder") || id == QStringLiteral("CopyToOtherBrowser") || id == QStringLiteral("MoveToOtherBrowser") || id == QStringLiteral("RenameItem") || id == QStringLiteral("MoveToTrash") || id == QStringLiteral("DeletePermanently") || id == QStringLiteral("ShowPackageContents") || id == QStringLiteral("OpenItem"))
            return 2;
        if (id.startsWith(QStringLiteral("SortBy")))
            return 3;
        return 4;
    }
    auto info_button(QWidget *parent, const QString &subject, const QString &description, bool shortcut_style = false) -> QToolButton * {
        QToolButton *button = shortcut_style ? static_cast<QToolButton *>(new ShortcutGlyphButton(parent, ShortcutGlyph::Info)) : new QToolButton(parent);
        if (!shortcut_style) {
            QPixmap bitmap(18, 18);
            bitmap.fill(Qt::transparent);
            QPainter painter(&bitmap);
            painter.setRenderHint(QPainter::Antialiasing);
            painter.setPen(QPen(QColor(145, 185, 230), 1.5));
            painter.drawEllipse(QRectF(1.5, 1.5, 15, 15));
            painter.setFont(QFont(QStringLiteral("Helvetica"), 11, QFont::Bold));
            painter.drawText(bitmap.rect(), Qt::AlignCenter, QStringLiteral("i"));
            painter.end();
            button->setIcon(QIcon(bitmap));
            button->setIconSize(QSize(18, 18));
            button->setFixedSize(24, 24);
        }
        button->setToolTip(subject + QStringLiteral(" information"));
        button->setAccessibleName(subject + QStringLiteral(" information"));
        button->setFocusPolicy(Qt::StrongFocus);
        QObject::connect(button, &QToolButton::clicked, this, [button, description] {
            auto *menu = new QMenu(button);
            menu->setObjectName(QStringLiteral("settingsInfoMenu"));
            menu->setAttribute(Qt::WA_DeleteOnClose);
            menu->setAttribute(Qt::WA_StyledBackground);
            menu->setFocusPolicy(Qt::StrongFocus);
            menu->setStyleSheet(QStringLiteral("QMenu#settingsInfoMenu { background-color:#1D2229; color:#F4F6F8; border:1px solid #778392; padding:4px; }"));
            auto *label = new QLabel(description, menu);
            label->setObjectName(QStringLiteral("settingsInfoText"));
            label->setAccessibleName(description);
            label->setWordWrap(true);
            label->setMinimumWidth(300);
            label->setMaximumWidth(400);
            label->setContentsMargins(12, 12, 12, 12);
            label->setStyleSheet(QStringLiteral("QLabel#settingsInfoText { background-color:#2B323B; color:#F4F6F8; border:1px solid #56616E; border-radius:5px; padding:12px; font-size:13px; }"));
            auto *action = new QWidgetAction(menu);
            action->setDefaultWidget(label);
            menu->addAction(action);
            QObject::connect(menu, &QMenu::aboutToHide, button, [button] { QTimer::singleShot(0, button, [button] { button->setFocus(Qt::OtherFocusReason); }); });
            menu->popup(button->mapToGlobal(QPoint(0, button->height())));
            menu->setFocus();
        });
        return button;
    }
    auto general_page() -> QWidget * {
        auto *page = new QWidget(pages_);
        page->setObjectName(QStringLiteral("settingsPage"));
        page->setAccessibleName(QStringLiteral("General Settings"));
        auto *layout = new QVBoxLayout(page);
        layout->setContentsMargins(24, 20, 24, 20);
        layout->setSpacing(14);
        auto *title = new QLabel(QStringLiteral("General"), page);
        layout->addWidget(title);
        auto make_card = [page, layout](const QString &name, const QString &heading, const QString &description) {
            auto *card = new QFrame(page);
            card->setObjectName(QStringLiteral("settingsCard"));
            auto *card_layout = new QVBoxLayout(card);
            card_layout->setContentsMargins(16, 14, 16, 14);
            card_layout->setSpacing(8);
            auto *card_heading = new QLabel(heading, card);
            card_heading->setObjectName(QStringLiteral("settingsCardHeading"));
            card->setAccessibleName(name);
            card_layout->addWidget(card_heading);
            if (!description.isEmpty()) {
                auto *card_description = new QLabel(description, card);
                card_description->setObjectName(QStringLiteral("settingsCardDescription"));
                card_description->setWordWrap(true);
                card_layout->addWidget(card_description);
            }
            layout->addWidget(card);
            return card_layout;
        };
        auto *startup_card = make_card(QStringLiteral("Startup Notices"), QStringLiteral("Startup Notices"), QStringLiteral("Choose which notices open automatically when the app starts."));
        auto *notices = new QHBoxLayout();
        auto *check = new QCheckBox(QStringLiteral("Don’t show notices at startup"), page);
        check->setObjectName(QStringLiteral("settingsHideNotices"));
        check->setAccessibleName(QStringLiteral("Don’t show notices at startup"));
        check->setChecked(bridge_ && bridge_->getHideNoticesAtStartup());
        check->setEnabled(bridge_ != nullptr);
        if (bridge_) {
            QObject::connect(check, &QCheckBox::toggled, page, [this](bool value) { bridge_->changeHideNoticesAtStartup(value); });
            QObject::connect(bridge_, &WorkspaceBridge::hideNoticesAtStartupChanged, page, [this, check] { QSignalBlocker block(check); check->setChecked(bridge_->getHideNoticesAtStartup()); });
        }
        notices->addWidget(check);
        notices->addWidget(info_button(page, QStringLiteral("Startup notices"), QStringLiteral("When selected, routine notices stay in Notices at startup. Important errors still open Notices so you can act on them.")));
        notices->addStretch();
        startup_card->addLayout(notices);
        auto *visibility_card = make_card(QStringLiteral("Item Visibility"), QStringLiteral("Item Visibility"), QStringLiteral("Choose which hidden item classes appear in both Browsers."));
        auto *visibility_help = new QLabel(QStringLiteral("An item matching more than one rule appears only when every matching Show control is selected."), page);
        visibility_help->setObjectName(QStringLiteral("settingsVisibilityHelp"));
        visibility_help->setAccessibleName(QStringLiteral("Visibility settings help"));
        visibility_help->setWordWrap(true);
        visibility_card->addWidget(visibility_help);
        auto *macos_items = new QCheckBox(QStringLiteral("Show macOS metadata and service items"), page);
        auto *dotfiles = new QCheckBox(QStringLiteral("Show dotfiles"), page);
        auto *macos_invisible = new QCheckBox(QStringLiteral("Show items marked invisible by macOS"), page);
        macos_items->setObjectName(QStringLiteral("settingsShowMacosItems"));
        dotfiles->setObjectName(QStringLiteral("settingsShowDotfiles"));
        macos_invisible->setObjectName(QStringLiteral("settingsShowMacosInvisible"));
        macos_items->setAccessibleName(QStringLiteral("Show macOS metadata and service items"));
        dotfiles->setAccessibleName(QStringLiteral("Show dotfiles"));
        macos_invisible->setAccessibleName(QStringLiteral("Show items marked invisible by macOS"));
        const bool visibility_available = bridge_ != nullptr;
        for (auto *control : {macos_items, dotfiles, macos_invisible}) {
            control->setEnabled(visibility_available);
        }
        if (bridge_) {
            macos_items->setChecked(bridge_->getShowMacosItems());
            dotfiles->setChecked(bridge_->getShowDotfiles());
            macos_invisible->setChecked(bridge_->getShowMacosInvisible());
            auto submit_visibility = [this, macos_items, dotfiles, macos_invisible] { bridge_->changeVisibility(macos_items->isChecked(), dotfiles->isChecked(), macos_invisible->isChecked()); };
            QObject::connect(macos_items, &QCheckBox::toggled, page, submit_visibility);
            QObject::connect(dotfiles, &QCheckBox::toggled, page, submit_visibility);
            QObject::connect(macos_invisible, &QCheckBox::toggled, page, submit_visibility);
            auto synchronize_visibility = [this, macos_items, dotfiles, macos_invisible] {
                QSignalBlocker macos_block(macos_items);
                QSignalBlocker dot_block(dotfiles);
                QSignalBlocker invisible_block(macos_invisible);
                macos_items->setChecked(bridge_->getShowMacosItems());
                dotfiles->setChecked(bridge_->getShowDotfiles());
                macos_invisible->setChecked(bridge_->getShowMacosInvisible());
            };
            QObject::connect(bridge_, &WorkspaceBridge::showMacosItemsChanged, page, synchronize_visibility);
            QObject::connect(bridge_, &WorkspaceBridge::showDotfilesChanged, page, synchronize_visibility);
            QObject::connect(bridge_, &WorkspaceBridge::showMacosInvisibleChanged, page, synchronize_visibility);
        }
        visibility_card->addWidget(macos_items);
        visibility_card->addWidget(dotfiles);
        visibility_card->addWidget(macos_invisible);
        auto *reset_card = make_card(QStringLiteral("Reset Settings"), QStringLiteral("Reset Settings"), QStringLiteral("Restore defaults for settings, Favorites, open tabs, and workspace layout."));
        auto *reset_row = new QHBoxLayout();
        auto *reset = new QPushButton(QStringLiteral("Reset Settings"), page);
        reset->setObjectName(QStringLiteral("settingsReset"));
        reset->setAccessibleName(QStringLiteral("Reset Settings"));
        reset->setEnabled(bridge_ != nullptr);
        QObject::connect(reset, &QPushButton::clicked, this, [this] {
            auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Reset Settings"), QStringLiteral("Reset Dual Pane’s settings?"), QMessageBox::NoButton, this);
            box->setObjectName(QStringLiteral("settingsResetConfirmation"));
            box->setInformativeText(QStringLiteral("Your settings and Favorites, including changes made in this session, will be replaced with defaults. Open tabs will close and the workspace layout will return to its defaults. The current settings database is kept as a backup."));
            auto *confirm = box->addButton(QStringLiteral("Reset"), QMessageBox::DestructiveRole);
            auto *cancel = box->addButton(QMessageBox::Cancel);
            box->setDefaultButton(cancel);
            box->setEscapeButton(cancel);
            box->setWindowModality(Qt::WindowModal);
            box->setAttribute(Qt::WA_DeleteOnClose);
            QObject::connect(box, &QMessageBox::finished, this, [this, box, confirm] { if (box->clickedButton() == confirm && bridge_) { reject(); bridge_->resetSettings(); } });
            box->open();
        });
        reset_row->addWidget(reset);
        reset_row->addWidget(info_button(page, QStringLiteral("Reset Settings"), QStringLiteral("Reset restores all compiled settings and shortcuts, removes Favorites, closes open tabs, and returns the workspace layout to its defaults. The current database is kept as a backup.")));
        reset_row->addStretch();
        reset_card->addLayout(reset_row);
        layout->addStretch();
        return page;
    }
    static auto shortcut_table_header(QWidget *parent) -> QWidget * {
        auto *header = new QWidget(parent);
        header->setObjectName(QStringLiteral("shortcutTableHeader"));
        header->setFixedHeight(36);
        auto *head = new QHBoxLayout(header);
        head->setContentsMargins(8, 0, 8, 0);
        head->setSpacing(8);
        for (const auto &name : {QStringLiteral("Action"), QStringLiteral("Shortcuts"), QStringLiteral("Original Shortcuts"), QStringLiteral("Scope"), QStringLiteral("Add")}) {
            auto *label = new QLabel(name, header);
            label->setObjectName(QStringLiteral("shortcutColumnHeading"));
            int width = 36;
            if (name == QStringLiteral("Action"))
                width = 170;
            else if (name == QStringLiteral("Shortcuts"))
                width = shortcut_column_width();
            else if (name == QStringLiteral("Original Shortcuts"))
                width = 160;
            else if (name == QStringLiteral("Scope"))
                width = 120;
            label->setFixedWidth(width);
            head->addWidget(label);
        }
        head->addStretch();
        return header;
    }
    static auto shortcut_column_width() -> int {
        auto font = QApplication::font("QLabel");
        font.setPixelSize(10);
        const QFontMetrics metrics(font);
        const auto longest_supported_shortcut = QStringLiteral("Control+Option+Shift+Command+Backspace");
        return metrics.horizontalAdvance(longest_supported_shortcut) + 6 + 18;
    }
    static auto shortcut_table_width() -> int {
        constexpr int action_width = 170;
        constexpr int original_width = 160;
        constexpr int scope_width = 120;
        constexpr int add_width = 36;
        constexpr int column_gap = 8;
        constexpr int horizontal_padding = 8;
        constexpr int card_border = 2;
        constexpr int column_count = 5;
        return action_width + shortcut_column_width() + original_width + scope_width + add_width + ((column_count - 1) * column_gap) + (2 * horizontal_padding) + card_border;
    }
    auto shortcut_page() -> QWidget * {
        auto *page = new QWidget(pages_);
        page->setObjectName(QStringLiteral("settingsShortcutPage"));
        page->setAccessibleName(QStringLiteral("Keyboard Shortcuts Settings"));
        auto *layout = new QVBoxLayout(page);
        layout->setContentsMargins(16, 16, 16, 16);
        shortcut_scroll_ = new QScrollArea(page);
        shortcut_scroll_->setWidgetResizable(true);
        shortcut_scroll_->setObjectName(QStringLiteral("settingsShortcutScroll"));
        shortcut_list_ = new QWidget(shortcut_scroll_);
        shortcut_list_->setObjectName(QStringLiteral("settingsShortcutList"));
        shortcut_layout_ = new QVBoxLayout(shortcut_list_);
        shortcut_layout_->setContentsMargins(0, 0, 0, 0);
        shortcut_layout_->setSpacing(10);
        shortcut_scroll_->setWidget(shortcut_list_);
        layout->addWidget(shortcut_scroll_);
        rebuild_shortcuts();
        return page;
    }
    void rebuild_shortcuts() {
        if (!shortcut_layout_)
            return;
        feedback_by_action_.clear();
        while (auto *item = shortcut_layout_->takeAt(0)) {
            if (auto *widget = item->widget()) {
                widget->hide();
                widget->deleteLater();
            }
            delete item;
        }
        if (!bridge_) {
            shortcut_layout_->addWidget(new QLabel(QStringLiteral("Shortcuts are unavailable."), shortcut_list_));
            return;
        }
        const QStringList group_names{QStringLiteral("Navigation"), QStringLiteral("Tabs and Windows"), QStringLiteral("File Actions"), QStringLiteral("Sorting"), QStringLiteral("Other Actions")};
        for (int group_index = 0; group_index < group_names.size(); ++group_index) {
            auto *group = new QFrame(shortcut_list_);
            group->setObjectName(QStringLiteral("shortcutGroup"));
            group->setAccessibleName(group_names.at(group_index) + QStringLiteral(" shortcuts"));
            group->setFixedWidth(shortcut_table_width());
            auto *group_layout = new QVBoxLayout(group);
            group_layout->setContentsMargins(0, 0, 0, 0);
            group_layout->setSpacing(0);
            auto *group_heading = new QLabel(group_names.at(group_index), group);
            group_heading->setObjectName(QStringLiteral("shortcutGroupHeading"));
            group_layout->addWidget(group_heading);
            group_layout->addWidget(shortcut_table_header(group));
            int group_rows = 0;
            for (int index = 0; index < bridge_->actionCount(); ++index) {
                const QString id = bridge_->actionId(index);
                if (shortcut_group(id) != group_index)
                    continue;
                const QString name = action_name(id);
                auto *row = new QWidget(group);
                row->setObjectName(QStringLiteral("shortcutRow"));
                row->setProperty("alternate", group_rows % 2 != 0);
                ++group_rows;
                row->setAccessibleName(name + QStringLiteral(" shortcuts"));
                auto *cells = new QHBoxLayout(row);
                cells->setContentsMargins(8, 3, 8, 3);
                cells->setSpacing(8);
                auto *action_cell = new QWidget(row);
                action_cell->setFixedWidth(170);
                auto *action_layout = new QHBoxLayout(action_cell);
                action_layout->setContentsMargins(0, 0, 0, 0);
                auto *title = new QLabel(name, action_cell);
                title->setAccessibleName(name);
                action_layout->addWidget(title);
                action_layout->addStretch();
                QString scope = QStringLiteral("Workspace window");
                if (bridge_->actionScope(index) == 0)
                    scope = QStringLiteral("Folder Items List");
                else if (bridge_->actionScope(index) == 2)
                    scope = QStringLiteral("Application");
                const QString help = name + QStringLiteral(". This action is available in the ") + scope + QStringLiteral(". Add one or more shortcuts, remove individual entries, or restore its compiled default.");
                row->setAccessibleDescription(help);
                action_layout->addWidget(info_button(action_cell, name, help, true));
                cells->addWidget(action_cell);
                auto *shortcuts_cell = new QWidget(row);
                shortcuts_cell->setFixedWidth(shortcut_column_width());
                auto *shortcuts_layout = new QVBoxLayout(shortcuts_cell);
                shortcuts_layout->setContentsMargins(0, 0, 0, 0);
                shortcuts_layout->setSpacing(2);
                const int count = bridge_->actionShortcutCount(index);
                if (count == 0) {
                    auto *empty_row = new QWidget(shortcuts_cell);
                    empty_row->setFixedHeight(20);
                    auto *empty_layout = new QHBoxLayout(empty_row);
                    empty_layout->setContentsMargins(0, 0, 0, 0);
                    auto *empty = new QLabel(QStringLiteral("No shortcut"), empty_row);
                    empty->setObjectName(QStringLiteral("shortcutEmpty"));
                    empty_layout->addWidget(empty);
                    empty_layout->addStretch();
                    shortcuts_layout->addWidget(empty_row);
                }
                for (int position = 0; position < count; ++position) {
                    const QString label = bridge_->actionShortcutLabel(index, position);
                    auto *shortcut_line = new QWidget(shortcuts_cell);
                    shortcut_line->setFixedHeight(20);
                    auto *line_layout = new QHBoxLayout(shortcut_line);
                    line_layout->setContentsMargins(0, 0, 0, 0);
                    line_layout->setSpacing(6);
                    line_layout->addWidget(new QLabel(label, shortcut_line));
                    line_layout->addStretch();
                    auto *remove = new ShortcutGlyphButton(shortcut_line, ShortcutGlyph::Remove);
                    remove->setToolTip(QStringLiteral("Remove ") + label);
                    remove->setAccessibleName(QStringLiteral("Remove ") + label + QStringLiteral(" from ") + name);
                    QObject::connect(remove, &QToolButton::clicked, this, [this, index, position] { const auto error = bridge_->removeActionShortcut(index, position); if (!error.isEmpty()) show_row_error(index, error); });
                    line_layout->addWidget(remove);
                    shortcuts_layout->addWidget(shortcut_line);
                }
                auto *feedback = new QLabel(shortcuts_cell);
                feedback->setObjectName(QStringLiteral("shortcutFeedback"));
                feedback->setAccessibleName(name + QStringLiteral(" shortcut feedback"));
                feedback->setWordWrap(true);
                feedback->hide();
                feedback_by_action_.insert(index, feedback);
                auto *capture = new ShortcutCapture(row, [this, index](int key, int modifiers) { return bridge_->addActionShortcut(index, key, modifiers); }, [this](bool active) { if (capture_) capture_(active); }, [this](QWidget *widget) { reveal_feedback(widget); }, feedback);
                capture->setAccessibleName(QStringLiteral("Add shortcut for ") + name);
                cells->addWidget(shortcuts_cell);
                const QString original_shortcut = bridge_->actionDefaultShortcutLabel(index);
                auto *original_cell = new QWidget(row);
                original_cell->setFixedWidth(160);
                auto *original_layout = new QHBoxLayout(original_cell);
                original_layout->setContentsMargins(0, 0, 0, 0);
                auto *original_label = new QLabel(original_shortcut.isEmpty() ? QStringLiteral("None") : original_shortcut, original_cell);
                original_label->setObjectName(QStringLiteral("shortcutOriginal"));
                original_label->setAccessibleName(QStringLiteral("Original shortcut for ") + name + QStringLiteral(": ") + (original_shortcut.isEmpty() ? QStringLiteral("None") : original_shortcut));
                original_layout->addWidget(original_label, 1);
                auto *restore = new ShortcutGlyphButton(original_cell, ShortcutGlyph::Restore);
                restore->setToolTip(QStringLiteral("Reset to default shortcuts for ") + name);
                restore->setAccessibleName(QStringLiteral("Reset to default shortcuts for ") + name);
                QObject::connect(restore, &QToolButton::clicked, this, [this, index] { const auto error = bridge_->restoreActionShortcuts(index); if (!error.isEmpty()) show_row_error(index, error); });
                original_layout->addWidget(restore);
                cells->addWidget(original_cell);
                auto *scope_label = new QLabel(scope, row);
                scope_label->setObjectName(QStringLiteral("shortcutScope"));
                scope_label->setFixedWidth(120);
                cells->addWidget(scope_label);
                auto *add_cell = new QWidget(row);
                add_cell->setFixedWidth(36);
                auto *add_layout = new QHBoxLayout(add_cell);
                add_layout->setContentsMargins(0, 0, 0, 0);
                add_layout->addWidget(capture, 0, Qt::AlignCenter);
                cells->addWidget(add_cell);
                cells->addStretch();
                group_layout->addWidget(row);
                group_layout->addWidget(feedback);
            }
            if (group_rows == 0) {
                delete group;
            } else {
                shortcut_layout_->addWidget(group, 0, Qt::AlignLeft);
            }
        }
        shortcut_layout_->addStretch();
    }
    void show_row_error(int index, const QString &error) {
        if (auto feedback = feedback_by_action_.value(index)) {
            feedback->setText(error);
            feedback->setVisible(!error.isEmpty());
            if (!error.isEmpty())
                reveal_feedback(feedback);
        }
    }
    void reveal_feedback(QWidget *widget) {
        QTimer::singleShot(0, this, [this, widget] {
            if (shortcut_scroll_)
                shortcut_scroll_->ensureWidgetVisible(widget, 0, 16);
        });
    }
    WorkspaceBridge *bridge_;
    std::function<void(bool)> capture_;
    QListWidget *categories_;
    QStackedWidget *pages_;
    QWidget *shortcut_list_ = nullptr;
    QScrollArea *shortcut_scroll_ = nullptr;
    QVBoxLayout *shortcut_layout_ = nullptr;
    QHash<int, QPointer<QLabel>> feedback_by_action_;
};

class SettingsWindowController final : public QObject {
  public:
    explicit SettingsWindowController(QWidget *owner, WorkspaceBridge *bridge = nullptr) : QObject(owner), owner_(owner), bridge_(bridge), shortcut_(new QShortcut(QKeySequence(Qt::META | Qt::Key_Comma), owner)) {
        shortcut_->setContext(Qt::WindowShortcut);
        shortcut_->setEnabled(false);
        QObject::connect(shortcut_, &QShortcut::activated, this, [this] { present(); });
    }
    void set_capture_handler(std::function<void(bool)> handler) { capture_handler_ = std::move(handler); }
    void set_available(bool available) {
        if (!available && dialog_)
            dialog_->reject();
        available_ = available;
        shortcut_->setEnabled(available);
    }
    void present() {
        if (!available_)
            return;
        if (dialog_) {
            dialog_->show();
            dialog_->raise();
            dialog_->activateWindow();
            return;
        }
        auto *dialog = new SettingsWindow(owner_, bridge_, capture_handler_);
        dialog_ = dialog;
        QObject::connect(dialog, &QDialog::finished, this, [this] { dialog_.clear(); if (capture_handler_) capture_handler_(false); });
        dialog->show();
    }
    [[nodiscard]] auto dialog() const -> SettingsWindow * { return dialog_; }
    [[nodiscard]] auto shortcut() const -> QShortcut * { return shortcut_; }

  private:
    QWidget *owner_;
    WorkspaceBridge *bridge_;
    QShortcut *shortcut_;
    QPointer<SettingsWindow> dialog_;
    std::function<void(bool)> capture_handler_;
    bool available_ = false;
};

/// Exercises real Qt widgets without a display. The repository check script
/// launches this path with Qt's offscreen platform plugin.
auto run_settings_window_checks(QApplication &app) -> int {
    QMainWindow owner;
    QToolButton gear(&owner);
    gear.setEnabled(false);
    WorkspaceBridge bridge;
    SettingsWindowController controller(&owner, &bridge);
    const auto apply_availability = [&controller, &gear](bool available) {
        controller.set_available(available);
        gear.setEnabled(available);
    };
    QObject::connect(&gear, &QToolButton::clicked, &controller, [&controller] { controller.present(); });
    owner.show();
    QTimer::singleShot(0, &app, [&] {
        bool failed = false;
        const auto check = [&failed](bool condition, const char *name) {
            if (!condition) {
                std::cerr << "Settings window check failed: " << name << '\n';
                failed = true;
            }
        };
        check(!gear.isEnabled() && !controller.shortcut()->isEnabled(), "initial availability");
        controller.present();
        check(controller.dialog() == nullptr, "unavailable entry");
        apply_availability(true);
        check(gear.isEnabled() && controller.shortcut()->isEnabled(), "healthy availability");
        gear.click();
        auto *dialog = controller.dialog();
        if (!dialog) {
            check(false, "pointer entry");
            QApplication::exit(1);
            return;
        }
        check(dialog->isVisible() && dialog->parentWidget() == &owner && dialog->windowModality() == Qt::ApplicationModal && dialog->windowFlags().testFlag(Qt::FramelessWindowHint), "movable frameless Settings modality");
        auto *categories = dialog->findChild<QListWidget *>(QStringLiteral("settingsCategoryList"));
        auto *pages = dialog->findChild<QStackedWidget *>(QStringLiteral("settingsPages"));
        auto *title_row = dialog->findChild<QWidget *>(QStringLiteral("settingsTitleRow"));
        auto *close = dialog->findChild<QToolButton *>(QStringLiteral("settingsCloseButton"));
        auto *heading = dialog->findChild<QLabel *>(QStringLiteral("settingsHeading"));
        auto *notices = dialog->findChild<QCheckBox *>(QStringLiteral("settingsHideNotices"));
        auto *macos_items = dialog->findChild<QCheckBox *>(QStringLiteral("settingsShowMacosItems"));
        auto *dotfiles = dialog->findChild<QCheckBox *>(QStringLiteral("settingsShowDotfiles"));
        auto *macos_invisible = dialog->findChild<QCheckBox *>(QStringLiteral("settingsShowMacosInvisible"));
        auto *visibility_help = dialog->findChild<QLabel *>(QStringLiteral("settingsVisibilityHelp"));
        auto *reset = dialog->findChild<QPushButton *>(QStringLiteral("settingsReset"));
        check(categories && dialog->focusWidget() == categories && categories->currentRow() == 0, "initial category focus");
        check(pages && pages->count() == 2, "placeholder pages");
        check(title_row && heading && heading->text() == QStringLiteral("Settings") && heading->parentWidget() == title_row && notices && reset, "General controls and title row");
        check(macos_items && macos_items->accessibleName() == QStringLiteral("Show macOS metadata and service items") && macos_items->focusPolicy() != Qt::NoFocus, "accessible macOS items control");
        check(dotfiles && dotfiles->accessibleName() == QStringLiteral("Show dotfiles") && dotfiles->focusPolicy() != Qt::NoFocus, "accessible dotfiles control");
        check(macos_invisible && macos_invisible->accessibleName() == QStringLiteral("Show items marked invisible by macOS") && macos_invisible->focusPolicy() != Qt::NoFocus, "accessible macOS invisible control");
        check(visibility_help && visibility_help->text().contains(QStringLiteral("every matching Show control")), "visibility overlap help");
        check(close && close->parentWidget() == title_row && close->text().isEmpty() && !close->icon().isNull(), "bitmap Close button in title row");
        if (title_row) {
            const QPoint original = dialog->pos();
            const QPoint start = title_row->mapToGlobal(QPoint(20, 20));
            const QPoint delta(24, 18);
            QMouseEvent press(QEvent::MouseButtonPress, QPointF(20, 20), QPointF(start), Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
            QApplication::sendEvent(title_row, &press);
            QMouseEvent move(QEvent::MouseMove, QPointF(20, 20) + QPointF(delta), QPointF(start + delta), Qt::NoButton, Qt::LeftButton, Qt::NoModifier);
            QApplication::sendEvent(title_row, &move);
            QMouseEvent release(QEvent::MouseButtonRelease, QPointF(20, 20) + QPointF(delta), QPointF(start + delta), Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
            QApplication::sendEvent(title_row, &release);
            check(dialog->pos() == original + delta, "dragging title row moves Settings");
        }
        check(dialog->findChildren<QWidget *>(QStringLiteral("shortcutRow")).size() == bridge.actionCount(), "all action rows");
        const auto shortcut_groups = dialog->findChildren<QFrame *>(QStringLiteral("shortcutGroup"));
        check(shortcut_groups.size() == 4, "four human-readable shortcut groups");
        for (auto *group : shortcut_groups)
            check(!group->findChildren<QWidget *>(QStringLiteral("shortcutRow")).isEmpty(), "group contains action rows");
        check(dialog->findChildren<QToolButton *>().size() >= bridge.actionCount() + 3, "item information controls");
        for (auto *button : dialog->findChildren<QToolButton *>()) {
            if (button->accessibleName() != QStringLiteral("Startup notices information"))
                continue;
            button->click();
            auto *help = button->findChild<QMenu *>();
            check(help && help->isVisible(), "anchored help opens");
            if (help) {
                QPointer<QMenu> closing_help(help);
                help->close();
                QApplication::processEvents();
                check(!closing_help || !closing_help->isVisible(), "help dismisses");
            }
            break;
        }
        if (categories && pages) {
            categories->setCurrentRow(1);
            check(pages->currentIndex() == 1, "category switching");
        }
        controller.present();
        check(controller.dialog() == dialog && owner.findChildren<QDialog *>().size() == 1, "one-window raising");
        check(close && close->accessibleName() == QStringLiteral("Close Settings Window"), "accessible Close control");
        if (close)
            close->click();
        check(controller.dialog() == nullptr, "Close dismissal");
        QMetaObject::invokeMethod(controller.shortcut(), "activated");
        dialog = controller.dialog();
        check(dialog != nullptr, "keyboard entry");
        if (dialog) {
            QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
            QApplication::sendEvent(dialog, &escape);
            check(controller.dialog() == nullptr, "Escape dismissal");
        }
        controller.present();
        check(controller.dialog() != nullptr, "reopen after Escape");
        apply_availability(false);
        check(controller.dialog() == nullptr && !gear.isEnabled() && !controller.shortcut()->isEnabled(), "failure-time dismissal");
        controller.present();
        check(controller.dialog() == nullptr, "disabled entry after failure");
        QLabel feedback(&owner);
        bool capturing = false;
        int submissions = 0;
        ShortcutCapture capture(&owner, [&submissions](int key, int flags) { ++submissions; return key == Qt::Key_A && flags == 1 ? QString() : QStringLiteral("Invalid"); }, [&capturing](bool active) { capturing = active; }, [](QWidget *) {}, &feedback);
        capture.show();
        capture.click();
        check(capturing, "capture starts");
        QKeyEvent invalid(QEvent::KeyPress, Qt::Key_B, Qt::NoModifier);
        QApplication::sendEvent(&capture, &invalid);
        check(capturing && submissions == 1 && feedback.text() == QStringLiteral("Invalid"), "invalid input stays in capture");
        QKeyEvent accepted(QEvent::KeyPress, Qt::Key_A, Qt::MetaModifier);
        QApplication::sendEvent(&capture, &accepted);
        check(!capturing && submissions == 2 && feedback.text().isEmpty(), "accepted input leaves capture");
        capture.click();
        QKeyEvent cancel(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
        QApplication::sendEvent(&capture, &cancel);
        check(!capturing && submissions == 2, "bare Escape cancels capture");
        QApplication::exit(failed ? 1 : 0);
    });
    return QApplication::exec();
}

class MainToolbar final : public QWidget {
  public:
    MainToolbar(QWidget *parent, const std::function<void()> &show_notices, const std::function<void()> &show_settings) : QWidget(parent), settings_(new QToolButton(this)) {
        setObjectName(QStringLiteral("mainToolbar"));
        setAccessibleName(QStringLiteral("Main Toolbar"));
        auto *layout = new QHBoxLayout(this);
        layout->setContentsMargins(favorite_group_margin, 4, 0, 4);
        layout->setSpacing(0);
        settings_->setObjectName(QStringLiteral("settings"));
        settings_->setIcon(QIcon(QPixmap(settings_glyph::pixels)));
        settings_->setToolTip(QStringLiteral("Settings"));
        settings_->setAccessibleName(QStringLiteral("Settings"));
        settings_->setFocusPolicy(Qt::NoFocus);
        settings_->setIconSize(QSize(16, 16));
        settings_->setFixedSize(24, 24);
        settings_->setDisabled(true);
        QObject::connect(settings_, &QToolButton::clicked, this, show_settings);
        layout->addWidget(settings_, 0, Qt::AlignLeft);
        auto *notices = new QToolButton(this);
        notices->setObjectName(QStringLiteral("noticesButton"));
        notices->setIcon(QIcon(QPixmap(notices_glyph::pixels)));
        notices->setIconSize(QSize(16, 16));
        notices->setFixedSize(24, 24);
        notices->setToolTip(QStringLiteral("Notices"));
        notices->setAccessibleName(QStringLiteral("Notices Button"));
        notices->setFocusPolicy(Qt::TabFocus);
        QObject::connect(notices, &QToolButton::clicked, this, show_notices);
        layout->addWidget(notices, 0, Qt::AlignLeft);
        layout->addStretch(1);
    }

    void set_settings_available(bool available) {
        settings_->setEnabled(available);
        settings_->setFocusPolicy(available ? Qt::TabFocus : Qt::NoFocus);
    }

  private:
    QToolButton *settings_;
};

class Sidebar final : public QWidget {
  public:
    Sidebar(WorkspaceBridge *bridge, const std::function<void()> &focus_active_list, const std::function<void()> &show_notices, const std::function<void()> &show_settings, QWidget *parent) : QWidget(parent), toolbar_(new MainToolbar(this, show_notices, show_settings)) {
        setObjectName(QStringLiteral("sidebar"));
        setAccessibleName(QStringLiteral("Sidebar"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        auto *title = new SidebarTitleRow(this);
        layout->addWidget(title);
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
        layout->addWidget(toolbar_);
    }

    void set_settings_available(bool available) { toolbar_->set_settings_available(available); }

  private:
    class SidebarTitleRow final : public QLabel {
      public:
        explicit SidebarTitleRow(QWidget *parent) : QLabel(QStringLiteral("Dual Pane"), parent) {
            setObjectName(QStringLiteral("applicationTitle"));
            setAccessibleName(QStringLiteral("Dual Pane"));
            setMinimumHeight(54);
            setAttribute(Qt::WA_StyledBackground);
        }

      protected:
        void mousePressEvent(QMouseEvent *event) override {
            if (event->button() == Qt::LeftButton && window()->windowHandle() != nullptr && window()->windowHandle()->startSystemMove()) {
                moving_window_ = true;
                event->accept();
                return;
            }
            QLabel::mousePressEvent(event);
        }
        void mouseReleaseEvent(QMouseEvent *event) override {
            if (moving_window_) {
                moving_window_ = false;
                event->accept();
                return;
            }
            QLabel::mouseReleaseEvent(event);
        }

      private:
        bool moving_window_ = false;
    };
    MainToolbar *toolbar_;
};

class BrowserHighlightController final : public QObject {
  public:
    BrowserHighlightController(QWidget *window, std::array<Browser *, 2> browsers, QWidget *sidebar) : window_(window), browsers_(browsers), sidebar_(sidebar) {
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
        sidebar_->setProperty("windowActive", window_active);
        repolish(sidebar_);
        for (QWidget *child : sidebar_->findChildren<QWidget *>())
            repolish(child);
        for (int browser = 0; browser < 2; ++browser) {
            const auto apply_state = [this, browser, window_active](QWidget *widget) {
                widget->setProperty("browserActive", browser == active_);
                widget->setProperty("windowActive", window_active);
                if (auto *sort = dynamic_cast<QToolButton *>(widget); sort != nullptr && sort->objectName() == QStringLiteral("sortControl"))
                    sort->setIcon(sort_caret_icon(sort->property("sortDescending").toBool(), browser == active_ && window_active));
                else if (auto *button = dynamic_cast<QToolButton *>(widget); button != nullptr && button->objectName() == QStringLiteral("backButton"))
                    button->setIcon(navigation_icon(NavigationIcon::Back, browser == active_ && window_active));
                else if (auto *button = dynamic_cast<QToolButton *>(widget); button != nullptr && button->objectName() == QStringLiteral("forwardButton"))
                    button->setIcon(navigation_icon(NavigationIcon::Forward, browser == active_ && window_active));
                else if (auto *button = dynamic_cast<QToolButton *>(widget); button != nullptr && button->objectName() == QStringLiteral("upButton"))
                    button->setIcon(navigation_icon(NavigationIcon::Up, browser == active_ && window_active));
                repolish(widget);
            };
            auto *browser_widget = static_cast<QWidget *>(browsers_.at(browser));
            apply_state(browser_widget);
            for (QWidget *child : browser_widget->findChildren<QWidget *>())
                apply_state(child);
            browsers_.at(browser)->view()->viewport()->update();
        }
    }

    QWidget *window_;
    std::array<Browser *, 2> browsers_;
    QWidget *sidebar_;
    int active_ = 0;
};

/// Binds every delivered action to its effective shortcut from settings and
/// rebinds after a load or reset. File commands on Folder Items act only
/// while a Folder Items List has focus. While an inline editor has focus,
/// only Quit stays active.
class ShortcutBinder final {
  public:
    using Handler = std::function<void()>;
    using ListHandler = std::function<void(int)>;
    ShortcutBinder(QWidget *window, WorkspaceBridge *bridge, QHash<QString, Handler> handlers, std::array<QWidget *, 2> lists, QHash<QString, ListHandler> list_handlers) : window_(window), bridge_(bridge), handlers_(std::move(handlers)), lists_(lists), list_handlers_(std::move(list_handlers)) {}

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
            const auto scope = bridge_->bindingScope(index);
            if (sequence.isEmpty())
                continue;
            // File commands act only on the Folder Items List that has focus.
            if (scope == scope_folder_items_list) {
                const auto list_handler = list_handlers_.value(action);
                if (!list_handler)
                    continue;
                for (int browser = 0; browser < 2; ++browser) {
                    auto *shortcut = new QShortcut(QKeySequence::fromString(sequence, QKeySequence::PortableText), lists_.at(browser));
                    shortcut->setContext(Qt::WidgetShortcut);
                    QObject::connect(shortcut, &QShortcut::activated, lists_.at(browser), [list_handler, browser] { list_handler(browser); });
                    shortcuts_.push_back(shortcut);
                }
                continue;
            }
            const auto handler = handlers_.value(action);
            if (!handler)
                continue;
            auto *shortcut = new QShortcut(QKeySequence::fromString(sequence, QKeySequence::PortableText), window_);
            const bool application = scope == scope_application;
            // Quit works from every window, including Notices; workspace
            // actions belong to the main window only.
            shortcut->setContext(application ? Qt::ApplicationShortcut : Qt::WindowShortcut);
            shortcut->setProperty("application", application);
            QObject::connect(shortcut, &QShortcut::activated, window_, handler);
            shortcuts_.push_back(shortcut);
        }
        apply();
    }
    void setEditing(bool editing) {
        editing_ = editing;
        apply();
    }
    void setCapture(bool active) {
        capture_ = active;
        apply();
    }

  private:
    void apply() {
        for (auto *shortcut : shortcuts_)
            shortcut->setEnabled(!capture_ && (!editing_ || shortcut->property("application").toBool()));
    }

    QWidget *window_;
    WorkspaceBridge *bridge_;
    QHash<QString, Handler> handlers_;
    std::array<QWidget *, 2> lists_;
    QHash<QString, ListHandler> list_handlers_;
    std::vector<QShortcut *> shortcuts_;
    bool editing_ = false;
    bool capture_ = false;
};

/// Refreshes every binder after any call that may have changed the session.
/// A refresh that triggers another call refreshes again instead of nesting.
class RefreshCoordinator final {
  public:
    RefreshCoordinator(FolderItemsListModel *left, FolderItemsListModel *right, WorkspaceBridge *bridge, OperationsModel *operations) : left_(left), right_(right), bridge_(bridge), operations_(operations) {}
    /// Runs after every refresh, for binders that follow the session as a
    /// whole, such as inline editors waiting on a job.
    void addListener(std::function<void()> listener) { listeners_.push_back(std::move(listener)); }
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
            operations_->refresh();
            for (const auto &listener : listeners_)
                listener();
        }
        refreshing_ = false;
    }

  private:
    FolderItemsListModel *left_;
    FolderItemsListModel *right_;
    WorkspaceBridge *bridge_;
    OperationsModel *operations_;
    std::vector<std::function<void()>> listeners_;
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
    if (qEnvironmentVariableIsSet("DUAL_PANE_INTERNAL_SETTINGS_WINDOW_CHECK"))
        return run_settings_window_checks(app);
    WorkspaceWindow window;
    auto *waiting = new WaitingWindow(&window);
    FolderItemsListModel left_model, right_model;
    right_model.setRightBrowser();
    WorkspaceBridge bridge;
    OperationsModel operations;
    RefreshCoordinator coordinator(&left_model, &right_model, &bridge, &operations);
    QObject::connect(&left_model, &FolderItemsListModel::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
    QObject::connect(&right_model, &FolderItemsListModel::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
    QObject::connect(&bridge, &WorkspaceBridge::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
    QObject::connect(&operations, &OperationsModel::sessionChanged, &window, [&coordinator] { coordinator.refresh(); });
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
    auto *settings_controller = new SettingsWindowController(&window, &bridge);
    const auto show_settings = [settings_controller] { settings_controller->present(); };
    auto *sidebar = new Sidebar(&bridge, focus_active_list, show_notices, show_settings, &standard_layout);
    split->addWidget(left_browser);
    split->addWidget(right_browser);
    // The Operation Panel Strip runs below both Browsers.
    auto *browser_area = new QWidget(&standard_layout);
    auto *browser_area_layout = new QVBoxLayout(browser_area);
    browser_area_layout->setContentsMargins(0, 0, 0, 0);
    browser_area_layout->setSpacing(0);
    browser_area_layout->addWidget(split, 1);
    auto *strip = new OperationPanelStrip(browser_area);
    browser_area_layout->addWidget(strip);
    standard_layout.addWidget(sidebar);
    standard_layout.addWidget(browser_area);
    bool quitting = false;
    OperationPanels panels(&operations, strip, &window, &quitting);
    DeleteConfirmations confirmations(&operations, &window);
    for (auto *browser : browsers)
        coordinator.addListener([browser] { browser->editors()->update(); });
    QTimer operations_tick;
    operations_tick.setSingleShot(true);
    QObject::connect(&operations_tick, &QTimer::timeout, &operations, [&operations] { operations.tick(); });
    QObject::connect(&operations, &OperationsModel::nextDeadlineChanged, &window, [&operations, &operations_tick] {
        const int deadline = operations.getNextDeadline();
        if (deadline < 0)
            operations_tick.stop();
        else
            operations_tick.start(deadline + 1);
    });
    // Command+Q, Dock Quit, and the close button all arrive here. With
    // operations running, the quit waits for confirmation; quitting cancels
    // them, and cleanup continues for a bounded time after the event loop.
    QPointer<QMessageBox> quit_prompt;
    const auto finish_quit = [&quitting] {
        quitting = true;
        QTimer::singleShot(0, qApp, [] { QApplication::quit(); });
    };
    window.setCloseHandler([&](QCloseEvent *event) {
        if (quitting) {
            event->accept();
            return;
        }
        if (operations.requestQuit()) {
            event->accept();
            finish_quit();
            return;
        }
        event->ignore();
        if (quit_prompt) {
            quit_prompt->raise();
            return;
        }
        const int running = operations.quitPromptRunning();
        auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Quit Dual Pane"), QStringLiteral("Quit Dual Pane?"), QMessageBox::NoButton, &window);
        box->setObjectName(QStringLiteral("quitConfirmationWindow"));
        box->setAccessibleName(QStringLiteral("Quit Confirmation Window"));
        box->setInformativeText(running == 1 ? QStringLiteral("1 file operation is still running. Quitting cancels it; work already done stays.") : QStringLiteral("%1 file operations are still running. Quitting cancels them; work already done stays.").arg(running));
        auto *quit = box->addButton(QStringLiteral("Quit"), QMessageBox::DestructiveRole);
        auto *cancel = box->addButton(QMessageBox::Cancel);
        box->setDefaultButton(cancel);
        box->setEscapeButton(cancel);
        box->setWindowModality(Qt::WindowModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, &window, [&operations, box, quit, finish_quit] {
            if (box->clickedButton() == quit && operations.confirmQuit())
                finish_quit();
        });
        quit_prompt = box;
        box->open();
    });
    window.setCentralWidget(&standard_layout);
    window.setWindowTitle(QStringLiteral("Dual Pane"));
    window.resize(initial_window_width, initial_window_height);
    window.setStyleSheet(style_sheet());
    QTimer layout_report;
    layout_report.setSingleShot(true);
    QTimer startup_period_timer;
    startup_period_timer.setSingleShot(true);
    QTimer waiting_delay;
    waiting_delay.setSingleShot(true);
    QTimer settings_load_timeout;
    settings_load_timeout.setSingleShot(true);
    const auto report_layout = [&bridge, &window, &standard_layout, split, browsers] {
        const auto normal = (window.isMaximized() || window.isFullScreen()) ? window.normalGeometry() : window.geometry();
        int state = 0;
        if (window.isFullScreen())
            state = 2;
        else if (window.isMaximized())
            state = 1;
        const auto sidebar_sizes = standard_layout.sizes();
        const auto browser_sizes = split->sizes();
        const auto packed_origin = (static_cast<std::uint64_t>(static_cast<std::uint32_t>(normal.x())) << static_cast<unsigned>(32)) | static_cast<std::uint64_t>(static_cast<std::uint32_t>(normal.y()));
        const auto origin = static_cast<qint64>(packed_origin);
        const auto packed_splitters = (static_cast<std::uint64_t>(static_cast<std::uint32_t>(sidebar_sizes.value(0))) << static_cast<unsigned>(32)) | static_cast<std::uint64_t>(static_cast<std::uint32_t>(browser_sizes.value(0)));
        const auto packed_previews = (static_cast<std::uint64_t>(static_cast<std::uint32_t>(browsers.at(0)->preview_height())) << static_cast<unsigned>(32)) | static_cast<std::uint64_t>(static_cast<std::uint32_t>(browsers.at(1)->preview_height()));
        bridge.updateWindowLayout(origin, normal.width(), normal.height(), state, static_cast<qint64>(packed_splitters), static_cast<qint64>(packed_previews));
    };
    window.setBeforeCloseHandler(report_layout);
    QObject::connect(&layout_report, &QTimer::timeout, &window, report_layout);
    window.setLayoutChangeHandler([&layout_report] { layout_report.start(120); });
    QObject::connect(&standard_layout, &QSplitter::splitterMoved, &window, [&layout_report](int, int) { layout_report.start(120); });
    QObject::connect(split, &QSplitter::splitterMoved, &window, [&layout_report](int, int) { layout_report.start(120); });
    for (auto *browser : browsers)
        browser->set_preview_changed_handler([&layout_report] { layout_report.start(120); });
    bool workspace_revealed = false;
    QObject::connect(&waiting_delay, &QTimer::timeout, &window, [&window, waiting, &workspace_revealed] {
        if (!workspace_revealed) {
            const auto available = window.screen()->availableGeometry();
            waiting->move(available.center() - waiting->rect().center());
            waiting->show();
        }
    });
    QObject::connect(&settings_load_timeout, &QTimer::timeout, &window, [&bridge, &workspace_revealed] {
        if (!workspace_revealed)
            bridge.settingsLoadTimedOut();
    });
    QObject::connect(&startup_period_timer, &QTimer::timeout, &window, [&bridge] { bridge.startupPeriodElapsed(); });
    QObject::connect(&bridge, &WorkspaceBridge::layoutRevisionChanged, &window, [&bridge, &window, &standard_layout, split, browsers, waiting, &waiting_delay, &settings_load_timeout, &workspace_revealed, &startup_period_timer, report_layout] {
        const bool saved_layout = bridge.getSavedLayoutAvailable();
        const auto apply_saved_splitters = [&bridge, &standard_layout, split, browsers] {
            // The window must have been shown and laid out before these
            // pixel sizes are meaningful. Applying them while the workspace
            // is hidden lets QSplitter redistribute the sizes again during
            // the following show/maximize/full-screen transition.
            if (standard_layout.width() > 0)
                standard_layout.setSizes({bridge.getSavedSidebarSplitter(), qMax(0, standard_layout.width() - bridge.getSavedSidebarSplitter())});
            if (split->width() > 0)
                split->setSizes({bridge.getSavedBrowserSplitter(), qMax(0, split->width() - bridge.getSavedBrowserSplitter())});
            browsers.at(0)->set_preview_height(bridge.getSavedLeftPreviewHeight());
            browsers.at(1)->set_preview_height(bridge.getSavedRightPreviewHeight());
        };
        if (!bridge.getSavedLayoutAvailable()) {
            for (auto *browser : browsers)
                browser->set_preview_height(0);
            window.showNormal();
            window.resize(initial_window_width, initial_window_height);
            const auto available = window.screen()->availableGeometry();
            window.move(available.center() - window.rect().center());
        } else {
            const QRect frame = fit_saved_frame(QRect(bridge.getSavedLayoutX(), bridge.getSavedLayoutY(), bridge.getSavedLayoutWidth(), bridge.getSavedLayoutHeight()));
            window.move(frame.topLeft());
            window.resize(frame.size());
            if (bridge.getSavedLayoutState() == 1)
                window.showMaximized();
            else if (bridge.getSavedLayoutState() == 2)
                window.showFullScreen();
            else
                window.showNormal();
            apply_saved_splitters();
        }
        if (!workspace_revealed) {
            workspace_revealed = true;
            waiting_delay.stop();
            settings_load_timeout.stop();
            waiting->close();
            browsers.at(bridge.getActiveBrowser() == 0 ? 0 : 1)->view()->focusList();
            // A hidden QMainWindow can still have stale child geometry after
            // resize/showState changes. Reapply once on the next event-loop
            // turn, when both nested splitters have their final width, then
            // persist the actual positions rather than the pre-layout ones.
            QTimer::singleShot(0, &window, [apply_saved_splitters, report_layout, saved_layout] {
                if (saved_layout)
                    apply_saved_splitters();
                report_layout();
            });
            startup_period_timer.start(5000);
        }
    });
    BrowserHighlightController browser_highlighter(&window, browsers, sidebar);
    QObject::connect(&bridge, &WorkspaceBridge::activeBrowserChanged, &window, [&bridge, &browser_highlighter] { browser_highlighter.activate(bridge.getActiveBrowser()); });
    QObject::connect(&bridge, &WorkspaceBridge::startupFailureChanged, &window, [&bridge, browsers] {
        for (auto *browser : browsers)
            browser->folder()->status()->set_full_text(bridge.getStartupFailure());
    });
    // An actionable storage error opens Notices, even at launch, without
    // taking focus from browsing.
    QObject::connect(&bridge, &WorkspaceBridge::noticesOpenRequestsChanged, &window, [notices] {
        notices->setAttribute(Qt::WA_ShowWithoutActivating, !notices->isVisible());
        notices->show();
        notices->raise();
        notices->setAttribute(Qt::WA_ShowWithoutActivating, false);
    });
    // This fixed presentation command is not an ActionId. Its workspace
    // window context keeps it inactive while the modal sheet owns focus.
    const auto apply_settings_availability = [&bridge, sidebar, settings_controller] {
        const bool available = bridge.getSettingsInteractionAvailable();
        settings_controller->set_available(available);
        sidebar->set_settings_available(available);
    };
    QObject::connect(&bridge, &WorkspaceBridge::settingsInteractionAvailableChanged, &window, apply_settings_availability);
    apply_settings_availability();

    const auto active_model = [&bridge, models] { return models.at(bridge.getActiveBrowser() == 0 ? 0 : 1); };
    QHash<QString, ShortcutBinder::Handler> handlers;
    handlers.insert(QStringLiteral("FocusOtherBrowser"), [&bridge, browsers] { browsers.at(bridge.getActiveBrowser() == 0 ? 1 : 0)->view()->focusList(); });
    handlers.insert(QStringLiteral("FocusFolderItemsFilter"), [&bridge, browsers] { browsers.at(bridge.getActiveBrowser() == 0 ? 0 : 1)->focus_filter(); });
    handlers.insert(QStringLiteral("NavigateParent"), [active_model] { active_model()->goToParent(); });
    handlers.insert(QStringLiteral("CloseWindow"), [&window] { window.close(); });
    handlers.insert(QStringLiteral("ToggleFullScreen"), [&window] {
        if (window.isFullScreen())
            window.showNormal();
        else
            window.showFullScreen();
    });
    // The workspace window decides first, before any other window is asked.
    handlers.insert(QStringLiteral("QuitApplication"), [&window] { window.close(); });
    handlers.insert(QStringLiteral("NewTab"), [active_model] { active_model()->newTab(); });
    // Command+W reaches a docked panel that has focus: it closes a finished
    // panel and does nothing on a running or waiting one.
    handlers.insert(QStringLiteral("CloseTab"), [active_model, &panels, &operations] {
        if (auto *panel = panels.docked_panel(QApplication::focusWidget())) {
            if (panel->finished())
                operations.closePanel(panel->job());
            return;
        }
        active_model()->closeActiveTab();
    });
    handlers.insert(QStringLiteral("NewFolder"), [active_model] { active_model()->startFileCommand(command_new_folder); });
    handlers.insert(QStringLiteral("NavigateBack"), [active_model] { active_model()->goBack(); });
    handlers.insert(QStringLiteral("NavigateForward"), [active_model] { active_model()->goForward(); });
    handlers.insert(QStringLiteral("RefreshFolder"), [active_model] { active_model()->refreshFolder(); });
    const std::array<QString, 8> sorts = {QStringLiteral("SortByNameAscending"), QStringLiteral("SortByNameDescending"), QStringLiteral("SortByTypeAscending"), QStringLiteral("SortByTypeDescending"), QStringLiteral("SortByDateAscending"), QStringLiteral("SortByDateDescending"), QStringLiteral("SortBySizeAscending"), QStringLiteral("SortBySizeDescending")};
    for (int choice = 0; choice < static_cast<int>(sorts.size()); ++choice)
        handlers.insert(sorts.at(choice), [active_model, choice] { active_model()->setSort(choice); });
    // The bridge decides which actions are scoped to a focused list.
    QHash<QString, ShortcutBinder::ListHandler> list_handlers;
    const std::array<std::pair<QString, int>, 5> file_commands = {{{QStringLiteral("CopyToOtherBrowser"), command_copy}, {QStringLiteral("MoveToOtherBrowser"), command_move}, {QStringLiteral("RenameItem"), command_rename}, {QStringLiteral("MoveToTrash"), command_trash}, {QStringLiteral("DeletePermanently"), command_delete}}};
    for (const auto &file_command : file_commands)
        list_handlers.insert(file_command.first, [models, command = file_command.second](int browser) { models.at(browser)->startFileCommand(command); });
    list_handlers.insert(QStringLiteral("OpenItem"), [models](int browser) { models.at(browser)->activateSelected(); });
    ShortcutBinder shortcuts(&window, &bridge, handlers, {left_browser->view(), right_browser->view()}, list_handlers);
    settings_controller->set_capture_handler([&shortcuts](bool active) { shortcuts.setCapture(active); });
    QObject::connect(&bridge, &WorkspaceBridge::bindingsRevisionChanged, &window, [&shortcuts] { shortcuts.rebind(); });

    auto *toggle_full_screen = new QShortcut(QKeySequence(QStringLiteral("Meta+Ctrl+F")), &window);
    toggle_full_screen->setContext(Qt::WindowShortcut);
    QObject::connect(toggle_full_screen, &QShortcut::activated, &window, [&window] {
        if (window.isFullScreen())
            window.showNormal();
        else
            window.showFullScreen();
    });

    // Focus entering any control inside a Browser activates that Browser.
    QObject::connect(&app, &QApplication::focusChanged, &window, [browsers, models, &shortcuts](QWidget *, QWidget *now) {
        shortcuts.setEditing(now != nullptr && (now->property("inlineEditor").toBool() || now->property("filterTextEditor").toBool()));
        for (int browser = 0; browser < 2; ++browser)
            if (inside(now, browsers.at(browser)))
                models.at(browser)->activateBrowser();
    });
    QObject::connect(&app, &QGuiApplication::applicationStateChanged, &window, [&bridge](Qt::ApplicationState state) { bridge.setApplicationActive(state == Qt::ApplicationActive); });

    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = &scheduler;
    }
    bridge.start(std::move(startup));
    bridge.setApplicationActive(QApplication::applicationState() == Qt::ApplicationActive);
    waiting_delay.start(150);
    settings_load_timeout.start(10000);
    shortcuts.rebind();
    // The root volume's name labels root tabs; reading it may touch the
    // disk, so it happens off the GUI thread.
    std::unique_ptr<QThread> volume_reader(QThread::create([&bridge] {
        const auto name = QStorageInfo::root().displayName();
        QMetaObject::invokeMethod(&bridge, [&bridge, name] { bridge.setRootVolumeName(name); }, Qt::QueuedConnection);
    }));
    volume_reader->start(QThread::LowPriority);
    const auto result = QApplication::exec();
    settings_controller->set_available(false);
    settings_controller->set_capture_handler({});
    // The lambdas above capture stack objects that are destroyed in reverse
    // order below; focus changes during that teardown must not reach them.
    QObject::disconnect(&app, nullptr, &window, nullptr);
    for (QObject *source : std::initializer_list<QObject *>{&bridge, &left_model, &right_model, &operations})
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
