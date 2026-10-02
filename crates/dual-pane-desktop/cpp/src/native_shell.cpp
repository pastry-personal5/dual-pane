#include "dual_pane_desktop/native_shell.hpp"

#include <QtCore/QByteArray>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QString>
#include <QtCore/QUrl>
#include <QtGui/QDesktopServices>

namespace dual_pane_desktop {
namespace {

auto file_name(::rust::Slice<const std::uint8_t> path) -> QString {
    // QByteArray takes characters; the bytes are reinterpreted, not changed.
    // NOLINTNEXTLINE(cppcoreguidelines-pro-type-reinterpret-cast)
    return QFile::decodeName(QByteArray(reinterpret_cast<const char *>(path.data()), static_cast<qsizetype>(path.size())));
}

} // namespace

auto move_to_trash(::rust::Slice<const std::uint8_t> path, ::rust::Vec<std::uint8_t> &trashed) -> bool {
    QString in_trash;
    if (!QFile::moveToTrash(file_name(path), &in_trash)) {
        return false;
    }
    const QByteArray encoded = QFile::encodeName(in_trash);
    trashed.clear();
    trashed.reserve(static_cast<std::size_t>(encoded.size()));
    for (const char byte : encoded) {
        trashed.push_back(static_cast<std::uint8_t>(byte));
    }
    return true;
}

auto is_bundle(::rust::Slice<const std::uint8_t> path) -> bool {
    return QFileInfo(file_name(path)).isBundle();
}

auto open_with_default_application(::rust::Slice<const std::uint8_t> path) -> bool {
    return QDesktopServices::openUrl(QUrl::fromLocalFile(file_name(path)));
}

} // namespace dual_pane_desktop
