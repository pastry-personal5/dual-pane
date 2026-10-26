#ifndef dual_pane_DESKTOP_SIDEBAR_SYMBOLS_HPP
#define dual_pane_DESKTOP_SIDEBAR_SYMBOLS_HPP

#include <QtCore/QString>
#include <QtGui/QImage>

namespace dual_pane_desktop::gui {

/// Picks a built-in symbol by the Favorite target, never by its editable alias.
auto favorite_symbol_name(const QString &path, const QString &home) -> QString;

/// Renders a macOS SF Symbol as a transparent 16-point image at Retina scale.
/// The caller applies its Sidebar tint while painting.
auto favorite_symbol_image(const QString &symbol_name) -> QImage;

} // namespace dual_pane_desktop::gui

#endif
