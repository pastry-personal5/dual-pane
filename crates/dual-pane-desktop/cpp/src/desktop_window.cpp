#include "dual_pane_desktop/desktop_window.hpp"
#include "dual-pane-desktop/src/folder_items_list_model.cxxqt.h"
#include "dual-pane-desktop/src/operations_bridge.cxxqt.h"
#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/desktop_checks.hpp"
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
namespace dual_pane_desktop {
namespace {
using namespace gui;

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
    if (qEnvironmentVariableIsSet("DUAL_PANE_INTERNAL_FOLDER_ITEMS_CONTEXT_MENU_CHECK"))
        return run_folder_items_context_menu_checks(app);
    if (qEnvironmentVariableIsSet("DUAL_PANE_INTERNAL_SETTINGS_WINDOW_CHECK"))
        return run_settings_window_checks(app);
    if (qEnvironmentVariableIsSet("DUAL_PANE_INTERNAL_TEXT_EDITOR_FOCUS_CHECK"))
        return run_text_editor_focus_checks(app);
    if (qEnvironmentVariableIsSet("DUAL_PANE_INTERNAL_APPLICATION_TITLE_ROW_CHECK"))
        return run_application_title_row_checks(app);
    if (qEnvironmentVariableIsSet("DUAL_PANE_INTERNAL_INSTANCE_ACTIVATION_CHECK"))
        return run_instance_activation_checks(app);
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
    auto *folder_items_menus = new FolderItemsMenuCoordinator(&window);
    auto *left_browser = new Browser(&left_model, &icons, folder_items_menus, split);
    auto *right_browser = new Browser(&right_model, &icons, folder_items_menus, split);
    const std::array<Browser *, 2> browsers = {left_browser, right_browser};
    const std::array<FolderItemsListModel *, 2> models = {&left_model, &right_model};
    const auto focus_active_list = [&bridge, browsers] { browsers.at(bridge.getActiveBrowser() == 0 ? 0 : 1)->view()->focusList(); };
    coordinator.addListener([&bridge, &window, waiting, browsers] {
        for (int request = bridge.takeActivationRequest(); request != -2; request = bridge.takeActivationRequest()) {
            activate_workspace_window(&window, waiting, request, [browsers](int browser) { browsers.at(browser == 0 ? 0 : 1)->view()->focusList(); });
        }
    });
    auto *notices = new NoticesWindow(&bridge, &window);
    const auto show_notices = [notices] {
        notices->show();
        notices->raise();
        notices->activateWindow();
    };
    auto *settings_controller = new SettingsWindowController(&window, &bridge);
    const auto show_settings = [settings_controller] { settings_controller->present(); };
    // The same quit as Command+Q.
    const auto quit_application = [&window] { window.close(); };
    // The Application Close Button asks first unless the person chose not to
    // be asked. While an operation is unfinished, the close handler's own
    // quit confirmation asks instead, so nobody is asked twice.
    QPointer<QMessageBox> application_quit_prompt;
    const auto close_application = [&window, &bridge, &operations, &application_quit_prompt, quit_application] {
        bool unfinished = false;
        for (int panel = 0; panel < operations.panelCount(); ++panel)
            unfinished |= !operations.panelFinished(panel);
        if (unfinished || bridge.getSkipQuitConfirmation()) {
            quit_application();
            return;
        }
        if (application_quit_prompt) {
            application_quit_prompt->raise();
            return;
        }
        auto *box = make_application_quit_confirmation(&window, bridge.getSettingsInteractionAvailable());
        box->setWindowModality(Qt::WindowModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, &window, [&bridge, box, quit_application] {
            if (box->buttonRole(box->clickedButton()) != QMessageBox::AcceptRole)
                return;
            if (box->checkBox() != nullptr && box->checkBox()->isChecked())
                bridge.changeSkipQuitConfirmation(true);
            quit_application();
        });
        application_quit_prompt = box;
        box->open();
    };
    auto *sidebar = new Sidebar(&bridge, focus_active_list, show_notices, show_settings, close_application, &standard_layout);
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
    handlers.insert(QStringLiteral("QuitApplication"), quit_application);
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
    const std::array<std::pair<QString, int>, 3> clipboard_commands = {{{QStringLiteral("CopyItems"), clipboard_copy}, {QStringLiteral("CutItems"), clipboard_cut}, {QStringLiteral("PasteItems"), clipboard_paste}}};
    for (const auto &clipboard_command : clipboard_commands)
        list_handlers.insert(clipboard_command.first, [models, command = clipboard_command.second](int browser) {
            auto *model = models.at(browser);
            model->startClipboardCommandObserved(command, model->activeTabId(), model->visibleRevision());
        });
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
        shortcuts.followFocus(now);
        for (int browser = 0; browser < 2; ++browser)
            if (inside(now, browsers.at(browser)))
                models.at(browser)->activateBrowser();
    });
    QObject::connect(&app, &QGuiApplication::applicationStateChanged, &window, [&bridge](Qt::ApplicationState state) {
        bridge.setApplicationActive(state == Qt::ApplicationActive);
        if (state == Qt::ApplicationActive)
            bridge.observeClipboard();
    });
    QObject::connect(QGuiApplication::clipboard(), &QClipboard::changed, &window, [&bridge](QClipboard::Mode mode) {
        if (mode == QClipboard::Clipboard)
            bridge.observeClipboard();
    });

    {
        auto &state = scheduler_state();
        std::scoped_lock lock(state.mutex);
        state.scheduler = &scheduler;
    }
    bridge.start(std::move(startup));
    bool instance_exit_ok = false;
    const int instance_exit_ms = qEnvironmentVariableIntValue("DUAL_PANE_INTERNAL_INSTANCE_EXIT_MS", &instance_exit_ok);
    if (instance_exit_ok && instance_exit_ms > 0)
        QTimer::singleShot(instance_exit_ms, &app, [] { QApplication::quit(); });
    bridge.observeClipboard();
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
    folder_items_menus->dismiss();
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
