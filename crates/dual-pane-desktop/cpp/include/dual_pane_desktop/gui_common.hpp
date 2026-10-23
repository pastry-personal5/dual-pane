#ifndef dual_pane_DESKTOP_GUI_COMMON_HPP
#define dual_pane_DESKTOP_GUI_COMMON_HPP

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
#include <QtGui/QClipboard>
#include <QtGui/QCloseEvent>
#include <QtGui/QColor>
#include <QtGui/QCursor>
#include <QtGui/QDrag>
#include <QtGui/QDragEnterEvent>
#include <QtGui/QDragLeaveEvent>
#include <QtGui/QDragMoveEvent>
#include <QtGui/QDropEvent>
#include <QtGui/QFocusEvent>
#include <QtGui/QFontDatabase>
#include <QtGui/QFontMetrics>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QInputMethodEvent>
#include <QtGui/QKeyEvent>
#include <QtGui/QKeySequence>
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
namespace dual_pane_desktop::gui {

inline constexpr int initial_window_width = 1100, initial_window_height = 680;

/// Moves a saved logical-point frame fully onto the display it overlaps most,
/// shrinking only when that display cannot contain it.
auto fit_saved_frame(QRect frame) -> QRect;

inline constexpr int splitter_width = 3;
inline constexpr int narrow_browser_width = 280;
inline constexpr int column_count = 6;
inline constexpr int icon_column_width = 18;
inline constexpr int relative_date_column_width = 44;
inline constexpr int type_column_width = 44;
inline constexpr int path_role = Qt::UserRole;
inline constexpr int native_path_role = Qt::UserRole + 2;
inline constexpr int native_directory_role = Qt::UserRole + 3;
inline constexpr int pending_cut_role = Qt::UserRole + 4;
inline constexpr int relative_age_role = Qt::UserRole + 1;
inline constexpr int max_pending_icons = 128;
inline constexpr int icon_cache_size = 1024;
inline constexpr int tab_limit = 8;
inline constexpr auto favorite_item_mime = "application/x-dual-pane-favorite-item";
inline constexpr auto operation_panel_mime = "application/x-dual-pane-operation-panel";
inline constexpr auto file_drag_mime = "application/x-dual-pane-file-drag-v1";
// File command codes of the Folder Items model.
inline constexpr int command_copy = 0, command_move = 1, command_rename = 2, command_new_folder = 3, command_trash = 4, command_delete = 5;
inline constexpr int clipboard_copy = 0, clipboard_cut = 1, clipboard_paste = 2;
// Shortcut scope codes of the workspace bridge.
inline constexpr int scope_folder_items_list = 0, scope_window = 1, scope_application = 2;
inline constexpr int status_reason_milliseconds = 3000;
inline constexpr int quick_look_hold_milliseconds = 256;
inline constexpr auto window_color = "#1B1D21", surface_color = "#23262B", inactive_surface_color = "#17191D", text_color = "#ECEFF3", inactive_text_color = "#8A9099", border_color = "#3A4048", active_color = "#2F6D9A", inactive_browser_color = "#173A55", hover_color = "#2B3037", favorite_group_color = "#15171A", divider_hover_border_color = "#737A84", error_color = "#E5737A";
// The Application Close Button quits, so its revealed state is a destructive
// red: the white glyph keeps 4.8:1 contrast, and the red keeps 3.5:1 against
// the Sidebar.
inline constexpr auto application_close_hover_color = "#C9434A", application_close_pressed_color = "#A8363D", application_close_glyph_color = "#FFFFFF";
inline constexpr int application_close_button_size = 30;
inline constexpr int favorite_group_margin = 8;
inline constexpr int favorite_row_horizontal_padding = 8;
inline constexpr int favorite_row_vertical_padding = 4;

// `toolbarState` flags from the Folder Items model.
inline constexpr unsigned toolbar_new_tab = 1U, toolbar_back = 2U, toolbar_forward = 4U, toolbar_up = 8U;

constexpr auto has_flag(int state, unsigned flag) -> bool { return (static_cast<unsigned>(state) & flag) != 0U; }

inline auto file_drag_payload(std::uint64_t drag) -> QByteArray {
    QByteArray payload;
    payload.reserve(9);
    payload.append(1);
    for (int byte = 0; byte < 8; ++byte)
        payload.append(static_cast<char>((drag >> static_cast<unsigned>(byte * 8)) & 0xffU));
    return payload;
}

inline auto decode_file_drag_payload(const QMimeData *mime) -> std::optional<std::uint64_t> {
    if (mime == nullptr || !mime->hasFormat(QString::fromLatin1(file_drag_mime)))
        return std::nullopt;
    const auto payload = mime->data(QString::fromLatin1(file_drag_mime));
    if (payload.size() != 9 || payload.at(0) != 1)
        return std::nullopt;
    std::uint64_t drag = 0;
    for (int byte = 0; byte < 8; ++byte)
        drag |= static_cast<std::uint64_t>(static_cast<unsigned char>(payload.at(byte + 1))) << static_cast<unsigned>(byte * 8);
    return drag == 0 ? std::nullopt : std::optional<std::uint64_t>(drag);
}

inline auto internal_file_drag(const QDropEvent *event) -> std::optional<std::uint64_t> {
    if (event == nullptr || event->source() == nullptr || !event->source()->property("dualPaneFolderItemsSource").toBool())
        return std::nullopt;
    return decode_file_drag_payload(event->mimeData());
}

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

constexpr auto drop_modifiers_supported(Qt::KeyboardModifiers modifiers) -> bool {
    const auto relevant = user_modifiers(modifiers);
    return relevant == Qt::NoModifier || relevant == Qt::AltModifier;
}

constexpr auto drop_is_copy(Qt::KeyboardModifiers modifiers) -> bool { return user_modifiers(modifiers) == Qt::AltModifier; }

static_assert(movement_for(Qt::Key_Up) == Movement::Previous);
static_assert(movement_for(Qt::Key_PageDown) == Movement::PageDown);
static_assert(!movement_for(Qt::Key_Left).has_value());
static_assert(drop_modifiers_supported(Qt::NoModifier));
static_assert(drop_modifiers_supported(Qt::AltModifier));
static_assert(!drop_modifiers_supported(Qt::ShiftModifier));
static_assert(!drop_modifiers_supported(Qt::AltModifier | Qt::ControlModifier));
static_assert(user_modifiers(Qt::KeypadModifier) == Qt::NoModifier);
static_assert(user_modifiers(Qt::ShiftModifier | Qt::KeypadModifier) == Qt::ShiftModifier);

auto style_sheet() -> QString;

void repolish(QWidget *widget);

auto command_button(const QString &object_name, const QString &text, const QString &tool_tip, const QString &accessible_name, QWidget *parent) -> QToolButton *;

auto sort_caret_icon(bool descending, bool focused) -> QIcon;

enum class NavigationIcon : std::uint8_t { Back,
                                           Forward,
                                           Up,
};

auto make_navigation_icon(NavigationIcon direction, bool focused) -> QIcon;

auto navigation_icon(NavigationIcon direction, bool focused) -> const QIcon &;

/// Whether `widget` is `ancestor` or inside it.
auto inside(const QWidget *widget, const QWidget *ancestor) -> bool;

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_GUI_COMMON_HPP
