#ifndef dual_pane_DESKTOP_SETTINGS_WINDOW_HPP
#define dual_pane_DESKTOP_SETTINGS_WINDOW_HPP

#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/shortcuts.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

/// A macOS-style on/off switch, painted in place of the native QCheckBox
/// indicator. It stays a QCheckBox so the usual checked state, `toggled`
/// signal, keyboard activation, and `findChild<QCheckBox *>` lookups in
/// tests keep working unchanged.
class ToggleSwitch final : public QCheckBox {
  public:
    using QCheckBox::QCheckBox;

  protected:
    [[nodiscard]] auto sizeHint() const -> QSize override {
        const QFontMetrics metrics(font());
        const int label_width = text().isEmpty() ? 0 : label_gap + metrics.horizontalAdvance(text());
        return {track_width + label_width, std::max(track_height, metrics.height())};
    }
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        const qreal y = (height() - track_height) / 2.0;
        const QRectF track(0, y, track_width, track_height);
        const QColor on(QString::fromLatin1(active_color));
        const QColor off(QStringLiteral("#53585F"));
        painter.setPen(Qt::NoPen);
        QColor track_color = off;
        if (isEnabled())
            track_color = isChecked() ? on : off;
        else
            track_color = off.darker(130);
        painter.setBrush(track_color);
        painter.drawRoundedRect(track, track_height / 2.0, track_height / 2.0);
        constexpr qreal knob_margin = 2.0;
        const qreal knob_diameter = track_height - (2 * knob_margin);
        const qreal knob_x = isChecked() ? track.right() - knob_diameter - knob_margin : track.left() + knob_margin;
        painter.setBrush(QColor(QStringLiteral("#F4F6F8")));
        painter.drawEllipse(QRectF(knob_x, y + knob_margin, knob_diameter, knob_diameter));
        if (hasFocus()) {
            painter.setPen(QPen(on, 2));
            painter.setBrush(Qt::NoBrush);
            painter.drawRoundedRect(track.adjusted(-2, -2, 2, 2), (track_height / 2.0) + 2, (track_height / 2.0) + 2);
        }
        if (!text().isEmpty()) {
            painter.setPen(QColor(QString::fromLatin1(isEnabled() ? text_color : inactive_text_color)));
            painter.drawText(QRectF(track_width + label_gap, 0, width() - track_width - label_gap, height()), Qt::AlignVCenter | Qt::AlignLeft, text());
        }
    }

  private:
    static constexpr int track_width = 34;
    static constexpr int track_height = 20;
    static constexpr int label_gap = 8;
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
        QFont category_font = categories_->font();
        category_font.setPixelSize(16);
        categories_->setFont(category_font);
        categories_->setMinimumWidth(320);
        categories_->addItem(QStringLiteral("General"));
        categories_->addItem(QStringLiteral("Keyboard Shortcuts"));
        categories_->addItem(QStringLiteral("Context Menus"));
        categories_->setCurrentRow(0);
        sidebar_layout->addWidget(categories_, 1);
        pages_->setObjectName(QStringLiteral("settingsPages"));
        pages_->addWidget(general_page());
        pages_->addWidget(shortcut_page());
        pages_->addWidget(context_menu_page());
        QObject::connect(categories_, &QListWidget::currentRowChanged, this, [this](int index) {
            pages_->setCurrentIndex(index);
            clear_context_menu_feedback();
        });
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
        if (bridge_) {
            QObject::connect(bridge_, &WorkspaceBridge::bindingsRevisionChanged, this, [this] { QTimer::singleShot(0, this, [this] { rebuild_shortcuts(); }); });
            QObject::connect(bridge_, &WorkspaceBridge::contextMenusRevisionChanged, this, [this] { sync_context_menus(); });
            QObject::connect(bridge_, &WorkspaceBridge::contextMenuRejectionRevisionChanged, this, [this] { QTimer::singleShot(0, this, [this] { show_context_menu_feedback(bridge_->contextMenuRejectionText()); }); });
        }
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
    auto info_button(QWidget *parent, const QString &subject, const QString &description) -> QToolButton * {
        auto *button = new ShortcutGlyphButton(parent, ShortcutGlyph::Info);
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
            label->setContentsMargins(6, 6, 6, 6);
            label->setStyleSheet(QStringLiteral("QLabel#settingsInfoText { background-color:#2B323B; color:#F4F6F8; border:1px solid #56616E; border-radius:5px; padding:6px; font-size:13px; }"));
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
        auto *check = new ToggleSwitch(QStringLiteral("Don’t show notices at startup"), page);
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
        auto *quit_card = make_card(QStringLiteral("Quitting"), QStringLiteral("Quitting"), QStringLiteral("Choose whether the Sidebar close button asks before it quits."));
        auto *quit_row = new QHBoxLayout();
        auto *ask_quit = new ToggleSwitch(QStringLiteral("Ask before quitting from the close button"), page);
        ask_quit->setObjectName(QStringLiteral("settingsAskBeforeQuit"));
        ask_quit->setAccessibleName(QStringLiteral("Ask before quitting from the close button"));
        ask_quit->setChecked(!bridge_ || !bridge_->getSkipQuitConfirmation());
        ask_quit->setEnabled(bridge_ != nullptr);
        if (bridge_) {
            QObject::connect(ask_quit, &QCheckBox::toggled, page, [this](bool ask) { bridge_->changeSkipQuitConfirmation(!ask); });
            QObject::connect(bridge_, &WorkspaceBridge::skipQuitConfirmationChanged, page, [this, ask_quit] { QSignalBlocker block(ask_quit); ask_quit->setChecked(!bridge_->getSkipQuitConfirmation()); });
        }
        quit_row->addWidget(ask_quit);
        quit_row->addWidget(info_button(page, QStringLiteral("Quit confirmation"), QStringLiteral("When selected, the close button at the right end of the Dual Pane title row asks before quitting. Choosing Don’t ask again in that confirmation clears this setting. Command+Q never asks, and running file operations always ask.")));
        quit_row->addStretch();
        quit_card->addLayout(quit_row);
        auto *visibility_card = make_card(QStringLiteral("Item Visibility"), QStringLiteral("Item Visibility"), QStringLiteral("Choose which hidden item classes appear in both Browsers."));
        auto *visibility_help = new QLabel(QStringLiteral("An item matching more than one rule appears only when every matching Show control is selected."), page);
        visibility_help->setObjectName(QStringLiteral("settingsVisibilityHelp"));
        visibility_help->setAccessibleName(QStringLiteral("Visibility settings help"));
        visibility_help->setWordWrap(true);
        visibility_card->addWidget(visibility_help);
        auto *macos_items = new ToggleSwitch(QStringLiteral("Show macOS metadata and service items"), page);
        auto *dotfiles = new ToggleSwitch(QStringLiteral("Show dotfiles"), page);
        auto *macos_invisible = new ToggleSwitch(QStringLiteral("Show items marked invisible by macOS"), page);
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
        auto add_visibility_row = [visibility_card, page, this](QCheckBox *control, const QString &subject, const QString &description) {
            auto *row = new QHBoxLayout();
            row->addWidget(control);
            row->addWidget(info_button(page, subject, description));
            row->addStretch();
            visibility_card->addLayout(row);
        };
        add_visibility_row(macos_items, QStringLiteral("macOS metadata and service items"), QStringLiteral("Shows items such as .DS_Store, Spotlight indexes, and other files macOS and its services maintain."));
        add_visibility_row(dotfiles, QStringLiteral("Dotfiles"), QStringLiteral("Shows files and folders whose names start with a dot, the Unix convention for hidden items."));
        add_visibility_row(macos_invisible, QStringLiteral("Items marked invisible by macOS"), QStringLiteral("Shows items macOS has explicitly flagged as invisible, separate from dotfiles and metadata items."));
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
            label->setObjectName(name == QStringLiteral("Original Shortcuts") ? QStringLiteral("shortcutColumnHeadingWide") : QStringLiteral("shortcutColumnHeading"));
            int width = 36;
            if (name == QStringLiteral("Action"))
                width = 170;
            else if (name == QStringLiteral("Shortcuts"))
                width = shortcut_column_width();
            else if (name == QStringLiteral("Original Shortcuts"))
                width = original_shortcut_column_width();
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
        const auto longest_supported_shortcut = QStringLiteral("Control+Option+Shift+Command+Forward Delete");
        return metrics.horizontalAdvance(longest_supported_shortcut) + 6 + 18;
    }
    static auto original_shortcut_column_width() -> int { return 200; }
    static auto shortcut_table_width() -> int {
        constexpr int action_width = 170;
        constexpr int scope_width = 120;
        constexpr int add_width = 36;
        constexpr int column_gap = 8;
        constexpr int horizontal_padding = 8;
        constexpr int card_border = 2;
        constexpr int column_count = 5;
        return action_width + shortcut_column_width() + original_shortcut_column_width() + scope_width + add_width + ((column_count - 1) * column_gap) + (2 * horizontal_padding) + card_border;
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
                if (bridge_->actionCategory(index) != group_index)
                    continue;
                const QString id = bridge_->actionId(index);
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
                action_layout->addWidget(info_button(action_cell, name, help));
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
                original_cell->setFixedWidth(original_shortcut_column_width());
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
                add_layout->addWidget(capture, 0, Qt::AlignLeft);
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
    static auto context_menu_table_header(QWidget *parent) -> QWidget * {
        auto *header = new QWidget(parent);
        header->setObjectName(QStringLiteral("contextMenuTableHeader"));
        header->setFixedHeight(36);
        auto *head = new QHBoxLayout(header);
        head->setContentsMargins(8, 0, 8, 0);
        head->setSpacing(8);
        const std::array<std::pair<QString, int>, 4> columns = {{{QStringLiteral("Action"), 260}, {QStringLiteral("Normal"), 120}, {QStringLiteral("Comprehensive"), 160}, {QStringLiteral("Shortcut"), 220}}};
        for (const auto &[name, width] : columns) {
            auto *label = new QLabel(name, header);
            label->setObjectName(QStringLiteral("contextMenuColumnHeading"));
            label->setFixedWidth(width);
            head->addWidget(label);
        }
        head->addStretch();
        return header;
    }
    auto context_menu_page() -> QWidget * {
        auto *page = new QWidget(pages_);
        page->setObjectName(QStringLiteral("settingsContextMenuPage"));
        page->setAccessibleName(QStringLiteral("Context Menus Settings"));
        auto *layout = new QVBoxLayout(page);
        layout->setContentsMargins(16, 16, 16, 16);
        layout->setSpacing(10);
        context_menu_scroll_ = new QScrollArea(page);
        context_menu_scroll_->setWidgetResizable(true);
        context_menu_scroll_->setObjectName(QStringLiteral("settingsContextMenuScroll"));
        context_menu_list_ = new QWidget(context_menu_scroll_);
        context_menu_list_->setObjectName(QStringLiteral("settingsContextMenuList"));
        context_menu_layout_ = new QVBoxLayout(context_menu_list_);
        context_menu_layout_->setContentsMargins(0, 0, 0, 0);
        context_menu_layout_->setSpacing(10);
        context_menu_scroll_->setWidget(context_menu_list_);
        layout->addWidget(context_menu_scroll_, 1);
        context_menu_feedback_ = new QLabel(page);
        context_menu_feedback_->setObjectName(QStringLiteral("contextMenuFeedback"));
        context_menu_feedback_->setAccessibleName(QStringLiteral("Context Menu settings feedback"));
        context_menu_feedback_->setWordWrap(true);
        context_menu_feedback_->hide();
        layout->addWidget(context_menu_feedback_);
        auto *resets = new QHBoxLayout();
        auto *normal = new QPushButton(QStringLiteral("Reset Normal…"), page);
        normal->setObjectName(QStringLiteral("contextMenuResetNormal"));
        normal->setAccessibleName(QStringLiteral("Reset Normal Context Menu"));
        normal->setToolTip(QStringLiteral("Restore the Normal Context Menu to its factory configuration."));
        auto *comprehensive = new QPushButton(QStringLiteral("Reset Comprehensive…"), page);
        comprehensive->setObjectName(QStringLiteral("contextMenuResetComprehensive"));
        comprehensive->setAccessibleName(QStringLiteral("Reset Comprehensive Context Menu"));
        comprehensive->setToolTip(QStringLiteral("Restore the Comprehensive Context Menu to its factory configuration."));
        auto *both = new QPushButton(QStringLiteral("Reset Both…"), page);
        both->setObjectName(QStringLiteral("contextMenuResetBoth"));
        both->setAccessibleName(QStringLiteral("Reset Both Context Menus"));
        both->setToolTip(QStringLiteral("Restore both Context Menus to their factory configurations."));
        QObject::connect(normal, &QPushButton::clicked, this, [this] { confirm_context_menu_reset(0); });
        QObject::connect(comprehensive, &QPushButton::clicked, this, [this] { confirm_context_menu_reset(1); });
        QObject::connect(both, &QPushButton::clicked, this, [this] { confirm_context_menu_reset(2); });
        resets->addWidget(normal);
        resets->addWidget(comprehensive);
        resets->addWidget(both);
        resets->addStretch();
        layout->addLayout(resets);
        rebuild_context_menus();
        return page;
    }
    void rebuild_context_menus() {
        if (!context_menu_layout_)
            return;
        context_menu_normal_checks_.clear();
        context_menu_comprehensive_checks_.clear();
        context_menu_shortcuts_.clear();
        while (auto *item = context_menu_layout_->takeAt(0)) {
            if (auto *widget = item->widget()) {
                widget->hide();
                widget->deleteLater();
            }
            delete item;
        }
        if (!bridge_) {
            context_menu_layout_->addWidget(new QLabel(QStringLiteral("Context Menu settings are unavailable."), context_menu_list_));
            return;
        }
        QString previous_group;
        QVBoxLayout *group_layout = nullptr;
        int group_rows = 0;
        for (int index = 0; index < bridge_->contextMenuActionCount(); ++index) {
            const QString group_name = bridge_->contextMenuGroupLabel(index);
            if (!group_layout || group_name != previous_group) {
                auto *group = new QFrame(context_menu_list_);
                group->setObjectName(QStringLiteral("contextMenuGroup"));
                group->setAccessibleName(group_name + QStringLiteral(" Context Menu actions"));
                group_layout = new QVBoxLayout(group);
                group_layout->setContentsMargins(0, 0, 0, 0);
                group_layout->setSpacing(0);
                auto *heading = new QLabel(group_name, group);
                heading->setObjectName(QStringLiteral("contextMenuGroupHeading"));
                group_layout->addWidget(heading);
                group_layout->addWidget(context_menu_table_header(group));
                context_menu_layout_->addWidget(group, 0, Qt::AlignLeft);
                previous_group = group_name;
                group_rows = 0;
            }
            auto *row = new QWidget(context_menu_list_);
            row->setObjectName(QStringLiteral("contextMenuRow"));
            row->setProperty("alternate", group_rows % 2 != 0);
            ++group_rows;
            const QString action = bridge_->contextMenuActionLabel(index);
            row->setAccessibleName(action + QStringLiteral(" Context Menu settings"));
            auto *cells = new QHBoxLayout(row);
            cells->setContentsMargins(8, 3, 8, 3);
            cells->setSpacing(8);
            auto *action_cell = new QWidget(row);
            action_cell->setFixedWidth(260);
            auto *action_cell_layout = new QHBoxLayout(action_cell);
            action_cell_layout->setContentsMargins(0, 0, 0, 0);
            auto *action_label = new QLabel(action, action_cell);
            action_cell_layout->addWidget(action_label);
            action_cell_layout->addStretch();
            action_cell_layout->addWidget(info_button(action_cell, action, action + QStringLiteral(" in the Folder Items context menu. Use Normal and Comprehensive to choose which menu variant shows this action.")));
            cells->addWidget(action_cell);
            auto add_checkbox = [this, row, cells, action, index](int kind, int width, bool checked) -> QCheckBox * {
                auto *cell = new QWidget(row);
                cell->setFixedWidth(width);
                auto *cell_layout = new QHBoxLayout(cell);
                cell_layout->setContentsMargins(0, 0, 0, 0);
                auto *check = new ToggleSwitch(cell);
                check->setObjectName(kind == 0 ? QStringLiteral("contextMenuNormalCheck") : QStringLiteral("contextMenuComprehensiveCheck"));
                const QString variant = kind == 0 ? QStringLiteral("Normal") : QStringLiteral("Comprehensive");
                check->setAccessibleName(QStringLiteral("Show ") + action + QStringLiteral(" in ") + variant + QStringLiteral(" Context Menu"));
                check->setChecked(checked);
                QObject::connect(check, &QCheckBox::toggled, this, [this, check, index, kind](bool visible) {
                    const QString error = bridge_->changeContextMenuVisibility(index, kind, visible);
                    if (!error.isEmpty()) {
                        QSignalBlocker block(check);
                        check->setChecked(kind == 0 ? bridge_->contextMenuNormalVisible(index) : bridge_->contextMenuComprehensiveVisible(index));
                        show_context_menu_feedback(error);
                    } else {
                        clear_context_menu_feedback();
                    }
                });
                cell_layout->addWidget(check, 0, Qt::AlignLeft);
                cells->addWidget(cell);
                return check;
            };
            context_menu_normal_checks_.emplace_back(add_checkbox(0, 120, bridge_->contextMenuNormalVisible(index)));
            context_menu_comprehensive_checks_.emplace_back(add_checkbox(1, 160, bridge_->contextMenuComprehensiveVisible(index)));
            const QString portable = bridge_->contextMenuShortcut(index);
            const QString shortcut = QKeySequence::fromString(portable, QKeySequence::PortableText).toString(QKeySequence::NativeText);
            auto *shortcut_label = new QLabel(shortcut.isEmpty() ? QStringLiteral("None") : shortcut, row);
            shortcut_label->setObjectName(shortcut.isEmpty() ? QStringLiteral("contextMenuShortcutNone") : QStringLiteral("contextMenuShortcut"));
            shortcut_label->setAccessibleName(QStringLiteral("Shortcut for ") + action + QStringLiteral(": ") + (shortcut.isEmpty() ? QStringLiteral("None") : shortcut));
            shortcut_label->setFixedWidth(220);
            cells->addWidget(shortcut_label);
            context_menu_shortcuts_.emplace_back(shortcut_label);
            cells->addStretch();
            group_layout->addWidget(row);
        }
        context_menu_layout_->addStretch();
    }
    void sync_context_menus() {
        if (!bridge_)
            return;
        const auto count = static_cast<std::size_t>(bridge_->contextMenuActionCount());
        if (context_menu_normal_checks_.size() != count || context_menu_comprehensive_checks_.size() != count || context_menu_shortcuts_.size() != count) {
            rebuild_context_menus();
            return;
        }
        for (std::size_t index = 0; index < count; ++index) {
            auto *normal = context_menu_normal_checks_.at(index).data();
            auto *comprehensive = context_menu_comprehensive_checks_.at(index).data();
            auto *shortcut_label = context_menu_shortcuts_.at(index).data();
            if (normal == nullptr || comprehensive == nullptr || shortcut_label == nullptr) {
                rebuild_context_menus();
                return;
            }
            const int row = static_cast<int>(index);
            const QSignalBlocker normal_block(normal);
            const QSignalBlocker comprehensive_block(comprehensive);
            normal->setChecked(bridge_->contextMenuNormalVisible(row));
            comprehensive->setChecked(bridge_->contextMenuComprehensiveVisible(row));
            const QString portable = bridge_->contextMenuShortcut(row);
            const QString shortcut = QKeySequence::fromString(portable, QKeySequence::PortableText).toString(QKeySequence::NativeText);
            const QString display = shortcut.isEmpty() ? QStringLiteral("None") : shortcut;
            shortcut_label->setText(display);
            const QString style_name = shortcut.isEmpty() ? QStringLiteral("contextMenuShortcutNone") : QStringLiteral("contextMenuShortcut");
            if (shortcut_label->objectName() != style_name) {
                shortcut_label->setObjectName(style_name);
                shortcut_label->style()->unpolish(shortcut_label);
                shortcut_label->style()->polish(shortcut_label);
            }
            shortcut_label->setAccessibleName(QStringLiteral("Shortcut for ") + bridge_->contextMenuActionLabel(row) + QStringLiteral(": ") + display);
        }
    }
    void show_context_menu_feedback(const QString &text) {
        if (!context_menu_feedback_)
            return;
        context_menu_feedback_->setText(text);
        context_menu_feedback_->setVisible(!text.isEmpty());
        if (!text.isEmpty() && context_menu_scroll_)
            context_menu_scroll_->ensureWidgetVisible(context_menu_feedback_, 0, 16);
    }
    void clear_context_menu_feedback() { show_context_menu_feedback(QString()); }
    void confirm_context_menu_reset(int scope) {
        QString scope_name;
        QString object_name;
        if (scope == 0) {
            scope_name = QStringLiteral("Normal Context Menu");
            object_name = QStringLiteral("contextMenuResetNormalConfirmation");
        } else if (scope == 1) {
            scope_name = QStringLiteral("Comprehensive Context Menu");
            object_name = QStringLiteral("contextMenuResetComprehensiveConfirmation");
        } else {
            scope_name = QStringLiteral("Both Context Menus");
            object_name = QStringLiteral("contextMenuResetBothConfirmation");
        }
        const QString question = scope == 2 ? QStringLiteral("Reset Both Context Menus to their factory configurations?") : QStringLiteral("Reset ") + scope_name + QStringLiteral(" to its factory configuration?");
        auto *box = new QMessageBox(QMessageBox::Question, QStringLiteral("Reset ") + scope_name, question, QMessageBox::NoButton, this);
        box->setObjectName(object_name);
        auto *confirm = box->addButton(QStringLiteral("Reset"), QMessageBox::AcceptRole);
        auto *cancel = box->addButton(QMessageBox::Cancel);
        box->setDefaultButton(cancel);
        box->setEscapeButton(cancel);
        box->setWindowModality(Qt::WindowModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, this, [this, box, confirm, scope] {
            if (box->clickedButton() != confirm || !bridge_)
                return;
            if (scope == 2)
                bridge_->resetContextMenus();
            else
                bridge_->resetContextMenu(scope);
            clear_context_menu_feedback();
        });
        box->open();
    }
    WorkspaceBridge *bridge_;
    std::function<void(bool)> capture_;
    QListWidget *categories_;
    QStackedWidget *pages_;
    QWidget *shortcut_list_ = nullptr;
    QScrollArea *shortcut_scroll_ = nullptr;
    QVBoxLayout *shortcut_layout_ = nullptr;
    QHash<int, QPointer<QLabel>> feedback_by_action_;
    QWidget *context_menu_list_ = nullptr;
    QScrollArea *context_menu_scroll_ = nullptr;
    QVBoxLayout *context_menu_layout_ = nullptr;
    QLabel *context_menu_feedback_ = nullptr;
    std::vector<QPointer<QCheckBox>> context_menu_normal_checks_;
    std::vector<QPointer<QCheckBox>> context_menu_comprehensive_checks_;
    std::vector<QPointer<QLabel>> context_menu_shortcuts_;
};

class SettingsWindowController final : public QObject {
  public:
    explicit SettingsWindowController(QWidget *owner, WorkspaceBridge *bridge = nullptr) : QObject(owner), owner_(owner), bridge_(bridge), shortcut_(new QShortcut(QKeySequence(Qt::CTRL | Qt::Key_Comma), owner)) {
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

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_SETTINGS_WINDOW_HPP
