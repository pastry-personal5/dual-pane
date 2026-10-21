#include "dual_pane_desktop/shortcuts.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

auto shortcut_glyph_bitmap(ShortcutGlyph glyph, const QColor &foreground) -> QPixmap {
    constexpr int logical_size = 18;
    constexpr qreal bitmap_scale = 3.0;
    QPixmap bitmap(logical_size * static_cast<int>(bitmap_scale), logical_size * static_cast<int>(bitmap_scale));
    bitmap.setDevicePixelRatio(bitmap_scale);
    bitmap.fill(Qt::transparent);
    QPainter painter(&bitmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(foreground, 1.7, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    switch (glyph) {
    case ShortcutGlyph::Info: {
        painter.drawEllipse(QRectF(2, 2, 14, 14));
        QFont font = painter.font();
        font.setPointSize(10);
        font.setBold(true);
        painter.setFont(font);
        painter.drawText(QRect(0, 0, logical_size, logical_size), Qt::AlignCenter, QStringLiteral("i"));
        break;
    }
    case ShortcutGlyph::Add:
        painter.drawLine(QPointF(9, 3.5), QPointF(9, 14.5));
        painter.drawLine(QPointF(3.5, 9), QPointF(14.5, 9));
        break;
    case ShortcutGlyph::Remove:
        painter.drawLine(QPointF(4.5, 4.5), QPointF(13.5, 13.5));
        painter.drawLine(QPointF(13.5, 4.5), QPointF(4.5, 13.5));
        break;
    case ShortcutGlyph::Restore:
        painter.drawArc(QRectF(3.5, 3.5, 11, 11), 35 * 16, 290 * 16);
        painter.drawLine(QPointF(13.3, 4.2), QPointF(14.3, 7.2));
        painter.drawLine(QPointF(14.3, 7.2), QPointF(11.2, 6.7));
        break;
    }
    return bitmap;
}

/// Whether `widget` edits text. Every text field, including a read-only name
/// editor waiting on its job, keeps Command+C, Command+X, Command+V, and
/// Command+A as text editing, so the workspace's own shortcuts stand aside.
auto edits_text(const QWidget *widget) -> bool {
    return qobject_cast<const QLineEdit *>(widget) != nullptr;
}

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
