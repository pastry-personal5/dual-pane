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
    auto *button = new QToolButton(&window);
    button->setFocusPolicy(Qt::StrongFocus);
    for (QWidget *widget : std::initializer_list<QWidget *>{left, right, filter, rename, favorite, button})
        layout->addWidget(widget);

    // Per Browser: Copy, Cut, and Paste Items.
    std::array<std::array<int, 3>, 2> list_commands{};
    int new_tabs = 0, quits = 0;
    QHash<QString, ShortcutBinder::Handler> handlers;
    handlers.insert(QStringLiteral("NewTab"), [&new_tabs] { ++new_tabs; });
    handlers.insert(QStringLiteral("QuitApplication"), [&quits] { ++quits; });
    QHash<QString, ShortcutBinder::ListHandler> list_handlers;
    const std::array<std::pair<QString, int>, 3> clipboard_commands = {{{QStringLiteral("CopyItems"), clipboard_copy}, {QStringLiteral("CutItems"), clipboard_cut}, {QStringLiteral("PasteItems"), clipboard_paste}}};
    for (const auto &[action, command] : clipboard_commands)
        list_handlers.insert(action, [&list_commands, command = command](int browser) { ++list_commands.at(browser).at(command); });
    WorkspaceBridge bridge;
    ShortcutBinder shortcuts(&window, &bridge, handlers, {left, right}, list_handlers);
    // The delivered defaults; `delivered_shortcuts_map_to_qt_key_sequences` pins them.
    shortcuts.rebind({{QStringLiteral("CopyItems"), QStringLiteral("Ctrl+C"), scope_folder_items_list}, {QStringLiteral("CutItems"), QStringLiteral("Ctrl+X"), scope_folder_items_list}, {QStringLiteral("PasteItems"), QStringLiteral("Ctrl+V"), scope_folder_items_list}, {QStringLiteral("NewTab"), QStringLiteral("Ctrl+T"), scope_window}, {QStringLiteral("QuitApplication"), QStringLiteral("Ctrl+Q"), scope_application}});
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

        const std::array<std::pair<QLineEdit *, std::string>, 3> editors = {{{filter, "filter row"}, {rename, "rename editor"}, {favorite, "Favorites name editor"}}};
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
            check(list_commands == observed && new_tabs == tabs, name + ": Folder Items and workspace shortcuts stand aside");
        }
        check(!edits_text(left) && !edits_text(button), "lists and buttons are not text fields");

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
