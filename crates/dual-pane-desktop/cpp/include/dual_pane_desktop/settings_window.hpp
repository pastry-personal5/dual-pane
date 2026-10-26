#ifndef dual_pane_DESKTOP_SETTINGS_WINDOW_HPP
#define dual_pane_DESKTOP_SETTINGS_WINDOW_HPP

#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/shortcuts.hpp"

#include <QtCore/QFile>
#include <QtWidgets/QFileDialog>

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
    // QCheckBox otherwise uses the platform indicator's smaller click rect.
    [[nodiscard]] auto hitButton(const QPoint &position) const -> bool override { return rect().contains(position); }
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
        categories_->addItem(QStringLiteral("File Associations"));
        categories_->setCurrentRow(0);
        sidebar_layout->addWidget(categories_, 1);
        pages_->setObjectName(QStringLiteral("settingsPages"));
        pages_->addWidget(general_page());
        pages_->addWidget(shortcut_page());
        pages_->addWidget(context_menu_page());
        pages_->addWidget(file_association_page());
        QObject::connect(categories_, &QListWidget::currentRowChanged, this, [this](int index) {
            pages_->setCurrentIndex(index);
            clear_context_menu_feedback();
            if (index == 3 && bridge_)
                bridge_->refreshFileAssociations();
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
            QObject::connect(bridge_, &WorkspaceBridge::fileAssociationsRevisionChanged, this, [this] { rebuild_file_associations(); });
            QObject::connect(bridge_, &WorkspaceBridge::fileAssociationsPendingChanged, this, [this] {
                sync_file_association_status();
                sync_file_association_loading();
            });
            QObject::connect(bridge_, &WorkspaceBridge::fileAssociationsErrorChanged, this, [this] { sync_file_association_status(); });
        }
    }

    void focus_file_association(const QString &content_type) {
        pending_association_focus_ = content_type;
        categories_->setCurrentRow(3);
        rebuild_file_associations();
    }

  private:
    void showEvent(QShowEvent *event) override {
        QDialog::showEvent(event);
        categories_->setFocus();
    }
    static auto action_name(const QString &id) -> QString {
        if (id == QStringLiteral("ConfigureOpenWith"))
            return QStringLiteral("Configure Open With…");
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
            menu->setStyleSheet(QStringLiteral("QMenu#settingsInfoMenu { background-color:#2B323B; color:#F4F6F8; border:1px solid #778392; padding:0; }"));
            auto *label = new QLabel(description, menu);
            label->setObjectName(QStringLiteral("settingsInfoText"));
            label->setAccessibleName(description);
            label->setWordWrap(true);
            label->setMinimumWidth(300);
            label->setMaximumWidth(400);
            label->setContentsMargins(6, 6, 6, 6);
            label->setStyleSheet(QStringLiteral("QLabel#settingsInfoText { background:transparent; color:#F4F6F8; border:0; padding:6px; font-size:13px; }"));
            auto *action = new QWidgetAction(menu);
            action->setDefaultWidget(label);
            menu->addAction(action);
            QObject::connect(menu, &QMenu::aboutToHide, button, [button] { QTimer::singleShot(0, button, [button] { button->setFocus(Qt::OtherFocusReason); }); });
            QPoint anchor = button->mapToGlobal(QPoint(0, button->height()));
            const int window_right = button->window()->mapToGlobal(QPoint(button->window()->width(), 0)).x();
            if (anchor.x() + menu->sizeHint().width() > window_right)
                anchor.setX(anchor.x() + button->width() - menu->sizeHint().width());
            menu->popup(anchor);
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
        auto make_card = [page, layout](const QString &name, const QString &heading) {
            auto *card = new QFrame(page);
            card->setObjectName(QStringLiteral("settingsCard"));
            auto *card_layout = new QVBoxLayout(card);
            card_layout->setContentsMargins(16, 14, 16, 14);
            card_layout->setSpacing(8);
            auto *card_heading = new QLabel(heading, card);
            card_heading->setObjectName(QStringLiteral("settingsCardHeading"));
            card->setAccessibleName(name);
            card_layout->addWidget(card_heading);
            layout->addWidget(card);
            return card_layout;
        };
        auto *startup_card = make_card(QStringLiteral("Startup Notices"), QStringLiteral("Startup Notices"));
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
        notices->addStretch();
        notices->addWidget(info_button(page, QStringLiteral("Startup notices"), QStringLiteral("When selected, routine notices stay in Notices at startup. Important errors still open Notices so you can act on them.")));
        startup_card->addLayout(notices);
        auto *quit_card = make_card(QStringLiteral("Quitting"), QStringLiteral("Quitting"));
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
        quit_row->addStretch();
        quit_row->addWidget(info_button(page, QStringLiteral("Quit confirmation"), QStringLiteral("When selected, the close button at the right end of the Dual Pane title row asks before quitting. Choosing Don’t ask again in that confirmation clears this setting. Command+Q never asks, and running file operations always ask.")));
        quit_card->addLayout(quit_row);
        auto *visibility_card = make_card(QStringLiteral("Item Visibility"), QStringLiteral("Item Visibility"));
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
            row->addStretch();
            row->addWidget(info_button(page, subject, description + QStringLiteral(" An item matching more than one rule appears only when every matching Show control is selected.")));
            visibility_card->addLayout(row);
        };
        add_visibility_row(macos_items, QStringLiteral("macOS metadata and service items"), QStringLiteral("Shows items such as .DS_Store, Spotlight indexes, and other files macOS and its services maintain."));
        add_visibility_row(dotfiles, QStringLiteral("Dotfiles"), QStringLiteral("Shows files and folders whose names start with a dot, the Unix convention for hidden items."));
        add_visibility_row(macos_invisible, QStringLiteral("Items marked invisible by macOS"), QStringLiteral("Shows items macOS has explicitly flagged as invisible, separate from dotfiles and metadata items."));
        auto *reset_card = make_card(QStringLiteral("Reset Settings"), QStringLiteral("Reset Settings"));
        auto *reset_row = new QHBoxLayout();
        auto *reset = new QPushButton(QStringLiteral("Reset Settings"), page);
        reset->setObjectName(QStringLiteral("settingsReset"));
        reset->setAccessibleName(QStringLiteral("Reset Settings"));
        reset->setEnabled(bridge_ != nullptr);
        QObject::connect(reset, &QPushButton::clicked, this, [this] {
            auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Reset Settings"), QStringLiteral("Reset Dual Pane’s settings, including File Association pins and Open With choices?\n\nThis does not undo default-application changes already made in macOS."), QMessageBox::NoButton, this);
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
        reset_row->addStretch();
        reset_row->addWidget(info_button(page, QStringLiteral("Reset Settings"), QStringLiteral("Reset restores all compiled settings and shortcuts, removes Favorites and Dual Pane’s File Association choices, closes open tabs, and returns the workspace layout to its defaults. It does not undo default-application changes already made in macOS. The current database is kept as a backup.")));
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
        add_info_column_heading(header, head, QStringLiteral("shortcutColumnHeading"));
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
    static auto info_column_width() -> int { return 18; }
    static auto table_text_inset() -> int {
        constexpr int card_border = 1;
        constexpr int cell_padding = 8;
        return card_border + cell_padding;
    }
    static auto category_section(QWidget *list, const QString &name, const QString &heading_name) -> QWidget * {
        auto *section = new QWidget(list);
        auto *layout = new QVBoxLayout(section);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(6);
        auto *heading = new QLabel(name, section);
        heading->setObjectName(heading_name);
        heading->setContentsMargins(table_text_inset(), 0, 0, 0);
        layout->addWidget(heading);
        return section;
    }
    static void add_info_column_heading(QWidget *header, QHBoxLayout *head, const QString &heading_name) {
        head->addStretch();
        auto *label = new QLabel(header);
        label->setObjectName(heading_name);
        label->setFixedWidth(info_column_width());
        head->addWidget(label);
    }
    void add_info_cell(QWidget *row, QHBoxLayout *cells, const QString &subject, const QString &description) {
        cells->addStretch();
        auto *cell = new QWidget(row);
        cell->setObjectName(QStringLiteral("settingsInfoCell"));
        cell->setFixedWidth(info_column_width());
        auto *layout = new QHBoxLayout(cell);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->addWidget(info_button(cell, subject, description), 0, Qt::AlignRight);
        cells->addWidget(cell);
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
        shortcut_layout_->setSpacing(20);
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
            auto *section = category_section(shortcut_list_, group_names.at(group_index), QStringLiteral("shortcutGroupHeading"));
            auto *group = new QFrame(section);
            group->setObjectName(QStringLiteral("shortcutGroup"));
            group->setAccessibleName(group_names.at(group_index) + QStringLiteral(" shortcuts"));
            section->layout()->addWidget(group);
            auto *group_layout = new QVBoxLayout(group);
            group_layout->setContentsMargins(0, 0, 0, 0);
            group_layout->setSpacing(0);
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
                auto *title = new QLabel(name, row);
                title->setAccessibleName(name);
                title->setFixedWidth(170);
                cells->addWidget(title);
                QString scope = QStringLiteral("Workspace window");
                if (bridge_->actionScope(index) == 0)
                    scope = QStringLiteral("Folder Items List");
                else if (bridge_->actionScope(index) == 2)
                    scope = QStringLiteral("Application");
                const QString help = name + QStringLiteral(". This action is available in the ") + scope + QStringLiteral(". Add one or more shortcuts, remove individual entries, or restore its compiled default.");
                row->setAccessibleDescription(help);
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
                add_info_cell(row, cells, name, help);
                group_layout->addWidget(row);
                group_layout->addWidget(feedback);
            }
            if (group_rows == 0) {
                delete section;
            } else {
                shortcut_layout_->addWidget(section);
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
        add_info_column_heading(header, head, QStringLiteral("contextMenuColumnHeading"));
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
        context_menu_layout_->setSpacing(20);
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
                auto *section = category_section(context_menu_list_, group_name, QStringLiteral("contextMenuGroupHeading"));
                auto *group = new QFrame(section);
                group->setObjectName(QStringLiteral("contextMenuGroup"));
                group->setAccessibleName(group_name + QStringLiteral(" Context Menu actions"));
                section->layout()->addWidget(group);
                group_layout = new QVBoxLayout(group);
                group_layout->setContentsMargins(0, 0, 0, 0);
                group_layout->setSpacing(0);
                group_layout->addWidget(context_menu_table_header(group));
                context_menu_layout_->addWidget(section);
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
            auto *action_label = new QLabel(action, row);
            action_label->setObjectName(QStringLiteral("contextMenuAction"));
            action_label->setFixedWidth(260);
            cells->addWidget(action_label);
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
            add_info_cell(row, cells, action, action + QStringLiteral(" in the Folder Items context menu. Use Normal and Comprehensive to choose which menu variant shows this action."));
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
    auto file_association_page() -> QWidget * {
        auto *page = new QWidget(pages_);
        page->setObjectName(QStringLiteral("settingsFileAssociationsPage"));
        page->setAccessibleName(QStringLiteral("File Associations Settings"));
        auto *layout = new QVBoxLayout(page);
        layout->setContentsMargins(16, 16, 16, 16);
        layout->setSpacing(10);
        auto *title_row = new QHBoxLayout();
        title_row->setContentsMargins(0, 0, 0, 0);
        title_row->setSpacing(8);
        auto *title = new QLabel(QStringLiteral("File Associations"), page);
        title->setObjectName(QStringLiteral("settingsPageHeading"));
        auto title_font = title->font();
        title_font.setPointSize(title_font.pointSize() + 4);
        title->setFont(title_font);
        auto *information = info_button(page, QStringLiteral("File Associations"), QStringLiteral("Choose which macOS applications appear as Open With actions in each Folder Items Context Menu. These choices are separate from the macOS default application."));
        title_row->addWidget(title);
        title_row->addWidget(information, 0, Qt::AlignVCenter);
        title_row->addStretch();
        layout->addLayout(title_row);
        auto *tools = new QHBoxLayout();
        auto *add = new QPushButton(QStringLiteral("Add File Type…"), page);
        add->setObjectName(QStringLiteral("fileAssociationAddType"));
        add->setAccessibleName(QStringLiteral("Add File Type from Sample File"));
        auto *refresh = new QPushButton(QStringLiteral("Refresh Applications"), page);
        refresh->setObjectName(QStringLiteral("fileAssociationRefresh"));
        refresh->setAccessibleName(QStringLiteral("Refresh File Association Applications"));
        add->setEnabled(bridge_ != nullptr);
        refresh->setEnabled(bridge_ != nullptr);
        QObject::connect(add, &QPushButton::clicked, this, [this] {
            const QString path = QFileDialog::getOpenFileName(this, QStringLiteral("Choose a Sample File"));
            if (!path.isEmpty() && bridge_)
                bridge_->addFileAssociationSample(QFile::encodeName(path));
        });
        QObject::connect(refresh, &QPushButton::clicked, this, [this] { if (bridge_) bridge_->refreshFileAssociations(); });
        tools->addWidget(add);
        tools->addWidget(refresh);
        tools->addStretch();
        layout->addLayout(tools);
        auto *split = new QSplitter(Qt::Horizontal, page);
        split->setObjectName(QStringLiteral("fileAssociationSplitter"));
        file_association_types_ = new QListWidget(split);
        file_association_types_->setObjectName(QStringLiteral("fileAssociationTypeList"));
        file_association_types_->setAccessibleName(QStringLiteral("File Types"));
        file_association_types_->setMinimumWidth(340);
        file_association_detail_stack_ = new QStackedWidget(split);
        file_association_detail_stack_->setObjectName(QStringLiteral("fileAssociationDetailStack"));
        file_association_detail_scroll_ = new QScrollArea(file_association_detail_stack_);
        file_association_detail_scroll_->setWidgetResizable(true);
        file_association_detail_scroll_->setObjectName(QStringLiteral("fileAssociationDetailScroll"));
        file_association_detail_scroll_->setFrameShape(QFrame::NoFrame);
        file_association_detail_ = new QWidget(file_association_detail_scroll_);
        file_association_detail_->setObjectName(QStringLiteral("fileAssociationDetail"));
        file_association_detail_layout_ = new QVBoxLayout(file_association_detail_);
        file_association_detail_layout_->setContentsMargins(12, 0, 0, 0);
        file_association_detail_layout_->setSpacing(10);
        file_association_detail_scroll_->setWidget(file_association_detail_);
        file_association_loading_ = new QWidget(file_association_detail_stack_);
        file_association_loading_->setObjectName(QStringLiteral("fileAssociationLoading"));
        file_association_loading_->setAccessibleName(QStringLiteral("Waiting for File Associations"));
        auto *loading_layout = new QVBoxLayout(file_association_loading_);
        loading_layout->setContentsMargins(12, 0, 0, 0);
        loading_layout->addStretch();
        auto *waiting = new QLabel(QStringLiteral("Waiting…"), file_association_loading_);
        waiting->setObjectName(QStringLiteral("fileAssociationWaitingText"));
        waiting->setAlignment(Qt::AlignCenter);
        loading_layout->addWidget(waiting);
        auto *loading_indicator = new QProgressBar(file_association_loading_);
        loading_indicator->setObjectName(QStringLiteral("fileAssociationLoadingIndicator"));
        loading_indicator->setAccessibleName(QStringLiteral("Loading File Associations"));
        loading_indicator->setRange(0, 0);
        loading_indicator->setTextVisible(false);
        loading_indicator->setFixedWidth(180);
        loading_layout->addWidget(loading_indicator, 0, Qt::AlignHCenter);
        loading_layout->addStretch();
        file_association_detail_stack_->addWidget(file_association_detail_scroll_);
        file_association_detail_stack_->addWidget(file_association_loading_);
        split->addWidget(file_association_types_);
        split->addWidget(file_association_detail_stack_);
        split->setStretchFactor(1, 1);
        layout->addWidget(split, 1);
        file_association_status_ = new QLabel(page);
        file_association_status_->setObjectName(QStringLiteral("fileAssociationStatus"));
        file_association_status_->setAccessibleName(QStringLiteral("File Associations Status"));
        file_association_status_->setWordWrap(true);
        layout->addWidget(file_association_status_);
        file_association_loading_reveal_ = new QTimer(page);
        file_association_loading_reveal_->setSingleShot(true);
        file_association_loading_minimum_ = new QTimer(page);
        file_association_loading_minimum_->setSingleShot(true);
        QObject::connect(file_association_loading_reveal_, &QTimer::timeout, this, [this] {
            if (!bridge_ || !bridge_->getFileAssociationsPending() || !file_association_detail_stack_)
                return;
            file_association_loading_visible_ = true;
            file_association_detail_stack_->setCurrentWidget(file_association_loading_);
            file_association_loading_minimum_->start(file_association_loading_minimum_milliseconds);
        });
        QObject::connect(file_association_loading_minimum_, &QTimer::timeout, this, [this] { sync_file_association_loading(); });
        QObject::connect(file_association_types_, &QListWidget::currentRowChanged, this, [this] { rebuild_file_association_detail(); });
        rebuild_file_associations();
        sync_file_association_loading();
        return page;
    }
    void sync_file_association_loading() {
        if (!file_association_detail_stack_ || !file_association_loading_reveal_ || !file_association_loading_minimum_)
            return;
        const bool pending = bridge_ && bridge_->getFileAssociationsPending();
        if (pending) {
            if (!file_association_loading_visible_ && !file_association_loading_reveal_->isActive())
                file_association_loading_reveal_->start(file_association_loading_reveal_milliseconds);
            return;
        }
        file_association_loading_reveal_->stop();
        if (file_association_loading_visible_ && file_association_loading_minimum_->isActive())
            return;
        file_association_loading_visible_ = false;
        file_association_detail_stack_->setCurrentWidget(file_association_detail_scroll_);
    }
    void rebuild_file_associations() {
        if (!file_association_types_ || !bridge_)
            return;
        QString selected = pending_association_focus_;
        if (selected.isEmpty() && file_association_types_->currentItem())
            selected = file_association_types_->currentItem()->data(Qt::UserRole).toString();
        pending_association_focus_.clear();
        const QSignalBlocker block(file_association_types_);
        file_association_types_->clear();
        int selected_row = -1;
        for (int index = 0; index < bridge_->fileAssociationTypeCount(); ++index) {
            const QString kind = bridge_->fileAssociationTypeKind(index);
            const QString extension = bridge_->fileAssociationTypeExtension(index);
            const QString identifier = bridge_->fileAssociationTypeIdentifier(index);
            QString label = kind;
            if (!extension.isEmpty())
                label += QStringLiteral(" (.") + extension + QStringLiteral(")");
            auto *item = new QListWidgetItem(label, file_association_types_);
            item->setData(Qt::UserRole, identifier);
            item->setToolTip(identifier);
            item->setData(Qt::AccessibleDescriptionRole, identifier);
            if (identifier == selected)
                selected_row = index;
        }
        if (selected_row < 0 && file_association_types_->count() > 0)
            selected_row = 0;
        file_association_types_->setCurrentRow(selected_row);
        rebuild_file_association_detail();
        sync_file_association_status();
    }
    // Updates the existing Normal/Comprehensive toggles and default button in
    // place for the already-built detail panel, instead of tearing it down.
    // Rebuilding on every toggle click destroyed the very ToggleSwitch that
    // was mid-click (the revision signal fires synchronously, inside the
    // click), which cost focus and made the toggles feel unreliable — the
    // same failure sync_context_menus() already avoids for the Context
    // Menus tab. Returns false when the panel must be rebuilt instead
    // (nothing built yet, a different type selected, or the candidate list
    // changed shape).
    bool sync_file_association_detail() {
        if (!bridge_ || !file_association_types_ || !file_association_default_label_)
            return false;
        const int type_index = file_association_types_->currentRow();
        if (type_index < 0 || type_index != file_association_detail_row_)
            return false;
        const auto count = static_cast<std::size_t>(bridge_->fileAssociationCandidateCount(type_index));
        if (count != file_association_name_labels_.size() || count != file_association_path_labels_.size() || count != file_association_normal_toggles_.size() || count != file_association_comprehensive_toggles_.size() || count != file_association_default_buttons_.size())
            return false;
        const QString kind = bridge_->fileAssociationTypeKind(type_index);
        QString default_name = QStringLiteral("None reported by macOS");
        for (std::size_t app = 0; app < count; ++app) {
            if (bridge_->fileAssociationCandidateDefault(type_index, static_cast<int>(app))) {
                default_name = bridge_->fileAssociationCandidateName(type_index, static_cast<int>(app));
                break;
            }
        }
        file_association_default_label_->setText(QStringLiteral("Current macOS default: ") + default_name);
        file_association_default_label_->setAccessibleName(file_association_default_label_->text());
        for (std::size_t app = 0; app < count; ++app) {
            auto *normal = file_association_normal_toggles_.at(app).data();
            auto *comprehensive = file_association_comprehensive_toggles_.at(app).data();
            auto *default_button = file_association_default_buttons_.at(app).data();
            auto *name_label = file_association_name_labels_.at(app).data();
            auto *path_label = file_association_path_labels_.at(app).data();
            if (!normal || !comprehensive || !default_button || !name_label || !path_label)
                return false;
            const int index = static_cast<int>(app);
            const QString name = bridge_->fileAssociationCandidateName(type_index, index);
            const QString location = bridge_->fileAssociationCandidateLocation(type_index, index);
            const bool available = bridge_->fileAssociationCandidateAvailable(type_index, index);
            const bool is_default = bridge_->fileAssociationCandidateDefault(type_index, index);
            name_label->setText(name + (available ? QString() : QStringLiteral(" — unavailable")));
            name_label->setAccessibleName(name + (available ? QString() : QStringLiteral(", unavailable")));
            path_label->setText(location);
            path_label->setAccessibleName(QStringLiteral("Application path: ") + location);
            {
                const QSignalBlocker block(normal);
                normal->setAccessibleName(QStringLiteral("Show Open with ") + name + QStringLiteral(" in Normal Context Menu"));
                normal->setChecked(bridge_->fileAssociationCandidateNormal(type_index, index));
                normal->setEnabled(available || normal->isChecked());
            }
            {
                const QSignalBlocker block(comprehensive);
                comprehensive->setAccessibleName(QStringLiteral("Show Open with ") + name + QStringLiteral(" in Comprehensive Context Menu"));
                comprehensive->setChecked(bridge_->fileAssociationCandidateComprehensive(type_index, index));
                comprehensive->setEnabled(available || comprehensive->isChecked());
            }
            default_button->setText(is_default ? QStringLiteral("Current Default") : QStringLiteral("Set as Default…"));
            default_button->setAccessibleName(is_default ? name + QStringLiteral(" is the current default") : QStringLiteral("Set ") + name + QStringLiteral(" as Default for ") + kind);
            default_button->setEnabled(available && !is_default && !bridge_->getFileAssociationsPending());
            QObject::disconnect(default_button, &QPushButton::clicked, this, nullptr);
            QObject::connect(default_button, &QPushButton::clicked, this, [this, type_index, index, name, kind, default_name] { confirm_default_application(type_index, index, name, kind, default_name); });
        }
        return true;
    }
    void rebuild_file_association_detail() {
        if (!file_association_detail_layout_)
            return;
        if (sync_file_association_detail())
            return;
        while (auto *item = file_association_detail_layout_->takeAt(0)) {
            if (auto *widget = item->widget()) {
                widget->hide();
                widget->deleteLater();
            }
            delete item;
        }
        file_association_default_label_.clear();
        file_association_name_labels_.clear();
        file_association_path_labels_.clear();
        file_association_normal_toggles_.clear();
        file_association_comprehensive_toggles_.clear();
        file_association_default_buttons_.clear();
        const int type_index = file_association_types_ ? file_association_types_->currentRow() : -1;
        file_association_detail_row_ = type_index;
        if (!bridge_ || type_index < 0) {
            auto *empty = new QLabel(bridge_ && bridge_->getFileAssociationsPending() ? QStringLiteral("Identifying file types and applications…") : QStringLiteral("No file types are available. Open a folder containing files or choose Add File Type… with a sample file."), file_association_detail_);
            empty->setWordWrap(true);
            empty->setAccessibleName(empty->text());
            file_association_detail_layout_->addWidget(empty);
            file_association_detail_layout_->addStretch();
            return;
        }
        const QString kind = bridge_->fileAssociationTypeKind(type_index);
        const QString identifier = bridge_->fileAssociationTypeIdentifier(type_index);
        const QString extension = bridge_->fileAssociationTypeExtension(type_index);
        auto *heading = new QLabel(kind, file_association_detail_);
        heading->setObjectName(QStringLiteral("fileAssociationKind"));
        auto heading_font = heading->font();
        heading_font.setPointSize(heading_font.pointSize() + 4);
        heading->setFont(heading_font);
        file_association_detail_layout_->addWidget(heading);
        auto *identity = new QLabel(QStringLiteral("Content type: ") + identifier + (extension.isEmpty() ? QString() : QStringLiteral("\nPreferred extension: .") + extension), file_association_detail_);
        identity->setObjectName(QStringLiteral("fileAssociationIdentity"));
        identity->setTextInteractionFlags(Qt::TextSelectableByKeyboard | Qt::TextSelectableByMouse);
        identity->setAccessibleName(identity->text());
        file_association_detail_layout_->addWidget(identity);
        QString default_name = QStringLiteral("None reported by macOS");
        for (int app = 0; app < bridge_->fileAssociationCandidateCount(type_index); ++app) {
            if (bridge_->fileAssociationCandidateDefault(type_index, app)) {
                default_name = bridge_->fileAssociationCandidateName(type_index, app);
                break;
            }
        }
        auto *current_default = new QLabel(QStringLiteral("Current macOS default: ") + default_name, file_association_detail_);
        current_default->setObjectName(QStringLiteral("fileAssociationDefault"));
        current_default->setAccessibleName(current_default->text());
        file_association_detail_layout_->addWidget(current_default);
        file_association_default_label_ = current_default;
        auto *table = new QWidget(file_association_detail_);
        table->setObjectName(QStringLiteral("fileAssociationApplicationsTable"));
        auto *grid = new QGridLayout(table);
        grid->setContentsMargins(0, 0, 0, 0);
        grid->setHorizontalSpacing(16);
        grid->setVerticalSpacing(10);
        grid->setColumnStretch(0, 1);
        grid->setColumnMinimumWidth(0, 260);
        const std::array<QString, 4> headings = {QStringLiteral("Applications"), QStringLiteral("Normal"), QStringLiteral("Comprehensive"), QStringLiteral("Default")};
        for (int column = 0; column < static_cast<int>(headings.size()); ++column) {
            auto *header = new QLabel(headings.at(static_cast<std::size_t>(column)), table);
            header->setObjectName(QStringLiteral("fileAssociationColumnHeading"));
            grid->addWidget(header, 0, column, Qt::AlignLeft | Qt::AlignVCenter);
        }
        const int count = bridge_->fileAssociationCandidateCount(type_index);
        for (int app = 0; app < count; ++app) {
            const QString name = bridge_->fileAssociationCandidateName(type_index, app);
            const QString location = bridge_->fileAssociationCandidateLocation(type_index, app);
            const bool available = bridge_->fileAssociationCandidateAvailable(type_index, app);
            auto *application = new QWidget(table);
            application->setObjectName(QStringLiteral("fileAssociationApplicationDescription"));
            auto *application_layout = new QVBoxLayout(application);
            application_layout->setContentsMargins(0, 0, 0, 0);
            application_layout->setSpacing(2);
            auto *name_label = new QLabel(name + (available ? QString() : QStringLiteral(" — unavailable")), application);
            name_label->setObjectName(QStringLiteral("fileAssociationApplicationName"));
            name_label->setAccessibleName(name + (available ? QString() : QStringLiteral(", unavailable")));
            auto *path_label = new QLabel(location, application);
            path_label->setObjectName(QStringLiteral("fileAssociationApplicationPath"));
            path_label->setWordWrap(true);
            path_label->setTextInteractionFlags(Qt::TextSelectableByKeyboard | Qt::TextSelectableByMouse);
            path_label->setAccessibleName(QStringLiteral("Application path: ") + location);
            application_layout->addWidget(name_label);
            application_layout->addWidget(path_label);
            grid->addWidget(application, app + 1, 0);
            file_association_name_labels_.emplace_back(name_label);
            file_association_path_labels_.emplace_back(path_label);
            auto add_choice = [this, table, grid, type_index, app, name, available](int menu_kind, bool checked) {
                auto *choice = new ToggleSwitch(table);
                const QString menu = menu_kind == 0 ? QStringLiteral("Normal") : QStringLiteral("Comprehensive");
                choice->setObjectName(menu_kind == 0 ? QStringLiteral("fileAssociationNormal") : QStringLiteral("fileAssociationComprehensive"));
                choice->setAccessibleName(QStringLiteral("Show Open with ") + name + QStringLiteral(" in ") + menu + QStringLiteral(" Context Menu"));
                choice->setChecked(checked);
                choice->setEnabled(available || checked);
                QObject::connect(choice, &QCheckBox::toggled, this, [this, type_index, app, menu_kind](bool selected) { if (bridge_) bridge_->changeFileAssociationSelection(type_index, app, menu_kind, selected); });
                grid->addWidget(choice, app + 1, menu_kind + 1, Qt::AlignLeft | Qt::AlignTop);
                if (menu_kind == 0)
                    file_association_normal_toggles_.emplace_back(choice);
                else
                    file_association_comprehensive_toggles_.emplace_back(choice);
            };
            add_choice(0, bridge_->fileAssociationCandidateNormal(type_index, app));
            add_choice(1, bridge_->fileAssociationCandidateComprehensive(type_index, app));
            auto *make_default = new QPushButton(bridge_->fileAssociationCandidateDefault(type_index, app) ? QStringLiteral("Current Default") : QStringLiteral("Set as Default…"), table);
            make_default->setObjectName(QStringLiteral("fileAssociationSetDefault"));
            make_default->setAccessibleName(bridge_->fileAssociationCandidateDefault(type_index, app) ? name + QStringLiteral(" is the current default") : QStringLiteral("Set ") + name + QStringLiteral(" as Default for ") + kind);
            make_default->setEnabled(available && !bridge_->fileAssociationCandidateDefault(type_index, app) && !bridge_->getFileAssociationsPending());
            QObject::connect(make_default, &QPushButton::clicked, this, [this, type_index, app, name, kind, default_name] { confirm_default_application(type_index, app, name, kind, default_name); });
            grid->addWidget(make_default, app + 1, 3, Qt::AlignLeft | Qt::AlignTop);
            file_association_default_buttons_.emplace_back(make_default);
        }
        file_association_detail_layout_->addWidget(table);
        if (count == 0) {
            auto *none = new QLabel(QStringLiteral("macOS reports no available applications for this content type."), file_association_detail_);
            none->setAccessibleName(none->text());
            file_association_detail_layout_->addWidget(none);
        }
        auto *action_row = new QWidget(file_association_detail_);
        action_row->setObjectName(QStringLiteral("fileAssociationActions"));
        auto *actions = new QHBoxLayout(action_row);
        actions->setContentsMargins(0, 0, 0, 0);
        auto *clear = new QPushButton(QStringLiteral("Clear Open With Choices…"), action_row);
        clear->setObjectName(QStringLiteral("fileAssociationClear"));
        clear->setAccessibleName(QStringLiteral("Clear Open With Choices for ") + kind);
        auto *remove = new QPushButton(QStringLiteral("Remove Type…"), action_row);
        remove->setObjectName(QStringLiteral("fileAssociationRemove"));
        remove->setAccessibleName(QStringLiteral("Remove File Type ") + kind);
        remove->setEnabled(bridge_->fileAssociationTypePinned(type_index));
        QObject::connect(clear, &QPushButton::clicked, this, [this, type_index, kind] { confirm_file_association_local_change(type_index, kind, false); });
        QObject::connect(remove, &QPushButton::clicked, this, [this, type_index, kind] { confirm_file_association_local_change(type_index, kind, true); });
        actions->addWidget(clear);
        actions->addWidget(remove);
        actions->addStretch();
        file_association_detail_layout_->addWidget(action_row);
        file_association_detail_layout_->addStretch();
    }
    void confirm_default_application(int type_index, int app_index, const QString &application, const QString &kind, const QString &old_default) {
        auto *box = new QMessageBox(QMessageBox::Warning, QStringLiteral("Set Default Application"), QStringLiteral("Change the macOS default for %1 from %2 to %3 for the current macOS user?\n\nThis type-wide choice can affect Finder and other applications. macOS may ask for additional consent.").arg(kind, old_default, application), QMessageBox::NoButton, this);
        box->setObjectName(QStringLiteral("fileAssociationDefaultConfirmation"));
        auto *confirm = box->addButton(QStringLiteral("Set as Default"), QMessageBox::AcceptRole);
        auto *cancel = box->addButton(QMessageBox::Cancel);
        box->setDefaultButton(cancel);
        box->setEscapeButton(cancel);
        box->setWindowModality(Qt::WindowModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, this, [this, box, confirm, type_index, app_index] { if (box->clickedButton() == confirm && bridge_) bridge_->setDefaultApplication(type_index, app_index); });
        box->open();
    }
    void confirm_file_association_local_change(int type_index, const QString &kind, bool remove) {
        const QString action = remove ? QStringLiteral("Remove Type") : QStringLiteral("Clear Choices");
        const QString question = remove ? QStringLiteral("Remove %1 from Dual Pane and clear its Normal and Comprehensive Open With choices?\n\nThis does not change the macOS default application.").arg(kind) : QStringLiteral("Clear the Normal and Comprehensive Open With choices for %1?\n\nThis does not change the macOS default application.").arg(kind);
        auto *box = new QMessageBox(QMessageBox::Question, action, question, QMessageBox::NoButton, this);
        box->setObjectName(remove ? QStringLiteral("fileAssociationRemoveConfirmation") : QStringLiteral("fileAssociationClearConfirmation"));
        auto *confirm = box->addButton(action, QMessageBox::AcceptRole);
        auto *cancel = box->addButton(QMessageBox::Cancel);
        box->setDefaultButton(cancel);
        box->setEscapeButton(cancel);
        box->setWindowModality(Qt::WindowModal);
        box->setAttribute(Qt::WA_DeleteOnClose);
        QObject::connect(box, &QMessageBox::finished, this, [this, box, confirm, type_index, remove] {
            if (box->clickedButton() != confirm || !bridge_)
                return;
            if (remove)
                bridge_->removeFileAssociation(type_index);
            else
                bridge_->clearFileAssociation(type_index);
        });
        box->open();
    }
    void sync_file_association_status() {
        if (!file_association_status_ || !bridge_)
            return;
        QString text = bridge_->getFileAssociationsError();
        if (text.isEmpty() && bridge_->getFileAssociationsPending())
            text = QStringLiteral("Identifying file types and refreshing applications…");
        if (text.isEmpty())
            text = QStringLiteral("Application choices are saved by Dual Pane. The current default is read from macOS.");
        file_association_status_->setText(text);
        file_association_status_->setAccessibleName(text);
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
    QListWidget *file_association_types_ = nullptr;
    QScrollArea *file_association_detail_scroll_ = nullptr;
    QStackedWidget *file_association_detail_stack_ = nullptr;
    QWidget *file_association_detail_ = nullptr;
    QWidget *file_association_loading_ = nullptr;
    QVBoxLayout *file_association_detail_layout_ = nullptr;
    QLabel *file_association_status_ = nullptr;
    QTimer *file_association_loading_reveal_ = nullptr;
    QTimer *file_association_loading_minimum_ = nullptr;
    bool file_association_loading_visible_ = false;
    QString pending_association_focus_;
    int file_association_detail_row_ = -1;
    QPointer<QLabel> file_association_default_label_;
    std::vector<QPointer<QLabel>> file_association_name_labels_;
    std::vector<QPointer<QLabel>> file_association_path_labels_;
    std::vector<QPointer<ToggleSwitch>> file_association_normal_toggles_;
    std::vector<QPointer<ToggleSwitch>> file_association_comprehensive_toggles_;
    std::vector<QPointer<QPushButton>> file_association_default_buttons_;
    static constexpr int file_association_loading_reveal_milliseconds = 150;
    static constexpr int file_association_loading_minimum_milliseconds = 500;
};

class SettingsWindowController final : public QObject {
  public:
    explicit SettingsWindowController(QWidget *owner, WorkspaceBridge *bridge = nullptr) : QObject(owner), owner_(owner), bridge_(bridge), shortcut_(new QShortcut(QKeySequence(Qt::CTRL | Qt::Key_Comma), owner)) {
        shortcut_->setContext(Qt::WindowShortcut);
        shortcut_->setEnabled(false);
        QObject::connect(shortcut_, &QShortcut::activated, this, [this] { present(); });
        if (bridge_)
            QObject::connect(bridge_, &WorkspaceBridge::fileAssociationOpenRequestsChanged, this, [this] { present_file_association(bridge_->takeFileAssociationType()); });
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
    void present_file_association(const QString &content_type) {
        if (!available_)
            return;
        present();
        if (dialog_)
            dialog_->focus_file_association(content_type);
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
