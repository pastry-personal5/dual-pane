#include "dual_pane_desktop/toolbar_sidebar.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

auto application_close_icon() -> const QIcon & {
    static const QIcon icon = [] {
        QIcon result;
        for (const qreal scale : {1.0, 2.0, 3.0}) {
            QPixmap pixmap(QSize(static_cast<int>(16 * scale), static_cast<int>(16 * scale)));
            pixmap.setDevicePixelRatio(scale);
            pixmap.fill(Qt::transparent);
            QPainter painter(&pixmap);
            painter.setRenderHint(QPainter::Antialiasing);
            painter.setPen(QPen(QColor(QString::fromLatin1(application_close_glyph_color)), 1.7, Qt::SolidLine, Qt::RoundCap));
            painter.drawLine(QPointF(3.5, 3.5), QPointF(12.5, 12.5));
            painter.drawLine(QPointF(12.5, 3.5), QPointF(3.5, 12.5));
            painter.end();
            result.addPixmap(pixmap);
        }
        return result;
    }();
    return icon;
}

/// The confirmation the Application Close Button shows before quitting.
auto make_application_quit_confirmation(QWidget *owner, bool offer_dont_ask) -> QMessageBox * {
    auto *box = new QMessageBox(QMessageBox::Question, QStringLiteral("Quit Dual Pane"), QStringLiteral("Are you sure you want to quit Dual Pane?"), QMessageBox::NoButton, owner);
    box->setObjectName(QStringLiteral("applicationQuitConfirmationWindow"));
    box->setAccessibleName(QStringLiteral("Application Quit Confirmation Window"));
    auto *quit = box->addButton(QStringLiteral("Quit"), QMessageBox::AcceptRole);
    auto *cancel = box->addButton(QMessageBox::Cancel);
    box->setDefaultButton(quit);
    box->setEscapeButton(cancel);
    if (offer_dont_ask) {
        auto *dont_ask = new QCheckBox(QStringLiteral("Don’t ask again"), box);
        dont_ask->setObjectName(QStringLiteral("applicationQuitDontAsk"));
        box->setCheckBox(dont_ask);
    }
    return box;
}

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
