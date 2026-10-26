#import <AppKit/AppKit.h>

#include "dual_pane_desktop/sidebar_symbols.hpp"

#include <QtCore/QDir>
#include <QtCore/QHash>
#include <QtGui/QPainter>

namespace dual_pane_desktop::gui {

auto favorite_symbol_name(const QString &path, const QString &home) -> QString {
    const auto target = QDir::cleanPath(path);
    const auto base = QDir::cleanPath(home);
    if (target == base)
        return QStringLiteral("house");
    if (target == QStringLiteral("/Applications") || target == base + QStringLiteral("/Applications"))
        return QStringLiteral("square.grid.2x2");
    if (target == base + QStringLiteral("/Desktop"))
        return QStringLiteral("desktopcomputer");
    if (target == base + QStringLiteral("/Documents/Screenshots"))
        return QStringLiteral("camera.viewfinder");
    if (target == base + QStringLiteral("/Documents"))
        return QStringLiteral("doc");
    if (target == base + QStringLiteral("/Downloads"))
        return QStringLiteral("arrow.down.circle");
    if (target == base + QStringLiteral("/Pictures"))
        return QStringLiteral("photo");
    return QStringLiteral("folder");
}

auto favorite_symbol_image(const QString &symbol_name) -> QImage {
    // Favorites are rebuilt after edits and probes. AppKit rendering and the PNG
    // conversion only need to happen once for each symbol in this GUI thread.
    static QHash<QString, QImage> images;
    if (const auto found = images.constFind(symbol_name); found != images.cend())
        return *found;
    constexpr int logical_size = 16;
    constexpr int scale = 2;
    constexpr int pixels = logical_size * scale;
    @autoreleasepool {
        const QByteArray utf8 = symbol_name.toUtf8();
        NSString *name = [NSString stringWithUTF8String:utf8.constData()];
        NSImage *symbol = [NSImage imageWithSystemSymbolName:name accessibilityDescription:nil];
        if (symbol == nil)
            symbol = [NSImage imageWithSystemSymbolName:@"folder" accessibilityDescription:nil];
        if (symbol == nil)
            return {};
        NSImageSymbolConfiguration *configuration = [NSImageSymbolConfiguration configurationWithPointSize:logical_size weight:NSFontWeightRegular];
        NSImage *configured = [symbol imageWithSymbolConfiguration:configuration];
        if (configured != nil)
            symbol = configured;
        NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:nullptr pixelsWide:pixels pixelsHigh:pixels bitsPerSample:8 samplesPerPixel:4 hasAlpha:YES isPlanar:NO colorSpaceName:NSDeviceRGBColorSpace bytesPerRow:0 bitsPerPixel:0];
        if (bitmap == nil)
            return {};
        NSGraphicsContext *context = [NSGraphicsContext graphicsContextWithBitmapImageRep:bitmap];
        [NSGraphicsContext saveGraphicsState];
        [NSGraphicsContext setCurrentContext:context];
        [[NSColor clearColor] set];
        NSRectFillUsingOperation(NSMakeRect(0, 0, pixels, pixels), NSCompositingOperationCopy);
        [symbol drawInRect:NSMakeRect(0, 0, pixels, pixels) fromRect:NSZeroRect operation:NSCompositingOperationSourceOver fraction:1.0];
        [context flushGraphics];
        [NSGraphicsContext restoreGraphicsState];
        NSData *png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
        QImage image = QImage::fromData(static_cast<const uchar *>(png.bytes), static_cast<int>(png.length), "PNG");
        image.setDevicePixelRatio(scale);
        images.insert(symbol_name, image);
        return image;
    }
}

} // namespace dual_pane_desktop::gui
