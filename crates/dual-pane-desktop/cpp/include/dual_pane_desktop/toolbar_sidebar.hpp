#ifndef dual_pane_DESKTOP_TOOLBAR_SIDEBAR_HPP
#define dual_pane_DESKTOP_TOOLBAR_SIDEBAR_HPP

#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/favorites_panel.hpp"
#include "dual_pane_desktop/folder_pane.hpp"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/notices_glyph.hpp"
#include "dual_pane_desktop/settings_glyph.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

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

auto application_close_icon() -> const QIcon &;

/// Quits from the Application Title Row. It paints nothing until the pointer
/// is over it, but it still takes the pointer, so the row never drags there.
class ApplicationCloseButton final : public QAbstractButton {
  public:
    explicit ApplicationCloseButton(QWidget *parent) : QAbstractButton(parent) {
        setObjectName(QStringLiteral("applicationCloseButton"));
        setAccessibleName(QStringLiteral("Application Close Button"));
        setToolTip(QStringLiteral("Quit Dual Pane"));
        // Command+Q is the keyboard path; an invisible control stays out of the Tab chain.
        setFocusPolicy(Qt::NoFocus);
        setIcon(application_close_icon());
        setIconSize(QSize(16, 16));
        setFixedSize(application_close_button_size, application_close_button_size);
    }

  protected:
    void enterEvent(QEnterEvent *event) override {
        hovered_ = true;
        update();
        QAbstractButton::enterEvent(event);
    }
    void leaveEvent(QEvent *event) override {
        hovered_ = false;
        update();
        QAbstractButton::leaveEvent(event);
    }
    void paintEvent(QPaintEvent *) override {
        if (!hovered_ && !isDown())
            return;
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor(QString::fromLatin1(isDown() ? application_close_pressed_color : application_close_hover_color)));
        painter.drawRoundedRect(QRectF(rect()), 4, 4);
        QRect glyph(QPoint(), iconSize());
        glyph.moveCenter(rect().center());
        icon().paint(&painter, glyph);
    }

  private:
    bool hovered_ = false;
};

/// The confirmation the Application Close Button shows before quitting.
auto make_application_quit_confirmation(QWidget *owner, bool offer_dont_ask) -> QMessageBox *;

class Sidebar final : public QWidget {
  public:
    Sidebar(WorkspaceBridge *bridge, const std::function<void()> &focus_active_list, const std::function<void()> &show_notices, const std::function<void()> &show_settings, const std::function<void()> &close_application, QWidget *parent) : QWidget(parent), toolbar_(new MainToolbar(this, show_notices, show_settings)) {
        setObjectName(QStringLiteral("sidebar"));
        setAccessibleName(QStringLiteral("Sidebar"));
        auto *layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        auto *title = new SidebarTitleRow(close_application, this);
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
    class SidebarTitleRow final : public QWidget {
      public:
        SidebarTitleRow(const std::function<void()> &close_application, QWidget *parent) : QWidget(parent) {
            setObjectName(QStringLiteral("applicationTitleRow"));
            setAccessibleName(QStringLiteral("Application Title Row"));
            setMinimumHeight(54);
            // The heading starts where Favorite Group names start and the close
            // button ends where Add Favorite Item Buttons end. The top inset
            // matches the first group's top margin, so both center between the
            // Sidebar top and the first Favorite Group box.
            constexpr int inset = favorite_group_margin + favorite_row_horizontal_padding;
            auto *row = new QHBoxLayout(this);
            row->setContentsMargins(inset, favorite_group_margin, inset, 0);
            row->setSpacing(0);
            auto *heading = new QLabel(QStringLiteral("Dual Pane"), this);
            heading->setObjectName(QStringLiteral("applicationTitle"));
            heading->setAccessibleName(QStringLiteral("Dual Pane"));
            heading->setAttribute(Qt::WA_TransparentForMouseEvents);
            row->addWidget(heading, 1);
            auto *close = new ApplicationCloseButton(this);
            QObject::connect(close, &QAbstractButton::clicked, this, close_application);
            row->addWidget(close, 0, Qt::AlignVCenter);
        }

      protected:
        void mousePressEvent(QMouseEvent *event) override {
            if (event->button() == Qt::LeftButton && window()->windowHandle() != nullptr && window()->windowHandle()->startSystemMove()) {
                moving_window_ = true;
                event->accept();
                return;
            }
            QWidget::mousePressEvent(event);
        }
        void mouseReleaseEvent(QMouseEvent *event) override {
            if (moving_window_) {
                moving_window_ = false;
                event->accept();
                return;
            }
            QWidget::mouseReleaseEvent(event);
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

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_TOOLBAR_SIDEBAR_HPP
