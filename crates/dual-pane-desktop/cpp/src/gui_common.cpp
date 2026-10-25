#include "dual_pane_desktop/gui_common.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

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
      QToolButton#shortcutGlyphButton { background:transparent; border:0; border-radius:9px; padding:0; }
      QToolButton#shortcutGlyphButton:hover:enabled { background:%11; }
      QToolButton#shortcutGlyphButton:focus { border:1px solid %5; }
      QToolButton#backButton:hover:enabled, QToolButton#forwardButton:hover:enabled, QToolButton#upButton:hover:enabled, QToolButton#settings:hover:enabled, QToolButton#noticesButton:hover:enabled { background:%5; }
      QToolButton#favoriteGroupMenuButton, QToolButton#addFavoriteItemButton { color:#A7ADB5; background:#24282E; }
      QToolButton#favoriteGroupMenuButton:hover:enabled, QToolButton#addFavoriteItemButton:hover:enabled { color:#FFFFFF; background:#2F6D9A; }
      QToolButton#favoriteGroupMenuButton:pressed:enabled, QToolButton#addFavoriteItemButton:pressed:enabled { color:#FFFFFF; background:#245878; }
      QToolButton#favoriteGroupMenuButton:disabled, QToolButton#addFavoriteItemButton:disabled { color:#6F757D; background:transparent; }
      QWidget#sidebar[windowActive="false"] QToolButton#favoriteGroupMenuButton, QWidget#sidebar[windowActive="false"] QToolButton#addFavoriteItemButton { color:#8A9099; background:#1B1D21; }
      QLabel#applicationTitle { background:transparent; color:%3; font-size:24px; font-weight:600; }
      QToolButton#sortControl:hover:enabled:!checked, QToolButton#newTabButton:hover:enabled, QToolButton#closeTabButton:hover:enabled, QToolButton#newGroupButton:hover:enabled, QFrame#operationPanel QToolButton:hover:enabled { background:%11; }
      QWidget#folderPaneToolbarRow1, QWidget#folderSummaryRow, QWidget#folderNavigationAndSortToolbar, QWidget#folderSummaryRow QLabel, QWidget#folderSummaryRow QLineEdit, QWidget#folderSummaryRow QToolButton, QWidget#folderNavigationAndSortToolbar QLabel, QWidget#folderNavigationAndSortToolbar QLineEdit, QWidget#folderNavigationAndSortToolbar QToolButton { background:%1; }
      QWidget#folderPaneToolbarRow1[browserActive="true"][windowActive="true"], QWidget#folderSummaryRow[browserActive="true"][windowActive="true"], QWidget#folderNavigationAndSortToolbar[browserActive="true"][windowActive="true"], QWidget#folderSummaryRow[browserActive="true"][windowActive="true"] QLabel, QWidget#folderSummaryRow[browserActive="true"][windowActive="true"] QLineEdit, QWidget#folderSummaryRow[browserActive="true"][windowActive="true"] QToolButton, QWidget#folderNavigationAndSortToolbar[browserActive="true"][windowActive="true"] QLabel, QWidget#folderNavigationAndSortToolbar[browserActive="true"][windowActive="true"] QLineEdit, QWidget#folderNavigationAndSortToolbar[browserActive="true"][windowActive="true"] QToolButton { background:%2; }
      QWidget#folderPaneToolbarRow1[browserActive="false"][windowActive="true"], QWidget#folderSummaryRow[browserActive="false"][windowActive="true"], QWidget#folderNavigationAndSortToolbar[browserActive="false"][windowActive="true"], QWidget#folderSummaryRow[browserActive="false"][windowActive="true"] QLabel, QWidget#folderSummaryRow[browserActive="false"][windowActive="true"] QLineEdit, QWidget#folderSummaryRow[browserActive="false"][windowActive="true"] QToolButton, QWidget#folderNavigationAndSortToolbar[browserActive="false"][windowActive="true"] QLabel, QWidget#folderNavigationAndSortToolbar[browserActive="false"][windowActive="true"] QLineEdit, QWidget#folderNavigationAndSortToolbar[browserActive="false"][windowActive="true"] QToolButton { background:%9; }
      QWidget#browserStatusBar, QWidget#browserStatusSummary, QWidget#browserStatusSummary QLabel, QLabel#browserStatusMessage { background:%1; }
      QWidget#browserStatusBar[browserActive="true"][windowActive="true"], QWidget#browserStatusSummary[browserActive="true"][windowActive="true"], QWidget#browserStatusSummary[browserActive="true"][windowActive="true"] QLabel, QLabel#browserStatusMessage[browserActive="true"][windowActive="true"] { background:%2; }
      QWidget#browserStatusBar[browserActive="false"][windowActive="true"], QWidget#browserStatusSummary[browserActive="false"][windowActive="true"], QWidget#browserStatusSummary[browserActive="false"][windowActive="true"] QLabel, QLabel#browserStatusMessage[browserActive="false"][windowActive="true"] { background:%9; color:%10; }
      QFrame#folderPane[browserActive="false"][windowActive="true"] QLabel, QFrame#folderPane[browserActive="false"][windowActive="true"] QLineEdit { background:%9; color:%10; }
      QToolButton#sortControl { min-width:16px; max-width:16px; min-height:16px; max-height:16px; padding:0; background:%1; color:#A7ADB5; }
      QToolButton#sortControl:checked { color:#FFFFFF; }
      QToolButton#upButton, QToolButton#backButton, QToolButton#forwardButton, QToolButton#sortControl, QToolButton#newTabButton, QToolButton#closeTabButton, QToolButton#favoriteGroupMenuButton, QToolButton#addFavoriteItemButton, QToolButton#newGroupButton { border:none; }
      QWidget#browserTabsStrip { background:%1; }
      QWidget#browser[browserActive="true"][windowActive="true"], QWidget#browserTabsStrip[browserActive="true"][windowActive="true"], QLineEdit#pathEditControl[browserActive="true"][windowActive="true"] { background:%2; }
      QWidget#browser[browserActive="false"][windowActive="true"], QLineEdit#pathEditControl[browserActive="false"][windowActive="true"] { background:%9; color:%10; }
      QWidget#browserTabsStrip[browserActive="false"][windowActive="true"] { background:%9; }
      QLineEdit#pathEditControl[pathError="true"] { border:1px solid #D9534F; }
      QLineEdit#pathEditControl[pathPending="true"] { font-style:italic; }
      QWidget#pathBreadcrumbRow[browserActive="true"][windowActive="true"], QWidget#pathColumns[browserActive="true"][windowActive="true"] { background:%2; }
      QWidget#pathBreadcrumbRow[browserActive="false"][windowActive="true"], QWidget#pathColumns[browserActive="false"][windowActive="true"] { background:%9; color:%10; }
      QScrollArea#breadcrumbScroll, QWidget#breadcrumbContents { background:transparent; border:0; }
      QToolButton#pathBreadcrumb { border:0; background:transparent; padding:2px 4px; }
      QToolButton#pathBreadcrumb:hover:enabled { background:%11; }
      QToolButton#expandPathButton, QToolButton#editPathButton { border:1px solid %4; border-radius:5px; background:%1; padding:0; }
      QToolButton#expandPathButton[pathExpanded="true"] { background:#173A55; border-color:%5; }
      QToolButton#expandPathButton:hover:enabled, QToolButton#editPathButton:hover:enabled { background:%11; border-color:%5; }
      QToolButton#expandPathButton:pressed:enabled, QToolButton#editPathButton:pressed:enabled { background:%5; }
      QToolButton#expandPathButton:focus, QToolButton#editPathButton:focus { border-color:%5; }
      QToolButton#pathBreadcrumb[pathPending="true"] { color:#75C7FF; font-style:italic; }
      QLabel#pathPendingTarget { color:#75C7FF; font-style:italic; }
      QListWidget#pathColumn { background:%2; color:%3; alternate-background-color:#2B3139; border:1px solid %4; outline:none; }
      QListWidget#pathColumn::item { padding:3px 6px; }
      QListWidget#pathColumn::item:hover:!selected { background:#354553; color:#FFFFFF; }
      QListWidget#pathColumn::item:selected { background:%5; color:#FFFFFF; }
      QListWidget#pathColumn[browserActive="false"][windowActive="true"], QListWidget#pathColumn[windowActive="false"] { background:%9; color:#BDC6D0; alternate-background-color:#22272D; }
      QListWidget#pathColumn[browserActive="false"][windowActive="true"]::item:selected, QListWidget#pathColumn[windowActive="false"]::item:selected { background:#173A55; color:#FFFFFF; }
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
      QListWidget#settingsCategoryList { background:%1; color:%3; border:0; outline:none; padding:8px 4px; }
      QListWidget#settingsCategoryList::item { padding:10px 12px; margin:2px 0; border-radius:4px; }
      QListWidget#settingsCategoryList::item:hover:!selected { background:%11; }
      QListWidget#settingsCategoryList::item:selected { background:%5; color:white; }
      QWidget#settingsContent, QStackedWidget#settingsPages { background:%2; border:0; }
      QWidget#settingsPage { background:%2; color:%3; }
      QStackedWidget#fileAssociationDetailStack, QScrollArea#fileAssociationDetailScroll, QScrollArea#fileAssociationDetailScroll QWidget#qt_scrollarea_viewport, QWidget#fileAssociationDetail, QWidget#fileAssociationLoading { background:%2; border:0; }
      QLabel#fileAssociationColumnHeading { background:transparent; color:#B7C0CA; font-size:10px; font-weight:600; }
      QWidget#fileAssociationApplicationDescription, QWidget#fileAssociationApplicationDescription QLabel { background:transparent; border:0; }
      QLabel#fileAssociationApplicationPath { color:#9CA5AF; font-size:11px; }
      QLabel#fileAssociationWaitingText { background:transparent; color:#B7C0CA; }
      QProgressBar#fileAssociationLoadingIndicator { background:%1; border:1px solid %4; border-radius:3px; min-height:6px; max-height:6px; }
      QProgressBar#fileAssociationLoadingIndicator::chunk { background:%5; border-radius:2px; }
      QFrame#settingsCard { background:%1; border:1px solid %4; border-radius:6px; }
      QLabel#settingsCardHeading { background:transparent; color:%3; font-size:14px; font-weight:600; }
      QWidget#settingsShortcutPage { background:%2; color:%3; }
      QWidget#settingsShortcutPage QLabel, QWidget#settingsShortcutPage QToolButton, QWidget#settingsShortcutPage QPushButton { color:%3; font-size:10px; }
      QWidget#shortcutTableHeader { background:%1; border-bottom:1px solid %4; }
      QLabel#shortcutColumnHeading { background:transparent; color:#B7C0CA; font-size:10px; font-weight:600; }
      QWidget#settingsShortcutList, QScrollArea#settingsShortcutScroll QWidget#qt_scrollarea_viewport { background:%2; color:%3; }
      QFrame#shortcutGroup { background:%2; border:1px solid %4; border-radius:4px; }
      QWidget#settingsShortcutPage QLabel#shortcutGroupHeading { background:transparent; color:%3; border:0; padding:0; font-size:11px; font-weight:bold; }
      QLabel#shortcutColumnHeadingWide { background:transparent; color:#B7C0CA; font-size:13px; font-weight:600; }
      QWidget#shortcutRow { background:%2; border-bottom:1px solid %4; }
      QWidget#shortcutRow[alternate="true"] { background:#272B31; }
      QWidget#shortcutRow:hover { background:%11; }
      QWidget#shortcutRow QWidget, QWidget#shortcutRow QLabel { background:transparent; border:0; color:%3; }
      QLabel#shortcutScope, QLabel#shortcutEmpty { color:#B7C0CA; }
      QWidget#shortcutRow QLabel#shortcutOriginal { color:#9CA5AF; font-size:11px; }
      QWidget#shortcutRow QToolButton { background:transparent; color:#9CA5AF; border:0; padding:0; }
      QWidget#shortcutRow QToolButton:hover:enabled, QWidget#shortcutRow QToolButton:focus { background:transparent; color:#A9D8FA; border:0; }
      QLabel#shortcutFeedback { background:%2; color:%8; font-size:10px; padding:2px 8px 4px; }
      QLabel#shortcutFeedback[capturePrompt="true"] { color:#A9D8FA; }
      QScrollArea#settingsShortcutScroll { background:%2; border:0; }
      QScrollArea#settingsShortcutScroll QScrollBar { background:%2; border:0; }
      QScrollArea#settingsShortcutScroll QScrollBar::handle { background:%4; border-radius:4px; min-height:24px; }
      QScrollArea#settingsShortcutScroll QScrollBar::add-line, QScrollArea#settingsShortcutScroll QScrollBar::sub-line { width:0; height:0; }
      QWidget#settingsContextMenuPage { background:%2; color:%3; }
      QWidget#settingsContextMenuPage QLabel, QWidget#settingsContextMenuPage QCheckBox, QWidget#settingsContextMenuPage QPushButton { color:%3; font-size:10px; }
      QWidget#contextMenuTableHeader { background:%1; border-bottom:1px solid %4; }
      QLabel#contextMenuColumnHeading { background:transparent; color:#B7C0CA; font-size:10px; font-weight:600; }
      QWidget#settingsContextMenuList, QScrollArea#settingsContextMenuScroll QWidget#qt_scrollarea_viewport { background:%2; color:%3; }
      QFrame#contextMenuGroup { background:%2; border:1px solid %4; border-radius:4px; }
      QWidget#settingsContextMenuPage QLabel#contextMenuGroupHeading { background:transparent; color:%3; border:0; padding:0; font-size:11px; font-weight:bold; }
      QWidget#contextMenuRow { background:%2; border-bottom:1px solid %4; }
      QWidget#contextMenuRow[alternate="true"] { background:#272B31; }
      QWidget#contextMenuRow:hover { background:%11; }
      QWidget#contextMenuRow QWidget, QWidget#contextMenuRow QLabel, QWidget#contextMenuRow QCheckBox { background:transparent; border:0; color:%3; }
      QLabel#contextMenuShortcutNone { color:#9CA5AF; }
      QLabel#contextMenuFeedback { background:%2; color:%8; font-size:10px; padding:4px 8px; }
      QScrollArea#settingsContextMenuScroll { background:%2; border:0; }
      QScrollArea#settingsContextMenuScroll QScrollBar { background:%2; border:0; }
      QScrollArea#settingsContextMenuScroll QScrollBar::handle { background:%4; border-radius:4px; min-height:24px; }
      QScrollArea#settingsContextMenuScroll QScrollBar::add-line, QScrollArea#settingsContextMenuScroll QScrollBar::sub-line { width:0; height:0; }
      QDialog#settingsWindow QCheckBox, QDialog#settingsWindow QPushButton { color:%3; }
      QDialog#settingsWindow QPushButton { background:%2; border:1px solid %4; border-radius:4px; padding:6px 12px; }
      QDialog#settingsWindow QPushButton:hover:enabled { background:%11; }
      QDialog#settingsWindow QPushButton:focus { border-color:%5; }
      QMenu#settingsInfoMenu { background:%1; color:%3; border:1px solid %4; }
      QMenu#settingsInfoMenu QLabel { background:transparent; color:%3; font-size:12px; }
      QMessageBox#settingsResetConfirmation, QMessageBox#applicationQuitConfirmationWindow { background:%1; color:%3; }
      QMessageBox#settingsResetConfirmation QLabel, QMessageBox#applicationQuitConfirmationWindow QLabel, QMessageBox#applicationQuitConfirmationWindow QCheckBox { background:transparent; color:%3; }
      QMessageBox#settingsResetConfirmation QPushButton, QMessageBox#applicationQuitConfirmationWindow QPushButton { background:%2; color:%3; border:1px solid %4; border-radius:4px; padding:6px 12px; }
      QMessageBox#applicationQuitConfirmationWindow QPushButton:default { border-color:%5; }
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

