#include "dual_pane_desktop/preview_reader.hpp"

#include <QtCore/QFile>
#include <QtGui/QImage>
#include <QtGui/QImageIOHandler>
#include <QtGui/QImageReader>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <limits>

namespace dual_pane_desktop {
namespace {
constexpr std::uint32_t supported = 0;
constexpr std::uint32_t unsupported = 1;
constexpr std::uint32_t unavailable = 2;
constexpr std::uint32_t decode_failed = 3;
constexpr std::int64_t maximum_source_pixels = 64'000'000;
constexpr std::int64_t maximum_delivered_pixels = 16'000'000;
constexpr int header_size = 20;

void write_u32(QByteArray &bytes, int offset, std::uint32_t value) {
    for (int index = 0; index < 4; ++index)
        bytes[offset + index] = static_cast<char>((value >> (index * 8)) & 0xffU);
}

auto failure(std::uint32_t status) -> QByteArray {
    QByteArray bytes(header_size, '\0');
    write_u32(bytes, 0, status);
    return bytes;
}
} // namespace

auto decode_preview(int fd, int target_width, int target_height) -> QByteArray {
    if (fd < 0 || target_width <= 0 || target_height <= 0)
        return failure(unavailable);
    QFile file;
    if (!file.open(fd, QIODevice::ReadOnly, QFileDevice::DontCloseHandle))
        return failure(unavailable);
    QImageReader reader(&file);
    reader.setDecideFormatFromContent(true);
    reader.setAutoTransform(true);
    if (!reader.canRead())
        return failure(reader.error() == QImageReader::UnsupportedFormatError ? unsupported : decode_failed);
    const QSize source = reader.size();
    if (!source.isValid() || static_cast<std::int64_t>(source.width()) * source.height() > maximum_source_pixels)
        return failure(decode_failed);
    const bool rotated = reader.transformation().testFlag(QImageIOHandler::TransformationRotate90);
    const QSize oriented = rotated ? QSize(source.height(), source.width()) : source;
    const QSize display = oriented.width() > target_width || oriented.height() > target_height ? oriented.scaled(QSize(target_width, target_height), Qt::KeepAspectRatio) : oriented;
    if (!display.isValid())
        return failure(decode_failed);
    reader.setScaledSize(rotated ? QSize(display.height(), display.width()) : display);
    QImage image = reader.read();
    if (image.isNull())
        return failure(reader.error() == QImageReader::UnsupportedFormatError ? unsupported : decode_failed);
    if (image.width() > target_width || image.height() > target_height)
        image = image.scaled(QSize(target_width, target_height), Qt::KeepAspectRatio, Qt::SmoothTransformation);
    if (image.isNull() || static_cast<std::int64_t>(image.width()) * image.height() > maximum_delivered_pixels)
        return failure(decode_failed);
    image = image.convertToFormat(QImage::Format_RGBA8888_Premultiplied);
    const std::int64_t pixel_bytes = static_cast<std::int64_t>(image.width()) * image.height() * 4;
    if (pixel_bytes > std::numeric_limits<int>::max() - header_size)
        return failure(decode_failed);
    QByteArray bytes(header_size + static_cast<int>(pixel_bytes), '\0');
    write_u32(bytes, 0, supported);
    write_u32(bytes, 4, static_cast<std::uint32_t>(image.width()));
    write_u32(bytes, 8, static_cast<std::uint32_t>(image.height()));
    write_u32(bytes, 12, static_cast<std::uint32_t>(oriented.width()));
    write_u32(bytes, 16, static_cast<std::uint32_t>(oriented.height()));
    const auto stride = static_cast<std::size_t>(image.width()) * 4;
    for (int row = 0; row < image.height(); ++row)
        std::memcpy(bytes.data() + header_size + static_cast<std::size_t>(row) * stride, image.constScanLine(row), stride);
    return bytes;
}

} // namespace dual_pane_desktop
