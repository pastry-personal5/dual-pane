#include "dual_pane_desktop/desktop_checks.hpp"
#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/clipboard_adapter.hpp"
#include "dual_pane_desktop/favorites_panel.hpp"
#include "dual_pane_desktop/folder_pane.hpp"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/operation_panels.hpp"
#include "dual_pane_desktop/settings_window.hpp"
#include "dual_pane_desktop/shortcuts.hpp"
#include "dual_pane_desktop/tabs_strip.hpp"
#include "dual_pane_desktop/toolbar_sidebar.hpp"
#include "dual_pane_desktop/workspace_window.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
// QtGui's exported entry to its shortcut map. QtTest delivers keys through it
// as the platform layer does; the text editor focus check uses it the same way.
QT_BEGIN_NAMESPACE
Q_GUI_EXPORT auto qt_sendShortcutOverrideEvent(QObject *receiver, ulong timestamp, int key, Qt::KeyboardModifiers modifiers, const QString &text, bool auto_repeat, ushort count) -> bool;
QT_END_NAMESPACE
namespace dual_pane_desktop::gui {
namespace {
struct PathAnnouncementCapture {
    QObject *object = nullptr;
    QString message;
};
auto path_announcement_capture() -> PathAnnouncementCapture & {
    static PathAnnouncementCapture capture;
    return capture;
}
void record_path_check_announcement(QAccessibleEvent *event) {
    if (event->type() == QAccessible::Announcement && event->object() == path_announcement_capture().object)
        if (auto *announcement = dynamic_cast<QAccessibleAnnouncementEvent *>(event))
            path_announcement_capture().message = announcement->message();
}
} // namespace

void schedule_path_entry_checks(QMainWindow &window, WorkspaceBridge &bridge, const std::array<Browser *, 2> &browsers, FolderItemsListModel &left_model) {
    const QString root = qEnvironmentVariable("DUAL_PANE_INTERNAL_PATH_ENTRY_ROOT");
    const QString start = root + QStringLiteral("/Start");
    const QString stored = root + QStringLiteral("/Stored");
    const QString missing = root + QStringLiteral("/Missing");
    auto *path = browsers.at(0)->path_control();
    auto *right_path = browsers.at(1)->path_control();
    auto *list = browsers.at(0)->view();
    auto *folder = browsers.at(0)->folder();
    auto *status = folder->status();
    auto *status_summary = status->findChild<QWidget *>(QStringLiteral("browserStatusSummary"));
    auto *status_count = status->findChild<QLabel *>(QStringLiteral("browserStatusCount"));
    auto *status_selected = status->findChild<QLabel *>(QStringLiteral("browserStatusSelected"));
    auto *status_size = status->findChild<QLabel *>(QStringLiteral("browserStatusSize"));
    auto *status_message = status->findChild<QLabel *>(QStringLiteral("browserStatusMessage"));
    auto *summary_count = folder->findChild<QLabel *>(QStringLiteral("summaryCount"));
    auto *summary_selected = folder->findChild<QLabel *>(QStringLiteral("summarySelected"));
    auto *summary_size = folder->findChild<QLabel *>(QStringLiteral("summarySize"));
    const auto status_matches_summary = [=] {
        return status_summary && status_count && status_selected && status_size && status_message && summary_count && summary_selected && summary_size && status_summary->isVisible() && !status_message->isVisible() && status_count->text() == summary_count->text() && status_selected->text() == summary_selected->text() && status_size->text() == summary_size->text() && status_selected->styleSheet() == summary_selected->styleSheet();
    };
    const auto path_icons_match_focus = [browsers] {
        return std::all_of(browsers.cbegin(), browsers.cend(), [](Browser *browser) {
            auto *expand = browser->findChild<QToolButton *>(QStringLiteral("expandPathButton"));
            auto *edit = browser->findChild<QToolButton *>(QStringLiteral("editPathButton"));
            if (!expand || !edit)
                return false;
            const bool focused = expand->property("browserActive").toBool() && expand->property("windowActive").toBool();
            return expand->icon().cacheKey() == path_action_icon(PathActionIcon::Expand, focused).cacheKey() && edit->icon().cacheKey() == path_action_icon(PathActionIcon::Edit, focused).cacheKey();
        });
    };
    auto *timer = new QTimer(&window);
    timer->setInterval(20);
    struct State {
        int step = 0;
        QElapsedTimer elapsed;
        QAccessible::UpdateHandler previous_handler = nullptr;
    };
    auto state = std::make_shared<State>();
    state->elapsed.start();
    path_announcement_capture().object = path;
    path_announcement_capture().message.clear();
    state->previous_handler = QAccessible::installUpdateHandler(record_path_check_announcement);
    const auto finish = [state, timer](bool passed, const char *reason) {
        timer->stop();
        QAccessible::installUpdateHandler(state->previous_handler);
        path_announcement_capture().object = nullptr;
        if (!passed)
            std::cerr << "Path entry check failed: " << reason << '\n';
        QApplication::exit(passed ? 0 : 1);
    };
    const auto type_path = [path, browser = browsers.at(0)](const QString &value) {
        browser->edit_path();
        path->setFocus(Qt::OtherFocusReason);
        path->selectAll();
        QInputMethodEvent commit;
        commit.setCommitString(value);
        QApplication::sendEvent(path, &commit);
        return path->text() == value;
    };
    const auto enter = [path] {
        QKeyEvent key(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
        QApplication::sendEvent(path, &key);
    };
    const auto escape = [path] {
        QKeyEvent key(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
        QApplication::sendEvent(path, &key);
    };
    QObject::connect(timer, &QTimer::timeout, &window, [=, &bridge, &left_model, &window] {
        if (state->elapsed.elapsed() > 10000) {
            finish(false, "timed out waiting for the workspace");
            return;
        }
        switch (state->step) {
        case 0: {
            if (!window.isVisible() || left_model.getPathText() != start || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            left_model.selectRow(0);
            const auto selected = left_model.selectedRanges();
            if (selected.isEmpty()) {
                finish(false, "could not select the fixture item");
                return;
            }
            if (!path_icons_match_focus()) {
                finish(false, "path button icons do not match initial Browser focus");
                return;
            }
            if (left_model.getStatusText() == left_model.getPathText()) {
                if (!status_matches_summary()) {
                    finish(false, "default Browser Status Bar does not match the Folder Summary Row");
                    return;
                }
            } else {
                if (!status_summary || !status_message || !summary_count || !summary_selected || !summary_size || status_summary->isVisible() || !status_message->isVisible() || status->accessibleDescription() != left_model.getStatusText()) {
                    finish(false, "monitoring feedback did not replace the default Browser Status Bar summary");
                    return;
                }
                status->set_summary(summary_count->text(), summary_selected->text(), summary_size->text());
                if (!status_matches_summary()) {
                    finish(false, "Browser Status Bar cannot present the matching default summary");
                    return;
                }
                status->set_full_text(left_model.getStatusText());
            }
            auto *right_edit = browsers.at(1)->findChild<QToolButton *>(QStringLiteral("editPathButton"));
            if (!right_edit) {
                finish(false, "path Edit buttons are missing");
                return;
            }
            right_edit->setFocus(Qt::MouseFocusReason);
            right_edit->click();
            QApplication::processEvents();
            if (bridge.getActiveBrowser() != 1 || !right_path->editing() || right_path->selectedText() != right_path->text()) {
                finish(false, "Edit in the inactive Browser does not activate and select its path");
                return;
            }
            if (!path_icons_match_focus()) {
                finish(false, "path button icons did not follow the active Browser");
                return;
            }
            list->setFocus(Qt::OtherFocusReason);
            if (!browsers.at(0)->findChild<QToolButton *>(QStringLiteral("editPathButton")) || !QApplication::focusWidget()) {
                finish(false, "path Edit button or keyboard focus is missing");
                return;
            }
            bool reached = false;
            for (int index = 0; index < 100 && !reached; ++index) {
                QKeyEvent tab(QEvent::KeyPress, Qt::Key_Tab, Qt::NoModifier);
                QApplication::sendEvent(QApplication::focusWidget(), &tab);
                reached = QApplication::focusWidget() == browsers.at(0)->findChild<QToolButton *>(QStringLiteral("editPathButton"));
            }
            if (reached)
                browsers.at(0)->findChild<QToolButton *>(QStringLiteral("editPathButton"))->click();
            QApplication::processEvents();
            if (!reached || !path->editing() || path->selectedText() != start || bridge.getActiveBrowser() != 0 || left_model.selectedRanges() != selected) {
                finish(false, "Tab traversal does not reach Edit and open the Path Edit Control");
                return;
            }
            if (!path_icons_match_focus()) {
                finish(false, "path button icons did not restore after Browser focus returned");
                return;
            }
            left_model.goToParent();
            state->step = 1;
            return;
        }
        case 1:
            if (left_model.getPathText() != root || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            if (path->text() != start) {
                finish(false, "confirmed path update overwrote an unedited draft");
                return;
            }
            enter();
            QApplication::processEvents();
            if (path->editing() || QApplication::focusWidget() != list) {
                finish(false, "accepted unedited Return did not return focus to Folder Items");
                return;
            }
            state->step = 2;
            return;
        case 2:
            if (left_model.getPathText() != start || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            if (!type_path(missing)) {
                finish(false, "typed path did not reach the editor");
                return;
            }
            enter();
            if (left_model.getPathEntryState() != 1 || !path->isReadOnly() || !path->property("pathPending").toBool() || path->accessibleDescription() != QStringLiteral("Checking path") || left_model.getStatusText() != QStringLiteral("Loading…")) {
                finish(false, "pending path does not show its busy and accessible state");
                return;
            }
            browsers.at(1)->edit_path();
            state->step = 3;
            return;
        case 3:
            if (left_model.getPathEntryState() == 1)
                return;
            if (left_model.getPathEntryState() != 0 || path->text() != start || path->property("pathError").toBool()) {
                finish(false, "a background failure kept text after Browser focus left");
                return;
            }
            if (!type_path(missing)) {
                finish(false, "could not retry the missing path");
                return;
            }
            enter();
            state->step = 4;
            return;
        case 4:
            if (left_model.getPathEntryState() == 1)
                return;
            if (left_model.getPathEntryState() != 2 || !path->property("pathError").toBool() || path->accessibleName() != QStringLiteral("Path Edit Control") || !path->accessibleDescription().contains(QStringLiteral("no longer exists")) || !left_model.getStatusText().contains(QStringLiteral("no longer exists")) || path_announcement_capture().message != path->accessibleDescription() || left_model.getPathText() != start || path->text() != missing) {
                finish(false, "missing path did not keep the confirmed tab and announce its error");
                return;
            }
            if (!status_summary || !status_message || !status_message->isVisible() || status_summary->isVisible() || status->accessibleDescription() != left_model.getStatusText()) {
                finish(false, "path error did not replace the default Browser Status Bar summary");
                return;
            }
            escape();
            if (path->text() != start || QApplication::focusWidget() != list) {
                finish(false, "Escape did not restore the confirmed path and focus");
                return;
            }
            if (!type_path(stored)) {
                finish(false, "could not type a valid path after Escape");
                return;
            }
            enter();
            state->step = 5;
            return;
        case 5:
            if (left_model.getPathText() != stored || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            if (path->editing() || QApplication::focusWidget() != list || !left_model.getPathText().endsWith(QStringLiteral("/Stored"))) {
                finish(false, "successful path did not navigate and return focus");
                return;
            }
            if (left_model.getStatusText() == left_model.getPathText() && !status_matches_summary()) {
                finish(false, "Browser Status Bar did not restore its default summary");
                return;
            }
            if (!type_path(start)) {
                finish(false, "could not type a path to cancel");
                return;
            }
            enter();
            escape();
            if (left_model.getPathEntryState() != 0 || path->text() != stored) {
                finish(false, "Escape did not cancel the pending check");
                return;
            }
            state->elapsed.restart();
            state->step = 6;
            return;
        case 6:
            if (state->elapsed.elapsed() < 200)
                return;
            if (left_model.getPathText() != stored) {
                finish(false, "a cancelled check navigated late");
                return;
            }
            if (!QDir(stored).removeRecursively()) {
                finish(false, "could not remove the disposable folder fixture");
                return;
            }
            left_model.refreshFolder();
            state->step = 7;
            return;
        case 7:
            if (!left_model.getMissingFolder())
                return;
            list->setFocus(Qt::OtherFocusReason);
            if (!qt_sendShortcutOverrideEvent(list, 0, Qt::Key_L, Qt::ControlModifier, QString(), false, 1)) {
                QKeyEvent command_l(QEvent::KeyPress, Qt::Key_L, Qt::ControlModifier);
                QApplication::sendEvent(list, &command_l);
            }
            QApplication::processEvents();
            if (!path->editing() || QApplication::focusWidget() != path || path->selectedText() != stored) {
                finish(false, "Command+L did not open the path editor over the Missing Folder Overlay");
                return;
            }
            if (!type_path(start)) {
                finish(false, "could not enter an absolute path from the Missing Folder Overlay");
                return;
            }
            enter();
            state->step = 8;
            return;
        case 8:
            if (left_model.getPathText() != start || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            finish(!path->editing() && QApplication::focusWidget() == list, "absolute path did not recover the missing-folder tab");
            return;
        default:
            finish(false, "invalid check step");
        }
    });
    timer->start();
}

void schedule_path_columns_checks(QMainWindow &window, const std::array<Browser *, 2> &browsers, FolderItemsListModel &left_model, FolderItemsListModel &right_model) {
    const QString root = qEnvironmentVariable("DUAL_PANE_INTERNAL_PATH_COLUMNS_ROOT");
    const QString parent = root + QStringLiteral("/Parent");
    const QString current = parent + QStringLiteral("/Current");
    const QString sibling = parent + QStringLiteral("/Sibling");
    auto *timer = new QTimer(&window);
    timer->setInterval(20);
    struct State {
        int step = 0;
        QElapsedTimer elapsed;
        QString right_path;
        bool narrow_checked = false;
        int original_width = 0;
    };
    auto state = std::make_shared<State>();
    state->elapsed.start();
    const auto finish = [timer](bool passed, const char *reason) {
        timer->stop();
        if (!passed)
            std::cerr << "Path columns check failed: " << reason << '\n';
        QApplication::exit(passed ? 0 : 1);
    };
    const auto column_list = [browser = browsers.at(0)](int column) -> QListWidget * {
        for (auto *list : browser->findChildren<QListWidget *>(QStringLiteral("pathColumn")))
            if (list->property("columnIndex").toInt() == column)
                return list;
        return nullptr;
    };
    QObject::connect(timer, &QTimer::timeout, &window, [=, &left_model, &right_model, &window] {
        if (state->elapsed.elapsed() > 15000) {
            std::cerr << "Path columns check timed out at step " << state->step << '\n';
            finish(false, "timed out");
            return;
        }
        const int final_column = left_model.breadcrumbCount() - 2;
        switch (state->step) {
        case 0: {
            if (!window.isVisible() || left_model.getPathText() != current || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            state->right_path = right_model.getPathText();
            auto *folder = browsers.at(0)->folder();
            auto *row = folder->path_row();
            auto *display = folder->findChild<QWidget *>(QStringLiteral("pathBreadcrumbRow"));
            auto *editor = browsers.at(0)->path_control();
            auto *panel = folder->findChild<QWidget *>(QStringLiteral("pathColumns"));
            auto *expand = browsers.at(0)->findChild<QToolButton *>(QStringLiteral("expandPathButton"));
            auto *edit = browsers.at(0)->findChild<QToolButton *>(QStringLiteral("editPathButton"));
            auto *scroll = folder->findChild<QScrollArea *>(QStringLiteral("breadcrumbScroll"));
            auto *contents = folder->findChild<QWidget *>(QStringLiteral("breadcrumbContents"));
            if (!row || row->parentWidget() != folder || !display || display->parentWidget() != row || !editor || editor->parentWidget() != row || !panel || panel->parentWidget() != folder || !expand || expand->parentWidget() != display || !edit || edit->parentWidget() != display || !row->isVisible() || !display->isVisible() || editor->isVisible() || display->toolTip() != current) {
                finish(false, "full path and controls are not in Folder Pane Row #1");
                return;
            }
            const auto crumbs = contents ? contents->findChildren<QToolButton *>(QStringLiteral("pathBreadcrumb")) : QList<QToolButton *>{};
            if (!scroll || !contents || !contents->isVisible() || contents->height() <= 0 || crumbs.size() != left_model.breadcrumbCount() || !std::all_of(crumbs.cbegin(), crumbs.cend(), [](const QToolButton *crumb) { return crumb->isVisible() && !crumb->text().isEmpty(); })) {
                finish(false, "confirmed full path is not visibly rendered in Row #1");
                return;
            }
            if (!expand || !expand->isEnabled() || left_model.breadcrumbPath(final_column) != parent) {
                finish(false, "ancestor trail or Expand control is incorrect");
                return;
            }
            if (expand->toolButtonStyle() != Qt::ToolButtonIconOnly || edit->toolButtonStyle() != Qt::ToolButtonIconOnly || !expand->text().isEmpty() || !edit->text().isEmpty() || expand->icon().isNull() || edit->icon().isNull() || expand->iconSize() != QSize(20, 20) || edit->iconSize() != QSize(20, 20)) {
                finish(false, "Expand or Edit is missing its bitmap button treatment");
                return;
            }
            for (const auto action : {PathActionIcon::Expand, PathActionIcon::Collapse, PathActionIcon::Edit}) {
                if (path_action_icon(action, true).pixmap(QSize(20, 20), QIcon::Normal).toImage() == path_action_icon(action, false).pixmap(QSize(20, 20), QIcon::Normal).toImage()) {
                    finish(false, "active and inactive path icons have the same foreground");
                    return;
                }
            }
            const bool focused = expand->property("browserActive").toBool() && expand->property("windowActive").toBool();
            if (expand->icon().cacheKey() != path_action_icon(PathActionIcon::Expand, focused).cacheKey() || edit->icon().cacheKey() != path_action_icon(PathActionIcon::Edit, focused).cacheKey()) {
                finish(false, "Expand or Edit does not use its Browser focus tone");
                return;
            }
            expand->click();
            if (expand->icon().cacheKey() != path_action_icon(PathActionIcon::Collapse, focused).cacheKey() || expand->accessibleName() != QStringLiteral("Collapse path columns") || expand->toolTip() != QStringLiteral("Hide ancestor folder choices")) {
                finish(false, "Expand did not switch to its Collapse icon and feedback");
                return;
            }
            state->step = 1;
            return;
        }
        case 1: {
            if (left_model.columnState(final_column) == -1 || left_model.columnState(final_column) == 0)
                return;
            if (left_model.columnState(final_column) != 1) {
                finish(false, "the final ancestor column did not load");
                return;
            }
            auto *list = column_list(final_column);
            auto *panel = browsers.at(0)->findChild<QWidget *>(QStringLiteral("pathColumns"));
            if (!list || !panel || !panel->isVisible()) {
                finish(false, "expanded ancestor choices are not visible");
                return;
            }
            int sibling_row = -1;
            bool current_selected = false;
            bool link_seen = false;
            bool package_seen = false;
            for (int row = 0; row < left_model.columnChoiceCount(final_column); ++row) {
                const QString label = left_model.columnChoiceLabel(final_column, row);
                if (label == QStringLiteral("Sibling"))
                    sibling_row = row;
                if (label == QStringLiteral("Current"))
                    current_selected = left_model.columnChoiceSelected(final_column, row);
                if (label == QStringLiteral("link"))
                    link_seen = true;
                if (label == QStringLiteral("bundle.app"))
                    package_seen = true;
            }
            if (sibling_row < 0 || !current_selected || !link_seen || package_seen || left_model.columnChoicePath(final_column, sibling_row) != sibling) {
                finish(false, "folder choices, selected path, symlink, or package policy is incorrect");
                return;
            }
            if (!state->narrow_checked) {
                state->original_width = window.width();
                const int old_first = qMax(0, final_column + 1 - qMax(1, (browsers.at(0)->width() - 8) / 160));
                if (auto *oldest = column_list(old_first); oldest && old_first < final_column) {
                    oldest->setFocus(Qt::OtherFocusReason);
                    window.resize(650, window.height());
                    state->step = 7;
                    return;
                }
                state->narrow_checked = true;
            }
            auto *sibling_item = list->item(sibling_row);
            if (!sibling_item || sibling_item->data(Qt::UserRole).toInt() != sibling_row)
                return;
            const auto foreground = list->palette().color(QPalette::Text);
            const auto background = list->palette().color(QPalette::Base);
            if (!list->alternatingRowColors() || foreground.lightness() - background.lightness() < 80 || !sibling_item->flags().testFlag(Qt::ItemIsEnabled) || list->visualItemRect(sibling_item).height() <= 0 || !list->currentItem() || !list->currentItem()->isSelected()) {
                finish(false, "expanded sibling rows lack readable text or a visible selected path");
                return;
            }
            list->itemClicked(sibling_item);
            if (left_model.getPathText() != current || left_model.columnState(final_column) != -1) {
                finish(false, "option activation did not collapse while keeping the confirmed path pending");
                return;
            }
            state->step = 2;
            return;
        }
        case 2: {
            if (left_model.getPathText() != sibling || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            if (right_model.getPathText() != state->right_path) {
                finish(false, "left column navigation changed the other Browser");
                return;
            }
            auto *expand = browsers.at(0)->findChild<QToolButton *>(QStringLiteral("expandPathButton"));
            const bool focused = expand && expand->property("browserActive").toBool() && expand->property("windowActive").toBool();
            if (!expand || expand->icon().cacheKey() != path_action_icon(PathActionIcon::Expand, focused).cacheKey() || expand->accessibleName() != QStringLiteral("Expand path columns") || expand->toolTip() != QStringLiteral("Show ancestor folder choices")) {
                finish(false, "Expand did not restore its icon and feedback after navigation");
                return;
            }
            expand->click();
            state->step = 3;
            return;
        }
        case 3: {
            if (left_model.columnState(final_column) != 1)
                return;
            auto *list = column_list(final_column);
            if (!list) {
                finish(false, "column did not reappear");
                return;
            }
            list->setFocus(Qt::OtherFocusReason);
            QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
            QApplication::sendEvent(list, &escape);
            if (left_model.columnState(final_column) != -1) {
                finish(false, "Escape did not collapse columns");
                return;
            }
            state->step = 4;
            return;
        }
        case 4: {
            if (QApplication::focusWidget() != browsers.at(0)->view())
                return;
            if (left_model.getPathText() != sibling)
                return;
            for (auto *button : browsers.at(0)->findChildren<QToolButton *>(QStringLiteral("pathBreadcrumb"))) {
                if (button->accessibleDescription() == parent) {
                    button->click();
                    state->step = 5;
                    return;
                }
            }
            finish(false, "parent breadcrumb is missing");
            return;
        }
        case 5: {
            if (left_model.getPathText() != parent || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            for (auto *button : browsers.at(0)->findChildren<QToolButton *>(QStringLiteral("pathBreadcrumb"))) {
                if (button->accessibleDescription() == QStringLiteral("/")) {
                    button->click();
                    state->step = 6;
                    return;
                }
            }
            finish(false, "root breadcrumb is missing");
            return;
        }
        case 6:
            if (left_model.getPathText() != QStringLiteral("/") || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            if (auto *expand = browsers.at(0)->findChild<QToolButton *>(QStringLiteral("expandPathButton")); expand && !expand->isEnabled()) {
                finish(true, "");
                return;
            }
            finish(false, "Expand remains enabled at root");
            return;
        case 7: {
            const int total = final_column + 1;
            const int expected_count = qMin(total, qMax(1, (browsers.at(0)->width() - 8) / 160));
            const int expected_first = total - expected_count;
            const auto lists = browsers.at(0)->findChildren<QListWidget *>(QStringLiteral("pathColumn"));
            if (window.width() >= state->original_width || expected_first == 0 || lists.size() != expected_count || !column_list(expected_first) || !column_list(final_column))
                return;
            if (auto *focused = qobject_cast<QListWidget *>(QApplication::focusWidget()); focused && focused->property("columnIndex").toInt() < expected_first) {
                finish(false, "resize left focus in a hidden ancestor column");
                return;
            }
            state->narrow_checked = true;
            state->step = 1;
            return;
        }
        default:
            finish(false, "invalid check step");
        }
    });
    timer->start();
}

void schedule_file_drag_drop_checks(QMainWindow &window, const std::array<Browser *, 2> &browsers, FolderItemsListModel &left_model, FolderItemsListModel &right_model) {
    const QString root = qEnvironmentVariable("DUAL_PANE_INTERNAL_FILE_DRAG_DROP_ROOT");
    const QString source = root + QStringLiteral("/Source");
    const QString destination = root + QStringLiteral("/Destination");
    auto *timer = new QTimer(&window);
    timer->setInterval(20);
    struct State {
        int step = 0;
        std::uint64_t drag = 0;
        std::uint64_t inactive_tab = 0;
        QElapsedTimer elapsed;
    };
    auto state = std::make_shared<State>();
    state->elapsed.start();
    const auto finish = [state, timer](bool passed, const char *reason) {
        timer->stop();
        if (!passed)
            std::cerr << "File drag and drop check failed: " << reason << '\n';
        QApplication::exit(passed ? 0 : 1);
    };
    const auto row_named = [](FolderItemsListModel &model, const QString &name) {
        for (int row = 0; row < model.rowCount(QModelIndex()); ++row)
            if (model.index(row, 1, QModelIndex()).data(Qt::DisplayRole).toString() == name)
                return row;
        return -1;
    };
    QObject::connect(timer, &QTimer::timeout, &window, [=, &left_model, &right_model, &window] {
        if (state->elapsed.elapsed() > 15000) {
            finish(false, "timed out waiting for the workspace or operation");
            return;
        }
        switch (state->step) {
        case 0:
            if (!window.isVisible() || left_model.getPathText() != source || left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            right_model.beginPathEntry();
            right_model.submitPath(right_model.activeTabId(), destination, true);
            state->step = 1;
            return;
        case 1: {
            if (right_model.getPathText() != destination || right_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            const int copy_row = row_named(left_model, QStringLiteral("copy-me"));
            if (copy_row < 0) {
                finish(false, "source listing did not contain copy-me");
                return;
            }
            left_model.selectRow(copy_row);
            state->drag = left_model.beginFileDrag(left_model.activeTabId(), left_model.visibleRevision());
            const auto payload = file_drag_payload(state->drag);
            QMimeData exact;
            exact.setData(QString::fromLatin1(file_drag_mime), payload);
            QMimeData malformed;
            malformed.setData(QString::fromLatin1(file_drag_mime), payload + QByteArray(1, '\0'));
            if (state->drag == 0 || payload.size() != 9 || decode_file_drag_payload(&exact) != state->drag || decode_file_drag_payload(&malformed).has_value() || payload.contains("copy-me")) {
                finish(false, "private payload was not a path-free exact versioned token");
                return;
            }
            if (right_model.previewFileDrop(state->drag, 1, 0, right_model.activeTabId(), -1, false) >= 0 || right_model.previewFileDrop(state->drag, 0, 0, right_model.activeTabId(), -1, false) != 1 || right_model.previewFileDrop(state->drag, 0, 0, right_model.activeTabId(), -1, true) != 1) {
                finish(false, "Browser-folder hover did not update Move and Option-Copy");
                return;
            }
            if (!right_model.acceptFileDrop(state->drag, 0, 0, right_model.activeTabId(), -1, true) || right_model.acceptFileDrop(state->drag, 0, 0, right_model.activeTabId(), -1, true)) {
                finish(false, "drop token was not accepted exactly once");
                return;
            }
            state->step = 2;
            return;
        }
        case 2:
            if (!QFileInfo::exists(destination + QStringLiteral("/copy-me")))
                return;
            if (!QFileInfo::exists(source + QStringLiteral("/copy-me"))) {
                finish(false, "Copy removed its source");
                return;
            }
            left_model.refreshFolder();
            state->step = 3;
            return;
        case 3: {
            if (left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            const int move_row = row_named(left_model, QStringLiteral("move-me"));
            if (move_row < 0)
                return;
            left_model.selectRow(move_row);
            state->drag = left_model.beginFileDrag(left_model.activeTabId(), left_model.visibleRevision());
            if (state->drag == 0 || !right_model.acceptFileDrop(state->drag, 0, 0, right_model.activeTabId(), -1, false)) {
                finish(false, "Move drop was not accepted");
                return;
            }
            state->step = 4;
            return;
        }
        case 4:
            if (!QFileInfo::exists(destination + QStringLiteral("/move-me")) || QFileInfo::exists(source + QStringLiteral("/move-me")))
                return;
            left_model.refreshFolder();
            right_model.refreshFolder();
            state->step = 5;
            return;
        case 5: {
            if (left_model.getStatusText() == QStringLiteral("Loading…") || right_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            const int item = row_named(left_model, QStringLiteral("row-target"));
            const int folder = row_named(right_model, QStringLiteral("Folder"));
            const int file = row_named(right_model, QStringLiteral("copy-me"));
            if (item < 0 || folder < 0 || file < 0)
                return;
            left_model.selectRow(item);
            state->drag = left_model.beginFileDrag(left_model.activeTabId(), left_model.visibleRevision());
            if (right_model.previewFileDrop(state->drag, 0, 1, right_model.activeTabId(), folder, false) != 1 || right_model.previewFileDrop(state->drag, 0, 1, right_model.activeTabId(), file, false) >= 0) {
                finish(false, "folder-row eligibility did not distinguish folders and files");
                return;
            }
            state->inactive_tab = right_model.activeTabId();
            right_model.newTab();
            if (right_model.activeTabId() == state->inactive_tab || right_model.previewFileDrop(state->drag, 0, 2, state->inactive_tab, -1, false) != 1) {
                finish(false, "inactive-tab target activated or lost stable identity");
                return;
            }
            left_model.cancelFileDrag(state->drag);
            if (right_model.previewFileDrop(state->drag, 0, 2, state->inactive_tab, -1, false) >= 0) {
                finish(false, "cancelled token remained usable");
                return;
            }
            state->step = 6;
            return;
        }
        case 6: {
            const int item = row_named(left_model, QStringLiteral("row-target"));
            if (item < 0)
                return;
            left_model.selectRow(item);
            const auto stale = left_model.beginFileDrag(left_model.activeTabId(), left_model.visibleRevision());
            left_model.refreshFolder();
            state->drag = stale;
            state->step = 7;
            return;
        }
        case 7:
            if (left_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            if (right_model.previewFileDrop(state->drag, 0, 2, state->inactive_tab, -1, false) >= 0) {
                finish(false, "a source revision change did not invalidate the drag");
                return;
            }
            right_model.activateTab(state->inactive_tab);
            state->elapsed.restart();
            state->step = 8;
            return;
        case 8: {
            if (right_model.activeTabId() != state->inactive_tab || right_model.getStatusText() == QStringLiteral("Loading…"))
                return;
            const int item = row_named(right_model, QStringLiteral("move-me"));
            if (item < 0) {
                finish(false, "right Browser source did not contain move-me");
                return;
            }
            right_model.selectRow(item);
            state->drag = right_model.beginFileDrag(right_model.activeTabId(), right_model.visibleRevision());
            if (state->drag == 0 || !left_model.acceptFileDrop(state->drag, 1, 0, left_model.activeTabId(), -1, true)) {
                finish(false, "right-to-left Copy drop was not accepted");
                return;
            }
            state->step = 9;
            return;
        }
        case 9:
            if (!QFileInfo::exists(source + QStringLiteral("/move-me")))
                return;
            finish(QFileInfo::exists(destination + QStringLiteral("/move-me")), "right-to-left Copy removed its source");
            return;
        default:
            finish(false, "invalid check step");
        }
    });
    timer->start();
}

/// Exercises the owned non-blocking native menu lifecycle without starting a
/// workspace or observing the global clipboard.
auto run_folder_items_context_menu_checks(QApplication &app) -> int {
    (void)app;
    QWidget owner;
    owner.resize(320, 200);
    owner.show();
    bool failed = false;
    int completions = 0;
    std::optional<int> last_choice;
    int observed_state = 0;
    const auto check = [&failed](bool condition, const char *name) {
        if (!condition) {
            std::cerr << "Folder Items Context Menu check failed: " << name << '\n';
            failed = true;
        }
    };
    FolderItemsListModel left_model, right_model;
    right_model.setRightBrowser();
    IconLoader icons;
    FolderItemsList left(&left_model, &icons), right(&right_model, &icons);
    left.setParent(&owner);
    right.setParent(&owner);
    std::vector<std::pair<int, bool>> invocations;
    left.setMenuHandler([&invocations](const QPoint &, int row, bool comprehensive) { invocations.emplace_back(row, comprehensive); });
    right.setMenuHandler([&invocations](const QPoint &, int row, bool comprehensive) { invocations.emplace_back(row, comprehensive); });
    const QPoint local(4, 4);
    QContextMenuEvent exact_shift(QContextMenuEvent::Mouse, local, left.viewport()->mapToGlobal(local), Qt::ShiftModifier);
    QApplication::sendEvent(left.viewport(), &exact_shift);
    QContextMenuEvent extra_modifier(QContextMenuEvent::Mouse, local, left.viewport()->mapToGlobal(local), Qt::ShiftModifier | Qt::ControlModifier);
    QApplication::sendEvent(left.viewport(), &extra_modifier);
    QContextMenuEvent keyboard(QContextMenuEvent::Keyboard, local, right.viewport()->mapToGlobal(local), Qt::NoModifier);
    QApplication::sendEvent(right.viewport(), &keyboard);
    QContextMenuEvent accessibility(QContextMenuEvent::Other, local, right.viewport()->mapToGlobal(local), Qt::ShiftModifier);
    QApplication::sendEvent(right.viewport(), &accessibility);
    check(invocations == std::vector<std::pair<int, bool>>({{-1, true}, {-1, false}, {-1, false}, {-1, true}}), "exact modifier, empty-space, keyboard, accessibility, and both-Browser routing");
    const auto check_context_rows = [&owner, &check](const std::vector<FolderItemsMenuSession::Row> &context_rows, const QStringList &expected, const char *name) {
        int completion_count = 0;
        auto *session = new FolderItemsMenuSession(QStringLiteral("Comprehensive Folder Items Context Menu"), context_rows, [&completion_count](std::optional<int>) { ++completion_count; }, &owner);
        session->popup(owner.mapToGlobal(QPoint(10, 10)));
        QStringList labels;
        for (const auto *action : session->menu()->actions())
            if (!action->isSeparator())
                labels.push_back(action->text().section('\t', 0, 0));
        check(labels == expected, name);
        session->dismiss_and_close();
        check(completion_count == 1, "context-shape dismissal");
    };
    check_context_rows({{QStringLiteral("Rename Item"), {}, false}, {QStringLiteral("New Folder"), {}, false}, {QStringLiteral("Open Item"), {}, true}, {QStringLiteral("Show Package Contents"), {}, false}, {QStringLiteral("Copy Items"), {}, true}}, {QStringLiteral("Rename Item"), QStringLiteral("New Folder"), QStringLiteral("Open Item"), QStringLiteral("Show Package Contents"), QStringLiteral("Copy Items")}, "package rows and section collapse");
    check_context_rows({{QStringLiteral("Rename Item"), {}, false}, {QStringLiteral("Open Item"), {}, true}, {QStringLiteral("Copy Items"), {}, true}}, {QStringLiteral("Rename Item"), QStringLiteral("Open Item"), QStringLiteral("Copy Items")}, "link rows exclude package-only action");
    check_context_rows({{QStringLiteral("Copy Items"), {}, false}, {QStringLiteral("Cut Items"), {}, false}, {QStringLiteral("Move to Trash"), {}, true}}, {QStringLiteral("Copy Items"), QStringLiteral("Cut Items"), QStringLiteral("Move to Trash")}, "multiple-selection rows exclude single-item actions");
    check_context_rows({{QStringLiteral("New Folder"), {}, false}}, {QStringLiteral("New Folder")}, "empty-space destination rows");
    const std::vector<FolderItemsMenuSession::Row> rows = {{QStringLiteral("Open Item"), QStringLiteral("Ctrl+Down"), false}, {QStringLiteral("Copy Items"), QStringLiteral("Ctrl+C"), true}, {QStringLiteral("Move to Trash"), QStringLiteral("Alt+Delete"), true}};
    const auto complete = [&completions, &last_choice, &observed_state](std::optional<int> choice) {
        ++completions;
        last_choice = choice;
        observed_state = 42;
    };

    auto *trigger_first = new FolderItemsMenuSession(QStringLiteral("Normal Folder Items Context Menu"), rows, complete, &owner);
    trigger_first->popup(owner.mapToGlobal(QPoint(20, 20)));
    auto *normal = trigger_first->menu();
    check(normal != nullptr && normal->isVisible(), "popup returns while Normal remains open");
    check(normal != nullptr && normal->accessibleName() == QStringLiteral("Normal Folder Items Context Menu"), "Normal accessible name");
    if (normal != nullptr) {
        const auto actions = normal->actions();
        check(actions.size() == 5 && actions.at(1)->isSeparator() && actions.at(3)->isSeparator(), "flat sections and collapsed separators");
        check(actions.at(0)->text().startsWith(QStringLiteral("Open Item\t")) && actions.at(2)->text().startsWith(QStringLiteral("Copy Items\t")), "native shortcut annotations");
    }
    QTimer::singleShot(0, &owner, [&, trigger_first] {
        auto *menu = trigger_first->menu();
        check(menu != nullptr && menu->isVisible() && completions == 0, "event loop resumes before menu completion");
        observed_state = 7;
        if (menu != nullptr) {
            menu->actions().at(2)->trigger();
            menu->close();
        }
        QTimer::singleShot(0, &owner, [&, trigger_first] {
            check(completions == 1 && last_choice == 1 && observed_state == 42, "trigger-before-hide dispatches once after closure and observes later state");
            trigger_first->dismiss_and_close();
            check(completions == 1, "completed session is idempotent");

            auto *hide_first = new FolderItemsMenuSession(QStringLiteral("Comprehensive Folder Items Context Menu"), rows, complete, &owner);
            hide_first->popup(owner.mapToGlobal(QPoint(30, 30)));
            auto *comprehensive = hide_first->menu();
            check(comprehensive != nullptr && comprehensive->accessibleName() == QStringLiteral("Comprehensive Folder Items Context Menu"), "Comprehensive accessible name");
            if (comprehensive != nullptr) {
                comprehensive->close();
                comprehensive->actions().at(4)->trigger();
            }
            QTimer::singleShot(0, &owner, [&, hide_first] {
                check(completions == 2 && last_choice == 2, "hide-before-trigger dispatches once on the queued finalizer");
                hide_first->dismiss_and_close();
                check(completions == 2, "second completed session is idempotent");

                auto *dismissed = new FolderItemsMenuSession(QStringLiteral("Normal Folder Items Context Menu"), rows, complete, &owner);
                dismissed->popup(owner.mapToGlobal(QPoint(40, 40)));
                dismissed->dismiss_and_close();
                dismissed->dismiss_and_close();
                check(completions == 3 && !last_choice.has_value(), "explicit dismissal completes exactly once");
                QApplication::exit(failed ? 1 : 0);
            });
        });
    });
    return QApplication::exec();
}

/// Exercises real Qt widgets without a display. The repository check script
/// launches this path with Qt's offscreen platform plugin.
auto run_settings_window_checks(QApplication &app) -> int {
    QMainWindow owner;
    owner.setStyleSheet(style_sheet());
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
        // Qt names the Command key Control on macOS.
        check(controller.shortcut()->key() == QKeySequence(Qt::CTRL | Qt::Key_Comma), "Command+, opens Settings");
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
        auto *reset = dialog->findChild<QPushButton *>(QStringLiteral("settingsReset"));
        check(categories && dialog->focusWidget() == categories && categories->currentRow() == 0, "initial category focus");
        check(categories && categories->count() == 3 && pages && pages->count() == 3, "three Settings categories and pages");
        if (categories && categories->count() == 3) {
            check(categories->item(0)->text() == QStringLiteral("General") && categories->item(1)->text() == QStringLiteral("Keyboard Shortcuts") && categories->item(2)->text() == QStringLiteral("Context Menus"), "concise Settings sidebar labels");
            check(categories->font().pixelSize() == 16, "larger Settings sidebar font");
        }
        check(title_row && heading && heading->text() == QStringLiteral("Settings") && heading->parentWidget() == title_row && notices && reset, "General controls and title row");
        check(macos_items && macos_items->accessibleName() == QStringLiteral("Show macOS metadata and service items") && macos_items->focusPolicy() != Qt::NoFocus, "accessible macOS items control");
        check(dotfiles && dotfiles->accessibleName() == QStringLiteral("Show dotfiles") && dotfiles->focusPolicy() != Qt::NoFocus, "accessible dotfiles control");
        auto *ask_quit = dialog->findChild<QCheckBox *>(QStringLiteral("settingsAskBeforeQuit"));
        check(ask_quit && ask_quit->accessibleName() == QStringLiteral("Ask before quitting from the close button") && ask_quit->isChecked() && ask_quit->focusPolicy() != Qt::NoFocus, "accessible quit confirmation control asks by default");
        check(macos_invisible && macos_invisible->accessibleName() == QStringLiteral("Show items marked invisible by macOS") && macos_invisible->focusPolicy() != Qt::NoFocus, "accessible macOS invisible control");
        check(dialog->findChildren<QLabel *>(QStringLiteral("settingsCardDescription")).isEmpty() && dialog->findChildren<QLabel *>(QStringLiteral("settingsVisibilityHelp")).isEmpty(), "General cards have no text above their controls");
        const auto general_cards = dialog->findChildren<QFrame *>(QStringLiteral("settingsCard"));
        int general_info_buttons = 0;
        bool info_at_right = !general_cards.isEmpty();
        for (auto *card : general_cards) {
            for (auto *button : card->findChildren<QToolButton *>()) {
                ++general_info_buttons;
                info_at_right = info_at_right && button->mapTo(card, button->rect().topRight()).x() == card->contentsRect().right() - card->layout()->contentsMargins().right();
            }
        }
        check(info_at_right && general_info_buttons == 6, "General information buttons sit at each card's right edge");
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
        const auto context_rows = dialog->findChildren<QWidget *>(QStringLiteral("contextMenuRow"));
        const auto context_groups = dialog->findChildren<QFrame *>(QStringLiteral("contextMenuGroup"));
        const auto normal_checks = dialog->findChildren<QCheckBox *>(QStringLiteral("contextMenuNormalCheck"));
        const auto comprehensive_checks = dialog->findChildren<QCheckBox *>(QStringLiteral("contextMenuComprehensiveCheck"));
        check(context_rows.size() == 12 && context_groups.size() == 5, "all categorized Context Menu rows");
        check(normal_checks.size() == 12 && comprehensive_checks.size() == 12, "independent Context Menu columns");
        check(std::count_if(normal_checks.cbegin(), normal_checks.cend(), [](const QCheckBox *check) { return check->isChecked(); }) == 7, "Normal factory configuration");
        check(std::all_of(comprehensive_checks.cbegin(), comprehensive_checks.cend(), [](const QCheckBox *check) { return check->isChecked(); }), "Comprehensive factory configuration");
        check(!normal_checks.isEmpty() && normal_checks.first()->accessibleName().contains(QStringLiteral("Normal Context Menu")), "accessible Context Menu checkbox");
        const auto is_information = [](const QToolButton *button) { return button->accessibleName().endsWith(QStringLiteral(" information")); };
        const auto information_ends_row = [&is_information](QWidget *row) {
            auto *cells = row->layout();
            auto *info_cell = row->findChild<QWidget *>(QStringLiteral("settingsInfoCell"), Qt::FindDirectChildrenOnly);
            if (!cells || !info_cell)
                return false;
            const auto row_buttons = row->findChildren<QToolButton *>();
            const auto cell_buttons = info_cell->findChildren<QToolButton *>();
            if (std::count_if(row_buttons.cbegin(), row_buttons.cend(), is_information) != 1 || cell_buttons.size() != 1 || !is_information(cell_buttons.first()))
                return false;
            int last_cell = -1;
            for (int position = 0; position < cells->count(); ++position) {
                if (cells->itemAt(position)->widget())
                    last_cell = position;
            }
            const int info_position = cells->indexOf(info_cell);
            return info_position > 0 && last_cell == info_position && cells->itemAt(info_position - 1)->spacerItem() != nullptr;
        };
        const auto shortcut_rows = dialog->findChildren<QWidget *>(QStringLiteral("shortcutRow"));
        check(std::all_of(shortcut_rows.cbegin(), shortcut_rows.cend(), information_ends_row), "Shortcut information buttons end each row after its free space");
        check(std::all_of(context_rows.cbegin(), context_rows.cend(), information_ends_row), "Context Menu information buttons end each row after its free space");
        const auto check_category_tables = [&check](QScrollArea *scroll, const QString &group_name, const QString &heading_name, const QString &column_heading_name, const char *fill_name, const char *heading_check_name, const char *heading_font_name) {
            QApplication::processEvents();
            auto *list = scroll ? scroll->widget() : nullptr;
            const auto groups = list ? list->findChildren<QFrame *>(group_name) : QList<QFrame *>();
            bool fills = list && !groups.isEmpty() && list->width() == scroll->viewport()->width();
            bool headings = fills;
            bool heading_fonts = fills;
            for (auto *group : groups) {
                fills = fills && group->width() == list->width();
                auto *section = group->parentWidget();
                auto *heading = section ? section->findChild<QLabel *>(heading_name, Qt::FindDirectChildrenOnly) : nullptr;
                QLabel *action_heading = nullptr;
                for (auto *label : group->findChildren<QLabel *>(column_heading_name)) {
                    if (label->text() == QStringLiteral("Action"))
                        action_heading = label;
                }
                heading_fonts = heading_fonts && heading && heading->font().pixelSize() == 11 && heading->font().weight() == QFont::Bold;
                headings = headings && heading && action_heading && !group->isAncestorOf(heading) && heading->geometry().bottom() < group->geometry().top() && heading->mapTo(list, heading->contentsRect().topLeft()).x() == action_heading->mapTo(list, QPoint(0, 0)).x();
            }
            check(fills, fill_name);
            check(headings, heading_check_name);
            check(heading_fonts, heading_font_name);
        };
        for (auto *button : dialog->findChildren<QToolButton *>()) {
            if (button->accessibleName() != QStringLiteral("Startup notices information"))
                continue;
            button->click();
            auto *help = button->findChild<QMenu *>();
            check(help && help->isVisible(), "anchored help opens");
            if (help) {
                const QImage popup = help->grab().toImage();
                const QColor surface(QStringLiteral("#2B323B"));
                const int middle = popup.height() / 2;
                bool one_box = popup.width() > 12 && QColor(popup.pixel(0, middle)) != surface;
                for (int x = 1; one_box && x <= 10; ++x)
                    one_box = QColor(popup.pixel(x, middle)) == surface;
                check(one_box, "information overlay draws one box");
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
            check_category_tables(dialog->findChild<QScrollArea *>(QStringLiteral("settingsShortcutScroll")), QStringLiteral("shortcutGroup"), QStringLiteral("shortcutGroupHeading"), QStringLiteral("shortcutColumnHeading"), "shortcut tables fill the page width", "shortcut category names sit above their tables, aligned with Action", "shortcut category names are 11px bold");
            categories->setCurrentRow(2);
            check(pages->currentIndex() == 2 && pages->currentWidget()->accessibleName() == QStringLiteral("Context Menus Settings"), "Context Menus category switching");
            check_category_tables(dialog->findChild<QScrollArea *>(QStringLiteral("settingsContextMenuScroll")), QStringLiteral("contextMenuGroup"), QStringLiteral("contextMenuGroupHeading"), QStringLiteral("contextMenuColumnHeading"), "Context Menu tables fill the page width", "Context Menu category names sit above their tables, aligned with Action", "Context Menu category names are 11px bold");
        }
        if (!normal_checks.isEmpty()) {
            auto *focused_check = normal_checks.first();
            focused_check->setFocus();
            check(QMetaObject::invokeMethod(&bridge, "contextMenusRevisionChanged", Qt::DirectConnection), "Context Menu revision notification");
            QApplication::processEvents();
            check(dialog->focusWidget() == focused_check && dialog->findChildren<QCheckBox *>(QStringLiteral("contextMenuNormalCheck")).first() == focused_check, "Context Menu updates preserve checkbox focus and identity");
        }
        const std::array<std::pair<QString, QString>, 3> reset_checks = {{{QStringLiteral("contextMenuResetNormal"), QStringLiteral("contextMenuResetNormalConfirmation")}, {QStringLiteral("contextMenuResetComprehensive"), QStringLiteral("contextMenuResetComprehensiveConfirmation")}, {QStringLiteral("contextMenuResetBoth"), QStringLiteral("contextMenuResetBothConfirmation")}}};
        for (const auto &[button_name, confirmation_name] : reset_checks) {
            auto *button = dialog->findChild<QPushButton *>(button_name);
            check(button && button->focusPolicy() != Qt::NoFocus, "accessible Context Menu reset button");
            if (!button)
                continue;
            button->click();
            auto *confirmation = dialog->findChild<QMessageBox *>(confirmation_name);
            check(confirmation && confirmation->isVisible() && confirmation->windowModality() == Qt::WindowModal && confirmation->text().contains(QStringLiteral("Context Menu")), "scope-specific Context Menu reset confirmation");
            if (confirmation) {
                auto *cancel = confirmation->button(QMessageBox::Cancel);
                check(cancel && confirmation->defaultButton() == cancel && confirmation->escapeButton() == cancel, "Context Menu reset Cancel default and Escape");
                if (cancel) {
                    cancel->click();
                    QApplication::processEvents();
                }
            }
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
        // Qt reports Command as ControlModifier on macOS, which capture encodes as flag 2.
        ShortcutCapture capture(&owner, [&submissions](int key, int flags) { ++submissions; return key == Qt::Key_A && flags == 2 ? QString() : QStringLiteral("Invalid"); }, [&capturing](bool active) { capturing = active; }, [](QWidget *) {}, &feedback);
        capture.show();
        capture.click();
        check(capturing, "capture starts");
        QKeyEvent invalid(QEvent::KeyPress, Qt::Key_B, Qt::NoModifier);
        QApplication::sendEvent(&capture, &invalid);
        check(capturing && submissions == 1 && feedback.text() == QStringLiteral("Invalid"), "invalid input stays in capture");
        QKeyEvent accepted(QEvent::KeyPress, Qt::Key_A, Qt::ControlModifier);
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

/// Exercises the keyboard focus rule for clipboard keys with real widgets and
/// no display. Keys reach Qt's shortcut map the way a platform key press does.
/// Folder Items commands only count, so nothing is published to the system
/// clipboard; Qt's offscreen platform keeps text copies in process, and the
/// check fails if the system clipboard changed anyway.
auto run_text_editor_focus_checks(QApplication &app) -> int {
    if (QGuiApplication::platformName() != QStringLiteral("offscreen")) {
        std::cerr << "Text editor focus check failed: it runs only on Qt's offscreen platform\n";
        return 1;
    }
    const auto system_generation = clipboard_generation();
    QWidget window;
    auto *layout = new QVBoxLayout(&window);
    FolderItemsListModel left_model, right_model;
    right_model.setRightBrowser();
    IconLoader icons;
    auto *left = new FolderItemsList(&left_model, &icons);
    auto *right = new FolderItemsList(&right_model, &icons);
    auto *filter = new FolderFilterTextEdit(&window);
    auto *rename = new FileNameEditor(QStringLiteral("renameItemEditor"), QStringLiteral("Rename Item Editor"), QString(), [] {}, [] {}, &window);
    auto *favorite = new InlineNameEditor(QStringLiteral("favoriteItemAliasEditor"), QStringLiteral("Favorite Item Alias Editor"), QString(), [](const QString &) { return QString(); }, [] {}, new QLabel(&window), &window);
    auto *path = new PathEditControl(&left_model, [left] { left->setFocus(Qt::OtherFocusReason); }, &window);
    auto *button = new QToolButton(&window);
    button->setFocusPolicy(Qt::StrongFocus);
    for (QWidget *widget : std::initializer_list<QWidget *>{left, right, filter, rename, favorite, path, button})
        layout->addWidget(widget);

    // Per Browser: Copy, Cut, and Paste Items.
    std::array<std::array<int, 3>, 2> list_commands{};
    int new_tabs = 0, quits = 0, edit_paths = 0;
    QHash<QString, ShortcutBinder::Handler> handlers;
    handlers.insert(QStringLiteral("NewTab"), [&new_tabs] { ++new_tabs; });
    handlers.insert(QStringLiteral("EditPath"), [&edit_paths] { ++edit_paths; });
    handlers.insert(QStringLiteral("QuitApplication"), [&quits] { ++quits; });
    QHash<QString, ShortcutBinder::ListHandler> list_handlers;
    const std::array<std::pair<QString, int>, 3> clipboard_commands = {{{QStringLiteral("CopyItems"), clipboard_copy}, {QStringLiteral("CutItems"), clipboard_cut}, {QStringLiteral("PasteItems"), clipboard_paste}}};
    for (const auto &[action, command] : clipboard_commands)
        list_handlers.insert(action, [&list_commands, command = command](int browser) { ++list_commands.at(browser).at(command); });
    WorkspaceBridge bridge;
    ShortcutBinder shortcuts(&window, &bridge, handlers, {left, right}, list_handlers);
    // The delivered defaults; `delivered_shortcuts_map_to_qt_key_sequences` pins them.
    shortcuts.rebind({{QStringLiteral("CopyItems"), QStringLiteral("Ctrl+C"), scope_folder_items_list}, {QStringLiteral("CutItems"), QStringLiteral("Ctrl+X"), scope_folder_items_list}, {QStringLiteral("PasteItems"), QStringLiteral("Ctrl+V"), scope_folder_items_list}, {QStringLiteral("NewTab"), QStringLiteral("Ctrl+T"), scope_window}, {QStringLiteral("EditPath"), QStringLiteral("Ctrl+L"), scope_window}, {QStringLiteral("QuitApplication"), QStringLiteral("Ctrl+Q"), scope_application}});
    QObject::connect(&app, &QApplication::focusChanged, &window, [&shortcuts](QWidget *, QWidget *now) { shortcuts.followFocus(now); });
    window.show();
    window.activateWindow();

    QTimer::singleShot(0, &app, [&] {
        bool failed = false;
        const auto check = [&failed](bool condition, const std::string &name) {
            if (!condition) {
                std::cerr << "Text editor focus check failed: " << name << '\n';
                failed = true;
            }
        };
        const auto focus = [](QWidget *widget) {
            widget->setFocus(Qt::OtherFocusReason);
            QApplication::processEvents();
            return QApplication::focusWidget() == widget;
        };
        // Qt names the Command key Control on macOS. The focused widget may
        // claim the key as a shortcut override; otherwise the shortcut map
        // runs, and a key no shortcut takes reaches the focused widget.
        const auto press = [](int key) {
            QWidget *target = QApplication::focusWidget();
            if (target == nullptr || qt_sendShortcutOverrideEvent(target, 0, key, Qt::ControlModifier, QString(), false, 1))
                return;
            QKeyEvent key_press(QEvent::KeyPress, key, Qt::ControlModifier);
            QApplication::sendEvent(target, &key_press);
            QKeyEvent key_release(QEvent::KeyRelease, key, Qt::ControlModifier);
            QApplication::sendEvent(target, &key_release);
        };
        QApplication::processEvents();
        auto *clipboard = QGuiApplication::clipboard();
        check(QApplication::activeWindow() == &window, "the check window is active");

        // Positive control: the same delivery reaches a focused list's commands.
        const auto clipboard_text = clipboard->text();
        check(focus(left), "the Left Folder Items List takes focus");
        press(Qt::Key_C);
        press(Qt::Key_X);
        press(Qt::Key_V);
        press(Qt::Key_T);
        check(list_commands.at(0) == std::array<int, 3>{1, 1, 1} && list_commands.at(1) == std::array<int, 3>{}, "Command+C, Command+X, and Command+V reach the focused list's Copy, Cut, and Paste Items");
        check(new_tabs == 1, "workspace shortcuts act from a Folder Items List");
        check(clipboard->text() == clipboard_text, "Copy Items writes no row text");
        check(focus(right), "the Right Folder Items List takes focus");
        press(Qt::Key_C);
        check(list_commands.at(1) == std::array<int, 3>{1, 0, 0} && list_commands.at(0) == std::array<int, 3>{1, 1, 1}, "only the focused list copies");

        const auto observed = list_commands;
        check(focus(button), "another control takes focus");
        press(Qt::Key_C);
        press(Qt::Key_V);
        check(list_commands == observed, "Folder Items commands need a focused Folder Items List");

        // Positive control: Command+L starts path editing from outside a text field.
        check(focus(button), "a button takes focus before Command+L");
        press(Qt::Key_L);
        check(edit_paths == 1, "Command+L reaches the Edit Path action outside a text field");

        const std::array<std::pair<QLineEdit *, std::string>, 4> editors = {{{filter, "filter row"}, {rename, "rename editor"}, {favorite, "Favorites name editor"}, {path, "Path Edit Control"}}};
        for (const auto &[editor, name] : editors) {
            check(edits_text(editor), name + " is a text field");
            editor->setText(QStringLiteral("alpha"));
            check(focus(editor), name + " takes focus");
            const int tabs = new_tabs;
            editor->selectAll();
            press(Qt::Key_C);
            check(clipboard->text() == QStringLiteral("alpha") && editor->text() == QStringLiteral("alpha"), name + ": Command+C copies text");
            press(Qt::Key_X);
            check(editor->text().isEmpty() && clipboard->text() == QStringLiteral("alpha"), name + ": Command+X cuts text");
            press(Qt::Key_V);
            press(Qt::Key_V);
            check(editor->text() == QStringLiteral("alphaalpha"), name + ": Command+V pastes text");
            press(Qt::Key_A);
            check(editor->selectedText() == editor->text(), name + ": Command+A selects text");
            press(Qt::Key_T);
            press(Qt::Key_L);
            check(list_commands == observed && new_tabs == tabs && edit_paths == 1, name + ": Folder Items and workspace shortcuts stand aside, including Command+L");
        }
        check(!edits_text(left) && !edits_text(button), "lists and buttons are not text fields");

        // The Path Edit Control: focus starts editing, Escape ends it and
        // hands focus to the list, and focus never stays on a control that is
        // not editing.
        path->setText(QStringLiteral("/confirmed"));
        check(focus(path) && path->editing(), "focusing the Path Edit Control starts editing");
        QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
        QApplication::sendEvent(path, &escape);
        QApplication::processEvents();
        check(!path->editing() && QApplication::focusWidget() == left, "Escape ends editing and focuses the Folder Items List");
        check(path->focusPolicy() == Qt::StrongFocus, "the Path Edit Control joins the Tab order");
        check(focus(path), "the Path Edit Control takes focus again");
        path->setText(QStringLiteral("/typed"));
        QKeyEvent enter(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
        QApplication::sendEvent(path, &enter);
        QApplication::processEvents();
        check(path->hasFocus() && path->editing(), "Return with no running workspace leaves the editor as it was");
        path->clear();
        QApplication::sendEvent(path, &enter);
        QApplication::processEvents();
        check(!path->editing(), "Return on empty text acts like Escape");

        // A rename waiting on its job is read-only; its text still copies,
        // and nothing else takes its keys.
        rename->setText(QStringLiteral("waiting"));
        rename->setReadOnly(true);
        check(focus(rename), "the waiting rename editor takes focus");
        rename->selectAll();
        press(Qt::Key_C);
        press(Qt::Key_X);
        press(Qt::Key_V);
        check(clipboard->text() == QStringLiteral("waiting") && rename->text() == QStringLiteral("waiting") && list_commands == observed, "a read-only rename editor copies and keeps its keys");

        // Composed text: a Folder Items List hands preedit and commit to its
        // handler unchanged, and the filter field's text, which drives the
        // query, changes once, on commit only.
        QString handed_preedit, handed_commit;
        int handed_events = 0;
        left->setInputMethodHandler([&](const QString &preedit, const QString &commit, const QList<QInputMethodEvent::Attribute> &) {
            handed_preedit = preedit;
            handed_commit = commit;
            ++handed_events;
        });
        check(focus(left), "the Left Folder Items List takes focus for composed text");
        QInputMethodEvent list_preedit(QStringLiteral("ni"), {});
        QApplication::sendEvent(left, &list_preedit);
        QInputMethodEvent list_commit(QString(), {});
        list_commit.setCommitString(QStringLiteral("你"));
        QApplication::sendEvent(left, &list_commit);
        check(handed_events == 2 && handed_commit == QStringLiteral("你") && handed_preedit.isEmpty(), "a Folder Items List hands composed text to its handler");
        filter->clear();
        int text_changes = 0;
        QObject::connect(filter, &QLineEdit::textChanged, filter, [&text_changes] { ++text_changes; });
        check(focus(filter), "the filter row takes focus for composed text");
        QInputMethodEvent preedit(QStringLiteral("ni"), {});
        QApplication::sendEvent(filter, &preedit);
        check(text_changes == 0 && filter->text().isEmpty(), "composition in the filter field leaves the query unchanged");
        QInputMethodEvent commit(QString(), {});
        commit.setCommitString(QStringLiteral("你"));
        QApplication::sendEvent(filter, &commit);
        check(text_changes == 1 && filter->text() == QStringLiteral("你"), "committed text changes the query exactly once");

        // Filter-row delivery: programmatic renders are signal-blocked, while
        // every user edit keeps the identity of the tab that supplied its
        // text. This seam has no running workspace, so it deliberately
        // models a tab switch between the edit and dispatch without asking a
        // current view which tab is active.
        FolderFilterTextEdit filter_row(&window);
        FilterRowState filter_state;
        int filter_user_edits = 0;
        std::uint64_t submitted_filter_tab = 0;
        QObject::connect(&filter_row, &QLineEdit::textChanged, &filter_row, [&](const QString &text) {
            filter_state.record_user_edit(text);
            submitted_filter_tab = filter_state.rendered_tab();
            ++filter_user_edits;
        });
        check(filter_state.render(41), "the first rendered filter tab is observed");
        {
            const QSignalBlocker blocker(&filter_row);
            filter_row.setText(QStringLiteral("alpha"));
        }
        check(filter_user_edits == 0 && filter_state.rendered_tab() == 41, "a programmatic filter render emits no user edit");
        filter_row.setText(QStringLiteral("al"));
        check(filter_user_edits == 1 && submitted_filter_tab == 41 && !filter_state.keeps_empty_row(), "a filter edit carries its rendered tab");
        check(filter_state.render(42), "a tab switch is observed before its replacement filter renders");
        {
            const QSignalBlocker blocker(&filter_row);
            filter_row.setText(QStringLiteral("bravo"));
        }
        check(filter_user_edits == 1 && submitted_filter_tab == 41 && filter_state.rendered_tab() == 42, "a replacement tab render cannot retarget an earlier edit");
        filter_state.open_empty_row();
        check(filter_state.keeps_empty_row(), "Command+F keeps its explicitly empty row visible");
        check(filter_state.render(43) && !filter_state.keeps_empty_row(), "switching tabs dismisses an empty filter row");
        filter_state.open_empty_row();
        filter_state.record_user_edit(QStringLiteral("a"));
        {
            const QSignalBlocker blocker(&filter_row);
            filter_row.clear();
        }
        check(!filter_state.keeps_empty_row(), "a programmatic clear after text does not retain an empty row");
        check(filter_status_text(1, false, QStringLiteral("a"), 0) == QStringLiteral("Updating…") && filter_status_text(2, false, QStringLiteral("a"), 0) == QStringLiteral("Filter unavailable; edit to retry") && filter_status_text(0, true, QStringLiteral("a"), 0) == QStringLiteral("No matches") && filter_status_text(0, false, QString(), 0) == QStringLiteral("0 items") && filter_status_text(0, false, QStringLiteral("a"), 1) == QStringLiteral("1 matches"), "filter statuses distinguish updating, failure, empty folders, and matches");

        check(focus(filter), "the filter row takes focus again");
        press(Qt::Key_Q);
        check(quits == 1, "Quit stays bound while a text field has focus");
        check(clipboard_generation() == system_generation, "the system clipboard is untouched");
        QApplication::exit(failed ? 1 : 0);
    });
    return QApplication::exec();
}

/// Exercises the Application Title Row and its close button with real widgets
/// and no display.
auto run_application_title_row_checks(QApplication &app) -> int {
    QWidget window;
    window.setStyleSheet(style_sheet());
    auto *layout = new QVBoxLayout(&window);
    layout->setContentsMargins(0, 0, 0, 0);
    WorkspaceBridge bridge;
    int closes = 0;
    auto *sidebar = new Sidebar(&bridge, [] {}, [] {}, [] {}, [&closes] { ++closes; }, &window);
    layout->addWidget(sidebar);
    window.resize(280, 600);
    window.show();
    QTimer::singleShot(0, &app, [&] {
        bool failed = false;
        const auto check = [&failed](bool condition, const char *name) {
            if (!condition) {
                std::cerr << "Application title row check failed: " << name << '\n';
                failed = true;
            }
        };
        auto *row = sidebar->findChild<QWidget *>(QStringLiteral("applicationTitleRow"));
        auto *heading = sidebar->findChild<QLabel *>(QStringLiteral("applicationTitle"));
        auto *close = sidebar->findChild<QAbstractButton *>(QStringLiteral("applicationCloseButton"));
        if (row == nullptr || heading == nullptr || close == nullptr || heading->parentWidget() != row || close->parentWidget() != row) {
            check(false, "heading and close button in the title row");
            QApplication::exit(1);
            return;
        }
        constexpr int inset = favorite_group_margin + favorite_row_horizontal_padding;
        const QRectF heading_box(heading->geometry());
        const QRectF close_box(close->geometry());
        check(heading->text() == QStringLiteral("Dual Pane") && heading->geometry().left() == inset && heading->frameWidth() == 0, "heading starts where Favorite Group names start");
        check(close->mapTo(sidebar, QPoint(close->width(), 0)).x() == sidebar->width() - inset, "close button ends where Add Favorite Item Buttons end");
        check(close_box.center().y() == heading_box.center().y() && heading_box.center().y() == (row->height() + favorite_group_margin) / 2.0, "heading and close button center above the first Favorite Group");
        check(close->accessibleName() == QStringLiteral("Application Close Button") && close->toolTip() == QStringLiteral("Quit Dual Pane") && close->focusPolicy() == Qt::NoFocus, "accessible, pointer-only close button");
        const QPixmap glyph = close->icon().pixmap(close->iconSize(), 3.0);
        check(glyph.devicePixelRatio() == 3.0 && glyph.width() == close->iconSize().width() * 3, "close glyph has a 3x bitmap");
        check(row->childAt(heading->geometry().center()) == nullptr && row->childAt(close->geometry().center()) == close, "presses on the heading reach the row's window drag");

        const QRect area(close->mapTo(&window, QPoint()), close->size());
        const auto uniform = [](const QImage &image) {
            for (int y = 0; y < image.height(); ++y)
                for (int x = 0; x < image.width(); ++x)
                    if (image.pixel(x, y) != image.pixel(0, 0))
                        return false;
            return true;
        };
        const QImage hidden = window.grab(area).toImage();
        check(uniform(hidden), "close button hidden until hovered");
        const QPoint center = close->rect().center();
        QEnterEvent enter(QPointF(center), QPointF(close->mapTo(&window, center)), QPointF(close->mapToGlobal(center)));
        QApplication::sendEvent(close, &enter);
        const QImage hovered = window.grab(area).toImage();
        const auto scale = hovered.devicePixelRatio();
        const QPoint glyph_center = QPointF(QPointF(center) * scale).toPoint();
        check(hovered.pixelColor(QPointF(QPointF(2, center.y()) * scale).toPoint()) == QColor(QString::fromLatin1(application_close_hover_color)), "hover shows the red background");
        check(hovered.pixelColor(glyph_center).lightness() > 200, "hover shows the white glyph");
        QMouseEvent press(QEvent::MouseButtonPress, QPointF(center), QPointF(close->mapToGlobal(center)), Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(close, &press);
        check(window.grab(area).toImage().pixelColor(QPointF(QPointF(2, center.y()) * scale).toPoint()) == QColor(QString::fromLatin1(application_close_pressed_color)), "press darkens the background");
        QMouseEvent release(QEvent::MouseButtonRelease, QPointF(center), QPointF(close->mapToGlobal(center)), Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
        QApplication::sendEvent(close, &release);
        check(closes == 1, "a click asks to close the application once");
        QEvent leave(QEvent::Leave);
        QApplication::sendEvent(close, &leave);
        check(window.grab(area).toImage() == hidden, "close button hides again when the pointer leaves");

        std::unique_ptr<QMessageBox> offered(make_application_quit_confirmation(&window, true));
        auto *dont_ask = offered->checkBox();
        check(offered->text() == QStringLiteral("Are you sure you want to quit Dual Pane?") && offered->defaultButton() != nullptr && offered->buttonRole(offered->defaultButton()) == QMessageBox::AcceptRole && offered->escapeButton() != nullptr, "quit confirmation defaults to Quit and Escape cancels");
        check(dont_ask != nullptr && dont_ask->text() == QStringLiteral("Don’t ask again") && !dont_ask->isChecked(), "quit confirmation offers an unchecked Don't ask again");
        std::unique_ptr<QMessageBox> unavailable(make_application_quit_confirmation(&window, false));
        check(unavailable->checkBox() == nullptr, "no Don't ask again while settings cannot be saved");
        QApplication::exit(failed ? 1 : 0);
    });
    return QApplication::exec();
}

/// Exercises the single-instance activation target with real modal and
/// waiting widgets. The check keeps native application activation disabled;
/// foregrounding itself remains an interactive macOS acceptance item.
auto run_instance_activation_checks(QApplication &app) -> int {
    if (QGuiApplication::platformName() != QStringLiteral("offscreen")) {
        std::cerr << "Instance activation check failed: it runs only on Qt's offscreen platform\n";
        return 1;
    }
    WorkspaceWindow window;
    WaitingWindow waiting(&window);
    auto *workspace_control = new QPushButton(QStringLiteral("Workspace"), &window);
    window.setCentralWidget(workspace_control);
    window.show();
    QTimer::singleShot(0, &app, [&] {
        bool failed = false;
        const auto check = [&failed](bool condition, const char *name) {
            if (!condition) {
                std::cerr << "Instance activation check failed: " << name << '\n';
                failed = true;
            }
        };
        int list_focus_requests = 0;
        const auto focus_browser = [&list_focus_requests](int) { ++list_focus_requests; };

        waiting.show();
        QApplication::processEvents();
        activate_workspace_window(&window, &waiting, 0, focus_browser, false);
        QApplication::processEvents();
        check(waiting.isVisible() && list_focus_requests == 0, "the Waiting Window stays visible and prevents Folder Items focus");
        waiting.hide();
        QApplication::processEvents();

        QMessageBox modal(QMessageBox::Question, QStringLiteral("Quit"), QStringLiteral("Keep this decision open"), QMessageBox::Cancel, &window);
        modal.setWindowModality(Qt::ApplicationModal);
        modal.show();
        QApplication::processEvents();
        auto *modal_focus = modal.button(QMessageBox::Cancel);
        modal_focus->setFocus(Qt::OtherFocusReason);
        activate_workspace_window(&window, &waiting, 0, focus_browser, false);
        QApplication::processEvents();
        check(modal.isVisible() && QApplication::activeModalWidget() == &modal && QApplication::focusWidget() == modal_focus && list_focus_requests == 0, "an existing modal retains visibility and focus");
        modal.hide();
        QApplication::processEvents();

        activate_workspace_window(&window, &waiting, 0, focus_browser, false);
        modal.show();
        QApplication::processEvents();
        check(modal.isVisible() && QApplication::activeModalWidget() == &modal && list_focus_requests == 0, "a modal appearing before the deferred activation prevents Folder Items focus");
        modal.hide();
        QApplication::processEvents();

        activate_workspace_window(&window, &waiting, 0, focus_browser, false);
        QApplication::processEvents();
        check(list_focus_requests == 1, "a Browser request focuses its Folder Items List when no blocking surface exists");
        activate_workspace_window(&window, &waiting, -1, focus_browser, false);
        QApplication::processEvents();
        check(list_focus_requests == 1, "a raise-only request changes no Browser focus");
        QApplication::exit(failed ? 1 : 0);
    });
    return QApplication::exec();
}

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