auto make_path_action_icon(const PathActionIcon action, const bool focused) -> QIcon {
    QIcon icon;
    const auto draw = [action](const QColor &color, qreal scale) {
        QPixmap pixmap(QSize(static_cast<int>(20 * scale), static_cast<int>(20 * scale)));
        pixmap.setDevicePixelRatio(scale);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing);
        QPen pen(color);
        pen.setWidthF(1.7);
        pen.setCapStyle(Qt::RoundCap);
        pen.setJoinStyle(Qt::RoundJoin);
        painter.setPen(pen);
        if (action == PathActionIcon::Edit) {
            painter.drawLine(QPointF(4, 14), QPointF(13, 5));
            painter.drawLine(QPointF(7, 17), QPointF(16, 8));
            painter.drawLine(QPointF(4, 14), QPointF(7, 17));
            painter.drawLine(QPointF(13, 5), QPointF(16, 8));
            painter.drawLine(QPointF(3, 18), QPointF(4, 14));
            painter.drawLine(QPointF(3, 18), QPointF(7, 17));
            painter.drawLine(QPointF(12, 6), QPointF(15, 9));
        } else {
            painter.drawRoundedRect(QRectF(2.5, 2.5, 15, 11), 1.5, 1.5);
            painter.drawLine(QPointF(3.5, 6), QPointF(16.5, 6));
            painter.drawLine(QPointF(8, 7), QPointF(8, 13));
            painter.drawLine(QPointF(12, 7), QPointF(12, 13));
            const qreal point = action == PathActionIcon::Expand ? 18.0 : 15.5;
            const qreal tail = action == PathActionIcon::Expand ? 15.5 : 18.0;
            painter.drawLine(QPointF(7, tail), QPointF(10, point));
            painter.drawLine(QPointF(10, point), QPointF(13, tail));
        }
        return pixmap;
    };
    const auto add_variants = [&icon, &draw](const QColor &color, QIcon::Mode mode) {
        icon.addPixmap(draw(color, 1.0), mode);
        icon.addPixmap(draw(color, 2.0), mode);
    };
    add_variants(QColor(focused ? QStringLiteral("#D5DCE5") : QString::fromLatin1(inactive_text_color)), QIcon::Normal);
    add_variants(QColor(focused ? QStringLiteral("#FFFFFF") : QStringLiteral("#A7ADB5")), QIcon::Active);
    add_variants(QColor(QStringLiteral("#6F757D")), QIcon::Disabled);
    return icon;
}

auto path_action_icon(const PathActionIcon action, const bool focused) -> const QIcon & {
    static const std::array<QIcon, 3> active = {make_path_action_icon(PathActionIcon::Expand, true), make_path_action_icon(PathActionIcon::Collapse, true), make_path_action_icon(PathActionIcon::Edit, true)};
    static const std::array<QIcon, 3> inactive = {make_path_action_icon(PathActionIcon::Expand, false), make_path_action_icon(PathActionIcon::Collapse, false), make_path_action_icon(PathActionIcon::Edit, false)};
    return (focused ? active : inactive).at(static_cast<std::size_t>(action));
}

/// Whether `widget` is `ancestor` or inside it.
auto inside(const QWidget *widget, const QWidget *ancestor) -> bool {
    return widget != nullptr && (widget == ancestor || ancestor->isAncestorOf(widget));
}

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
