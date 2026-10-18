#include "dual_pane_desktop/clipboard_adapter.hpp"

#import <AppKit/AppKit.h>

#include <QtCore/QList>

#include <cstdint>
#include <limits>
#include <optional>

namespace dual_pane_desktop {
namespace {
NSString *const private_type_name = @"org.dual-pane.private-transfer";
constexpr std::uint8_t wire_version = 1;
/// Read-result flag: the pasteboard changed during every read attempt.
constexpr std::uint8_t busy_flag = 8;
constexpr int seam_replace_with_string = 0, seam_replace_with_file_url_lines = 1, seam_replace_with_private_bytes = 2, seam_append_string_item = 3, seam_replace_with_file_reference_url = 4;

void append_u32(QByteArray &bytes, std::uint32_t value) {
    for (int shift = 0; shift < 32; shift += 8)
        bytes.append(static_cast<char>((value >> shift) & 0xffU));
}

void append_u64(QByteArray &bytes, std::uint64_t value) {
    for (int shift = 0; shift < 64; shift += 8)
        bytes.append(static_cast<char>((value >> shift) & 0xffU));
}

auto read_u32(const QByteArray &bytes, qsizetype &offset) -> std::optional<std::uint32_t> {
    if (offset < 0 || bytes.size() - offset < 4)
        return std::nullopt;
    std::uint32_t value = 0;
    for (int shift = 0; shift < 32; shift += 8)
        value |= static_cast<std::uint32_t>(static_cast<unsigned char>(bytes.at(offset++))) << shift;
    return value;
}

auto read_u64(const QByteArray &bytes, qsizetype &offset) -> std::optional<std::uint64_t> {
    if (offset < 0 || bytes.size() - offset < 8)
        return std::nullopt;
    std::uint64_t value = 0;
    for (int shift = 0; shift < 64; shift += 8)
        value |= static_cast<std::uint64_t>(static_cast<unsigned char>(bytes.at(offset++))) << shift;
    return value;
}

/// Tests install a per-thread unique pasteboard so they never touch the
/// user's clipboard. Production threads always use the general pasteboard.
thread_local const void *isolated_pasteboard = nullptr;

auto active_pasteboard() -> NSPasteboard * {
    return isolated_pasteboard != nullptr ? (__bridge NSPasteboard *)isolated_pasteboard : NSPasteboard.generalPasteboard;
}

auto private_type() -> NSPasteboardType {
    return static_cast<NSPasteboardType>(private_type_name);
}

auto is_unreserved(unsigned char byte) -> bool {
    return (byte >= 'A' && byte <= 'Z') || (byte >= 'a' && byte <= 'z') || (byte >= '0' && byte <= '9') || byte == '-' || byte == '.' || byte == '_' || byte == '~';
}

auto hex_value(unsigned char byte) -> int {
    if (byte >= '0' && byte <= '9')
        return byte - '0';
    if (byte >= 'A' && byte <= 'F')
        return byte - 'A' + 10;
    if (byte >= 'a' && byte <= 'f')
        return byte - 'a' + 10;
    return -1;
}

/// Writes `path` as a `file://` URL, percent-encoding every byte that is not
/// unreserved. NSURL's filesystem-representation calls cannot carry bytes that
/// are not valid UTF-8 and re-decompose precomposed names, so neither would
/// keep an Item's exact name.
auto encode_file_url(const QByteArray &path, bool directory) -> NSString * {
    static const char digits[] = "0123456789ABCDEF";
    QByteArray encoded("file://");
    for (const char character : path) {
        const auto byte = static_cast<unsigned char>(character);
        if (byte == '/' || is_unreserved(byte)) {
            encoded.append(character);
        } else {
            encoded.append('%');
            encoded.append(digits[byte >> 4]);
            encoded.append(digits[byte & 0x0fU]);
        }
    }
    if (directory && !path.endsWith('/'))
        encoded.append('/');
    return [[NSString alloc] initWithBytes:encoded.constData() length:static_cast<NSUInteger>(encoded.size()) encoding:NSASCIIStringEncoding];
}

auto utf8_bytes(NSString *value) -> std::optional<QByteArray> {
    NSData *data = value == nil ? nil : [value dataUsingEncoding:NSUTF8StringEncoding];
    if (data == nil)
        return std::nullopt;
    return QByteArray(static_cast<const char *>(data.bytes), static_cast<qsizetype>(data.length));
}

/// The exact path bytes of a local `file://` URL, or nothing when the text is
/// not one: another scheme or host, relative form, query or fragment, control
/// character, bad percent escape, or NUL.
auto decode_file_url_text(const QByteArray &text) -> std::optional<QByteArray> {
    if (text.size() < 8 || text.left(7).toLower() != QByteArrayLiteral("file://"))
        return std::nullopt;
    const auto path_start = text.indexOf('/', 7);
    if (path_start < 0)
        return std::nullopt;
    const auto host = text.mid(7, path_start - 7).toLower();
    if (!host.isEmpty() && host != QByteArrayLiteral("localhost"))
        return std::nullopt;
    QByteArray path;
    for (qsizetype index = path_start; index < text.size(); ++index) {
        const auto byte = static_cast<unsigned char>(text.at(index));
        if (byte <= 0x20 || byte == 0x7f || byte == '?' || byte == '#')
            return std::nullopt;
        if (byte != '%') {
            path.append(static_cast<char>(byte));
            continue;
        }
        if (text.size() - index < 3)
            return std::nullopt;
        const int high = hex_value(static_cast<unsigned char>(text.at(index + 1)));
        const int low = hex_value(static_cast<unsigned char>(text.at(index + 2)));
        if (high < 0 || low < 0 || (high == 0 && low == 0))
            return std::nullopt;
        path.append(static_cast<char>((high << 4) | low));
        index += 2;
    }
    return path;
}

/// Finder can offer a file reference URL (`file:///.file/id=...`), which names
/// no path until the system resolves it. A reference that no longer resolves
/// is refused rather than read as a literal path.
auto decode_file_url(NSString *value) -> std::optional<QByteArray> {
    const auto text = utf8_bytes(value);
    if (!text)
        return std::nullopt;
    const auto direct = decode_file_url_text(*text);
    if (!direct || !direct->startsWith(QByteArrayLiteral("/.file/id=")))
        return direct;
    NSURL *resolved = [NSURL URLWithString:value].filePathURL;
    if (resolved == nil || resolved.isFileReferenceURL)
        return std::nullopt;
    const auto resolved_text = utf8_bytes(resolved.absoluteString);
    return resolved_text ? decode_file_url_text(*resolved_text) : std::nullopt;
}

auto token_data(std::uint64_t transfer) -> NSData * {
    std::uint8_t bytes[9] = {wire_version};
    for (int index = 0; index < 8; ++index)
        bytes[index + 1] = static_cast<std::uint8_t>((transfer >> (index * 8)) & 0xffU);
    return [NSData dataWithBytes:bytes length:sizeof(bytes)];
}

auto decode_token(NSData *data) -> std::optional<std::uint64_t> {
    if (data == nil || data.length != 9)
        return std::nullopt;
    const auto *bytes = static_cast<const std::uint8_t *>(data.bytes);
    if (bytes[0] != wire_version)
        return std::nullopt;
    std::uint64_t transfer = 0;
    for (int index = 0; index < 8; ++index)
        transfer |= static_cast<std::uint64_t>(bytes[index + 1]) << (index * 8);
    return transfer;
}
} // namespace

auto clipboard_generation() -> qint64 {
    @autoreleasepool {
        return static_cast<qint64>(active_pasteboard().changeCount);
    }
}

auto clipboard_offer() -> int {
    @autoreleasepool {
        NSPasteboard *pasteboard = active_pasteboard();
        const bool private_data = [pasteboard availableTypeFromArray:@[ private_type() ]] != nil;
        const bool files = [pasteboard availableTypeFromArray:@[ NSPasteboardTypeFileURL ]] != nil;
        return (private_data ? 1 : 0) | (files ? 2 : 0);
    }
}

auto clipboard_publish(const QByteArray &request) -> QByteArray {
    @autoreleasepool {
        QByteArray result;
        NSPasteboard *pasteboard = active_pasteboard();
        const auto finish = [&](bool succeeded) {
            append_u64(result, static_cast<std::uint64_t>(pasteboard.changeCount));
            result.append(succeeded ? '\1' : '\0');
            return result;
        };
        qsizetype offset = 0;
        if (request.size() < 14 || request.left(5) != QByteArrayLiteral("DPCB1"))
            return finish(false);
        offset = 5;
        const auto transfer = read_u64(request, offset);
        if (!transfer || offset >= request.size())
            return finish(false);
        const bool copy = request.at(offset++) == 0;
        const auto count = read_u32(request, offset);
        if (!count || (!copy && *count != 0))
            return finish(false);
        NSMutableArray<NSPasteboardItem *> *items = [NSMutableArray arrayWithCapacity:copy ? *count : 1];
        for (std::uint32_t index = 0; index < *count; ++index) {
            if (offset >= request.size())
                return finish(false);
            const bool directory = request.at(offset++) != 0;
            const auto length = read_u32(request, offset);
            if (!length || *length == 0 || *length > static_cast<std::uint32_t>(std::numeric_limits<int>::max()) || request.size() - offset < *length)
                return finish(false);
            const QByteArray path = request.mid(offset, static_cast<qsizetype>(*length));
            offset += static_cast<qsizetype>(*length);
            if (path.at(0) != '/' || path.contains('\0'))
                return finish(false);
            NSString *url = encode_file_url(path, directory);
            if (url == nil)
                return finish(false);
            NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
            if (![item setString:url forType:NSPasteboardTypeFileURL])
                return finish(false);
            if (index == 0 && ![item setData:token_data(*transfer) forType:private_type()])
                return finish(false);
            [items addObject:item];
        }
        if (offset != request.size() || (copy && items.count == 0))
            return finish(false);
        if (!copy) {
            NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
            if (![item setData:token_data(*transfer) forType:private_type()])
                return finish(false);
            [items addObject:item];
        }
        // Clearing takes ownership and returns this write's generation. A
        // later read of changeCount could report another application's write.
        const NSInteger written = [pasteboard clearContents];
        if (![pasteboard writeObjects:items])
            return finish(false);
        append_u64(result, static_cast<std::uint64_t>(written));
        result.append('\1');
        return result;
    }
}

auto clipboard_read() -> QByteArray {
    @autoreleasepool {
        NSPasteboard *pasteboard = active_pasteboard();
        // A snapshot belongs to a generation only if no write landed while
        // its items were read; a pasteboard that keeps changing reads as busy.
        constexpr int attempts = 3;
        for (int attempt = 0; attempt < attempts; ++attempt) {
            const NSInteger generation = pasteboard.changeCount;
            std::optional<std::uint64_t> transfer;
            bool malformed = false;
            QList<QByteArray> paths;
            for (NSPasteboardItem *item in pasteboard.pasteboardItems) {
                bool supported_item = false;
                if ([item.types containsObject:private_type()] && !transfer) {
                    transfer = decode_token([item dataForType:private_type()]);
                    if (!transfer)
                        malformed = true;
                    supported_item = true;
                }
                if (![item.types containsObject:NSPasteboardTypeFileURL]) {
                    if (!supported_item)
                        malformed = true;
                    continue;
                }
                const auto path = decode_file_url([item stringForType:NSPasteboardTypeFileURL]);
                if (!path) {
                    malformed = true;
                    continue;
                }
                paths.append(*path);
            }
            if (pasteboard.changeCount != generation)
                continue;
            QByteArray result;
            append_u64(result, static_cast<std::uint64_t>(generation));
            const std::uint8_t flags = (transfer ? 1U : 0U) | (!paths.isEmpty() ? 2U : 0U) | (malformed ? 4U : 0U);
            result.append(static_cast<char>(flags));
            if (transfer)
                append_u64(result, *transfer);
            append_u32(result, static_cast<std::uint32_t>(paths.size()));
            for (const QByteArray &path : paths) {
                append_u32(result, static_cast<std::uint32_t>(path.size()));
                result.append(path);
            }
            return result;
        }
        QByteArray busy;
        append_u64(busy, static_cast<std::uint64_t>(pasteboard.changeCount));
        busy.append(static_cast<char>(busy_flag));
        append_u32(busy, 0);
        return busy;
    }
}

void clipboard_isolation_begin() {
    @autoreleasepool {
        if (isolated_pasteboard == nullptr)
            isolated_pasteboard = CFBridgingRetain([NSPasteboard pasteboardWithUniqueName]);
    }
}

void clipboard_isolation_end() {
    @autoreleasepool {
        if (isolated_pasteboard == nullptr)
            return;
        NSPasteboard *pasteboard = (__bridge NSPasteboard *)isolated_pasteboard;
        [pasteboard releaseGlobally];
        CFBridgingRelease(isolated_pasteboard);
        isolated_pasteboard = nullptr;
    }
}

auto clipboard_seam_write(int kind, const QByteArray &payload) -> bool {
    @autoreleasepool {
        if (isolated_pasteboard == nullptr)
            return false;
        NSPasteboard *pasteboard = active_pasteboard();
        if (kind == seam_replace_with_private_bytes) {
            NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
            [pasteboard clearContents];
            return [item setData:[NSData dataWithBytes:payload.constData() length:static_cast<NSUInteger>(payload.size())] forType:private_type()] && [pasteboard writeObjects:@[ item ]];
        }
        NSString *text = [[NSString alloc] initWithBytes:payload.constData() length:static_cast<NSUInteger>(payload.size()) encoding:NSUTF8StringEncoding];
        if (text == nil)
            return false;
        if (kind == seam_replace_with_string || kind == seam_append_string_item) {
            NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
            if (kind == seam_replace_with_string)
                [pasteboard clearContents];
            return [item setString:text forType:NSPasteboardTypeString] && [pasteboard writeObjects:@[ item ]];
        }
        if (kind == seam_replace_with_file_reference_url) {
            NSURL *reference = [[NSURL fileURLWithPath:text] fileReferenceURL];
            NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
            if (reference == nil || ![item setString:reference.absoluteString forType:NSPasteboardTypeFileURL])
                return false;
            [pasteboard clearContents];
            return [pasteboard writeObjects:@[ item ]];
        }
        if (kind == seam_replace_with_file_url_lines) {
            NSMutableArray<NSPasteboardItem *> *items = [NSMutableArray array];
            for (NSString *line in [text componentsSeparatedByString:@"\n"]) {
                NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
                if (![item setString:line forType:NSPasteboardTypeFileURL])
                    return false;
                [items addObject:item];
            }
            [pasteboard clearContents];
            return [pasteboard writeObjects:items];
        }
        return false;
    }
}

auto clipboard_seam_describe() -> QByteArray {
    @autoreleasepool {
        QByteArray result;
        if (isolated_pasteboard == nullptr)
            return result;
        for (NSPasteboardItem *item in active_pasteboard().pasteboardItems) {
            if (!result.isEmpty())
                result.append('\n');
            const bool has_private = [item.types containsObject:private_type()];
            const bool has_file = [item.types containsObject:NSPasteboardTypeFileURL];
            result.append(has_private && has_file ? "private+file" : has_private ? "private" : has_file ? "file" : "other");
        }
        return result;
    }
}

auto clipboard_seam_private_payload() -> QByteArray {
    @autoreleasepool {
        QByteArray result;
        if (isolated_pasteboard == nullptr)
            return result;
        for (NSPasteboardItem *item in active_pasteboard().pasteboardItems) {
            NSData *data = [item dataForType:private_type()];
            if (data != nil)
                return QByteArray(static_cast<const char *>(data.bytes), static_cast<qsizetype>(data.length));
        }
        return result;
    }
}

} // namespace dual_pane_desktop
