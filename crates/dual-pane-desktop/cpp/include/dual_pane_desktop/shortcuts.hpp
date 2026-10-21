#ifndef dual_pane_DESKTOP_SHORTCUTS_HPP
#define dual_pane_DESKTOP_SHORTCUTS_HPP

#include "dual-pane-desktop/src/workspace_bridge.cxxqt.h"
#include "dual_pane_desktop/gui_common.hpp"
#include "dual_pane_desktop/keyboard_layout.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

enum class ShortcutGlyph : std::uint8_t { Info,
                                          Add,
                                          Remove,
                                          Restore,
};

auto shortcut_glyph_bitmap(ShortcutGlyph glyph, const QColor &foreground) -> QPixmap;

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

/// Whether `widget` edits text. Every text field, including a read-only name
/// editor waiting on its job, keeps Command+C, Command+X, Command+V, and
/// Command+A as text editing, so the workspace's own shortcuts stand aside.
auto edits_text(const QWidget *widget) -> bool;

/// Binds every delivered action to its effective shortcut from settings and
/// rebinds after a load or reset. File commands on Folder Items act only
/// while a Folder Items List has focus. While a text field has focus, only
/// Quit stays active.
class ShortcutBinder final {
  public:
    using Handler = std::function<void()>;
    using ListHandler = std::function<void(int)>;
    struct Binding {
        QString action;
        /// Qt portable key-sequence text, empty when unbound.
        QString sequence;
        int scope = -1;
    };
    ShortcutBinder(QWidget *window, WorkspaceBridge *bridge, QHash<QString, Handler> handlers, std::array<QWidget *, 2> lists, QHash<QString, ListHandler> list_handlers) : window_(window), bridge_(bridge), handlers_(std::move(handlers)), lists_(lists), list_handlers_(std::move(list_handlers)) {}

    void rebind() {
        std::vector<Binding> bindings;
        bindings.reserve(static_cast<std::size_t>(qMax(0, bridge_->bindingCount())));
        for (int index = 0; index < bridge_->bindingCount(); ++index)
            bindings.push_back({bridge_->bindingAction(index), bridge_->bindingSequence(index), bridge_->bindingScope(index)});
        rebind(bindings);
    }
    void rebind(const std::vector<Binding> &bindings) {
        for (auto *shortcut : shortcuts_) {
            // deleteLater leaves the old binding alive until the event loop
            // resumes; it must stop matching keys before replacements exist.
            shortcut->setEnabled(false);
            shortcut->deleteLater();
        }
        shortcuts_.clear();
        for (const auto &[action, sequence, scope] : bindings) {
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
    /// Follows application focus, so a focused text field keeps its keys.
    void followFocus(const QWidget *now) {
        editing_ = edits_text(now);
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

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)

#endif // dual_pane_DESKTOP_SHORTCUTS_HPP
