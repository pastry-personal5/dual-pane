#import <AppKit/AppKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

#include "dual_pane_desktop/native_shell.hpp"

#include <dispatch/dispatch.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <limits>
#include <optional>
#include <string>
#include <sys/stat.h>
#include <unordered_map>
#include <utility>
#include <vector>

// The bounded byte codec uses explicit wire widths and pointer traversal, and
// AppKit's Objective-C pointer/null idioms do not benefit from C++ rewrites.
// NOLINTBEGIN(cppcoreguidelines-pro-bounds-pointer-arithmetic,cppcoreguidelines-avoid-magic-numbers,cppcoreguidelines-pro-type-reinterpret-cast,readability-magic-numbers,readability-braces-around-statements,readability-implicit-bool-conversion,modernize-use-trailing-return-type)
namespace dual_pane_desktop {
namespace {

class Decoder {
  public:
    explicit Decoder(::rust::Slice<const std::uint8_t> bytes) : current_(bytes.data()), remaining_(bytes.size()) {}

    auto byte(std::uint8_t &value) -> bool {
        if (remaining_ < 1)
            return false;
        value = *current_++;
        --remaining_;
        return true;
    }

    auto bytes(std::vector<std::uint8_t> &value) -> bool {
        std::uint32_t length = 0;
        if (!number(length) || remaining_ < length)
            return false;
        value.assign(current_, current_ + length);
        current_ += length;
        remaining_ -= length;
        return true;
    }

    auto text() -> NSString * {
        std::vector<std::uint8_t> bytes;
        if (!this->bytes(bytes))
            return nil;
        return [[NSString alloc] initWithBytes:bytes.data() length:bytes.size() encoding:NSUTF8StringEncoding];
    }

    auto count(std::uint32_t &value) -> bool { return number(value) && value <= 4096; }
    auto wide(std::uint64_t &value) -> bool {
        if (remaining_ < 8)
            return false;
        value = 0;
        for (int index = 0; index < 8; ++index)
            value = (value << 8U) | static_cast<std::uint64_t>(current_[index]);
        current_ += 8;
        remaining_ -= 8;
        return true;
    }
    [[nodiscard]] auto empty() const -> bool { return remaining_ == 0; }

  private:
    auto number(std::uint32_t &value) -> bool {
        if (remaining_ < 4)
            return false;
        value = (static_cast<std::uint32_t>(current_[0]) << 24U) | (static_cast<std::uint32_t>(current_[1]) << 16U) | (static_cast<std::uint32_t>(current_[2]) << 8U) | static_cast<std::uint32_t>(current_[3]);
        current_ += 4;
        remaining_ -= 4;
        return true;
    }

    const std::uint8_t *current_;
    std::size_t remaining_;
};

void append_number(::rust::Vec<std::uint8_t> &output, std::size_t value) {
    const auto bounded = static_cast<std::uint32_t>(std::min<std::size_t>(value, std::numeric_limits<std::uint32_t>::max()));
    output.push_back(static_cast<std::uint8_t>(bounded >> 24U));
    output.push_back(static_cast<std::uint8_t>(bounded >> 16U));
    output.push_back(static_cast<std::uint8_t>(bounded >> 8U));
    output.push_back(static_cast<std::uint8_t>(bounded));
}

void append_wide(::rust::Vec<std::uint8_t> &output, std::uint64_t value) {
    for (int shift = 56; shift >= 0; shift -= 8)
        output.push_back(static_cast<std::uint8_t>(value >> static_cast<unsigned>(shift)));
}

void append_bytes(::rust::Vec<std::uint8_t> &output, const std::uint8_t *bytes, std::size_t length) {
    append_number(output, length);
    for (std::size_t index = 0; index < length; ++index)
        output.push_back(bytes[index]);
}

void append_string(::rust::Vec<std::uint8_t> &output, NSString *value) {
    NSData *data = [(value ?: @"") dataUsingEncoding:NSUTF8StringEncoding];
    append_bytes(output, static_cast<const std::uint8_t *>(data.bytes), data.length);
}

void set_error(::rust::Vec<std::uint8_t> &response, NSString *message, bool status_byte) {
    response.clear();
    NSString *text = message.length > 0 ? message : @"macOS could not complete the file association request.";
    if (status_byte) {
        response.push_back(0);
        append_string(response, text);
        return;
    }
    NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
    const auto *bytes = static_cast<const std::uint8_t *>(data.bytes);
    for (std::size_t index = 0; index < data.length; ++index)
        response.push_back(bytes[index]);
}

auto file_url(const std::vector<std::uint8_t> &path) -> NSURL * {
    if (path.empty())
        return nil;
    const std::string representation(reinterpret_cast<const char *>(path.data()), path.size());
    return [NSURL fileURLWithFileSystemRepresentation:representation.c_str() isDirectory:NO relativeToURL:nil];
}

auto url_bytes(NSURL *url) -> std::pair<const std::uint8_t *, std::size_t> {
    const char *path = url.fileSystemRepresentation;
    if (path == nullptr)
        return {nullptr, 0};
    return {reinterpret_cast<const std::uint8_t *>(path), std::strlen(path)};
}

struct Application {
    NSString *bundle_identifier;
    NSString *bundle_url;
    NSString *display_name;
    NSString *location_hint;
};

struct Record {
    UTType *type;
    std::vector<Application> candidates;
    std::optional<Application> default_application;
};

auto application(NSURL *url) -> std::optional<Application> {
    if (url == nil)
        return std::nullopt;
    NSBundle *bundle = [NSBundle bundleWithURL:url];
    NSString *identifier = bundle.bundleIdentifier;
    NSString *absolute = url.absoluteString;
    if (identifier.length == 0 || absolute.length == 0)
        return std::nullopt;
    NSString *name = [[NSFileManager defaultManager] displayNameAtPath:url.path];
    NSString *hint = url.URLByDeletingLastPathComponent.path ?: @"";
    return Application{identifier, absolute, name.length > 0 ? name : identifier, hint};
}

auto applications(NSArray<NSURL *> *urls) -> std::vector<Application> {
    std::vector<Application> result;
    for (NSURL *url in urls) {
        auto candidate = application(url);
        if (!candidate.has_value())
            continue;
        const bool duplicate = std::any_of(result.begin(), result.end(), [&candidate](const Application &existing) { return [existing.bundle_identifier isEqualToString:candidate->bundle_identifier] && [existing.bundle_url isEqualToString:candidate->bundle_url]; });
        if (!duplicate)
            result.push_back(*candidate);
    }
    std::unordered_map<std::string, std::size_t> names;
    for (const auto &candidate : result)
        ++names[candidate.display_name.UTF8String ?: ""];
    for (auto &candidate : result) {
        if (names[candidate.display_name.UTF8String ?: ""] > 1)
            candidate.display_name = [NSString stringWithFormat:@"%@ — %@", candidate.display_name, candidate.location_hint];
    }
    // Two bundles can report the same display name from the same parent
    // directory. Their full paths are the final disambiguator; the exact URL
    // remains the dispatch identity on the Rust side.
    std::unordered_map<std::string, std::size_t> qualified_names;
    for (const auto &candidate : result)
        ++qualified_names[candidate.display_name.UTF8String ?: ""];
    for (auto &candidate : result) {
        if (qualified_names[candidate.display_name.UTF8String ?: ""] > 1) {
            NSURL *url = [NSURL URLWithString:candidate.bundle_url];
            candidate.display_name = [NSString stringWithFormat:@"%@ — %@", candidate.display_name, url.path ?: candidate.bundle_url];
        }
    }
    return result;
}

auto record_for_type(UTType *type, NSArray<NSURL *> *candidate_urls) -> Record {
    NSWorkspace *workspace = [NSWorkspace sharedWorkspace];
    auto default_application = application([workspace URLForApplicationToOpenContentType:type]);
    return Record{type, applications(candidate_urls), default_application};
}

void append_application(::rust::Vec<std::uint8_t> &output, const Application &application) {
    append_string(output, application.bundle_identifier);
    append_string(output, application.bundle_url);
}

void append_record(::rust::Vec<std::uint8_t> &output, const Record &record) {
    append_string(output, record.type.identifier);
    append_string(output, record.type.localizedDescription ?: record.type.identifier);
    append_string(output, record.type.preferredFilenameExtension ?: @"");
    append_number(output, record.candidates.size());
    for (const auto &candidate : record.candidates) {
        append_application(output, candidate);
        append_string(output, candidate.display_name);
        append_string(output, candidate.location_hint);
    }
    output.push_back(record.default_application.has_value() ? 1 : 0);
    if (record.default_application.has_value())
        append_application(output, *record.default_application);
}

struct ResolvedFile {
    std::vector<std::uint8_t> source;
    NSURL *target;
    std::uint64_t source_device;
    std::uint64_t source_inode;
    std::uint64_t target_device;
    std::uint64_t target_inode;
    Record record;
};

struct Resolution {
    std::optional<ResolvedFile> file;
    NSString *error;
};

auto resolve_file(const std::vector<std::uint8_t> &source) -> Resolution {
    NSURL *source_url = file_url(source);
    if (source_url == nil) {
        return {std::nullopt, @"The selected file path is invalid."};
    }
    struct stat source_stat{};
    const std::string source_path(reinterpret_cast<const char *>(source.data()), source.size());
    if (::lstat(source_path.c_str(), &source_stat) != 0)
        return {std::nullopt, @"The selected file changed or is no longer available."};
    NSNumber *symbolic = nil;
    NSError *resource_error = nil;
    if (![source_url getResourceValue:&symbolic forKey:NSURLIsSymbolicLinkKey error:&resource_error]) {
        return {std::nullopt, [NSString stringWithFormat:@"The selected file's link status could not be read: %@", resource_error.localizedDescription]};
    }
    NSURL *target = symbolic.boolValue ? source_url.URLByResolvingSymlinksInPath : source_url;
    NSNumber *regular = nil;
    NSNumber *package = nil;
    UTType *type = nil;
    if (![target getResourceValue:&regular forKey:NSURLIsRegularFileKey error:&resource_error])
        return {std::nullopt, [NSString stringWithFormat:@"The selected target's file kind could not be read: %@", resource_error.localizedDescription]};
    if (![target getResourceValue:&package forKey:NSURLIsPackageKey error:&resource_error])
        return {std::nullopt, [NSString stringWithFormat:@"The selected target's package status could not be read: %@", resource_error.localizedDescription]};
    NSError *content_type_error = nil;
    const bool read_content_type = [target getResourceValue:&type forKey:NSURLContentTypeKey error:&content_type_error];
    // Local temporary and some network-backed files can be readable regular
    // files while the resource service declines NSURLContentTypeKey. Keep the
    // resource value authoritative when available; only then ask macOS's UTI
    // registry for the filename extension, never a Dual Pane mapping.
    if ((!read_content_type || type == nil) && target.pathExtension.length > 0)
        type = [UTType typeWithFilenameExtension:target.pathExtension];
    if (type == nil)
        return {std::nullopt, [NSString stringWithFormat:@"The selected target's content type could not be read: %@", content_type_error.localizedDescription ?: @"macOS did not return a content type."]};
    if (!regular.boolValue || package.boolValue || type == nil) {
        return {std::nullopt, @"Choose a regular file or a file link, not a folder or package."};
    }
    struct stat target_stat{};
    if (::stat(target.fileSystemRepresentation, &target_stat) != 0)
        return {std::nullopt, @"The selected file target changed or is no longer available."};
    NSWorkspace *workspace = [NSWorkspace sharedWorkspace];
    NSArray<NSURL *> *file_candidates = [workspace URLsForApplicationsToOpenURL:target];
    // A file URL can have no per-item registration even though macOS has
    // candidates for its resolved UTType (notably in disposable profiles).
    // The type list remains macOS-owned and supplies the same safe fallback
    // to both discovery and the later open-time revalidation.
    if (file_candidates.count == 0)
        file_candidates = [workspace URLsForApplicationsToOpenContentType:type];
    return {ResolvedFile{source, target, static_cast<std::uint64_t>(source_stat.st_dev), static_cast<std::uint64_t>(source_stat.st_ino), static_cast<std::uint64_t>(target_stat.st_dev), static_cast<std::uint64_t>(target_stat.st_ino), record_for_type(type, file_candidates)}, nil};
}

auto same_application(NSURL *url, NSString *bundle_identifier, NSString *bundle_url) -> bool {
    auto candidate = application(url);
    return candidate.has_value() && [candidate->bundle_identifier isEqualToString:bundle_identifier] && [candidate->bundle_url isEqualToString:bundle_url];
}

struct DecodedIdentity {
    NSString *bundle_identifier;
    NSString *bundle_url;
};

auto decode_identity(Decoder &decoder) -> std::optional<DecodedIdentity> {
    NSString *bundle_identifier = nil;
    NSString *bundle_url = nil;
    bundle_identifier = decoder.text();
    bundle_url = decoder.text();
    if (bundle_identifier.length == 0 || bundle_url.length == 0)
        return std::nullopt;
    return DecodedIdentity{bundle_identifier, bundle_url};
}

void file_associations_impl(::rust::Slice<const std::uint8_t> request, ::rust::Vec<std::uint8_t> &response) {
    Decoder decoder(request);
    std::uint32_t file_count = 0;
    if (!decoder.count(file_count)) {
        set_error(response, @"The association request is malformed.", true);
        return;
    }
    std::vector<std::vector<std::uint8_t>> file_paths;
    for (std::uint32_t index = 0; index < file_count; ++index) {
        std::vector<std::uint8_t> path;
        if (!decoder.bytes(path)) {
            set_error(response, @"The association request is malformed.", true);
            return;
        }
        file_paths.push_back(std::move(path));
    }
    std::uint32_t type_count = 0;
    if (!decoder.count(type_count)) {
        set_error(response, @"The association request is malformed.", true);
        return;
    }
    std::vector<UTType *> types;
    for (std::uint32_t index = 0; index < type_count; ++index) {
        NSString *identifier = decoder.text();
        if (identifier == nil) {
            set_error(response, @"The association request is malformed.", true);
            return;
        }
        UTType *type = [UTType typeWithIdentifier:identifier];
        if (type != nil)
            types.push_back(type);
    }
    std::uint8_t purpose = 0;
    if (!decoder.byte(purpose) || !decoder.empty()) {
        set_error(response, @"The association request is malformed.", true);
        return;
    }

    std::vector<ResolvedFile> files;
    NSString *first_error = nil;
    for (const auto &path : file_paths) {
        auto resolution = resolve_file(path);
        if (resolution.file.has_value()) {
            files.push_back(std::move(*resolution.file));
            types.push_back(files.back().record.type);
        } else if (first_error == nil) {
            first_error = resolution.error;
        }
    }
    if ((purpose == 1 || purpose == 2) && files.size() != file_paths.size()) {
        set_error(response, first_error, true);
        return;
    }

    std::vector<Record> records;
    for (UTType *type : types) {
        const bool known = std::any_of(records.begin(), records.end(), [type](const Record &record) { return [record.type.identifier isEqualToString:type.identifier]; });
        if (!known)
            records.push_back(record_for_type(type, [[NSWorkspace sharedWorkspace] URLsForApplicationsToOpenContentType:type]));
    }
    response.clear();
    response.push_back(1);
    append_number(response, records.size());
    for (const auto &record : records)
        append_record(response, record);
    append_number(response, files.size());
    for (const auto &file : files) {
        append_bytes(response, file.source.data(), file.source.size());
        const auto [target_bytes, target_length] = url_bytes(file.target);
        append_bytes(response, target_bytes, target_length);
        append_wide(response, file.source_device);
        append_wide(response, file.source_inode);
        append_wide(response, file.target_device);
        append_wide(response, file.target_inode);
        append_record(response, file.record);
    }
}

void open_with_application_impl(::rust::Slice<const std::uint8_t> request, ::rust::Vec<std::uint8_t> &response) {
    Decoder decoder(request);
    NSString *content_type = nil;
    std::uint32_t file_count = 0;
    content_type = decoder.text();
    if (content_type == nil) {
        set_error(response, @"The Open With request is malformed.", false);
        return;
    }
    auto identity = decode_identity(decoder);
    if (!identity.has_value() || !decoder.count(file_count) || file_count == 0) {
        set_error(response, @"The Open With request is malformed.", false);
        return;
    }
    NSMutableArray<NSURL *> *files = [NSMutableArray arrayWithCapacity:file_count];
    for (std::uint32_t index = 0; index < file_count; ++index) {
        std::vector<std::uint8_t> path;
        std::vector<std::uint8_t> target_path;
        std::uint64_t source_device = 0;
        std::uint64_t source_inode = 0;
        std::uint64_t target_device = 0;
        std::uint64_t target_inode = 0;
        if (!decoder.bytes(path) || !decoder.bytes(target_path) || !decoder.wide(source_device) || !decoder.wide(source_inode) || !decoder.wide(target_device) || !decoder.wide(target_inode)) {
            set_error(response, @"The Open With request is malformed.", false);
            return;
        }
        auto resolution = resolve_file(path);
        if (!resolution.file.has_value() || ![resolution.file->record.type.identifier isEqualToString:content_type]) {
            set_error(response, resolution.error ?: @"A selected file changed type or is no longer available.", false);
            return;
        }
        if (resolution.file->source_device != source_device || resolution.file->source_inode != source_inode || resolution.file->target_device != target_device || resolution.file->target_inode != target_inode) {
            set_error(response, @"A selected file or link target changed after the menu opened.", false);
            return;
        }
        const auto [resolved_target, resolved_target_length] = url_bytes(resolution.file->target);
        if (resolved_target_length != target_path.size() || !std::equal(target_path.begin(), target_path.end(), resolved_target)) {
            set_error(response, @"A selected file link points to a different target than when the menu opened.", false);
            return;
        }
        const bool eligible = std::any_of(resolution.file->record.candidates.begin(), resolution.file->record.candidates.end(), [&](const Application &candidate) { return [candidate.bundle_identifier isEqualToString:identity->bundle_identifier] && [candidate.bundle_url isEqualToString:identity->bundle_url]; });
        if (!eligible) {
            set_error(response, @"The chosen application is no longer available for every selected file.", false);
            return;
        }
        [files addObject:resolution.file->target];
    }
    if (!decoder.empty()) {
        set_error(response, @"The Open With request is malformed.", false);
        return;
    }
    NSURL *application_url = [NSURL URLWithString:identity->bundle_url];
    if (!same_application(application_url, identity->bundle_identifier, identity->bundle_url)) {
        set_error(response, @"The chosen application moved, was replaced, or is unavailable.", false);
        return;
    }
    __block NSError *completion_error = nil;
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_main_queue(), ^{
      [[NSWorkspace sharedWorkspace] openURLs:files
                         withApplicationAtURL:application_url
                                configuration:[NSWorkspaceOpenConfiguration configuration]
                            completionHandler:^(NSRunningApplication *, NSError *error) {
                              completion_error = error;
                              dispatch_semaphore_signal(semaphore);
                            }];
    });
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, 30LL * NSEC_PER_SEC)) != 0)
        completion_error = [NSError errorWithDomain:@"DualPaneFileAssociations" code:1 userInfo:@{NSLocalizedDescriptionKey : @"macOS did not finish opening the files in time."}];
    response.clear();
    if (completion_error != nil)
        set_error(response, completion_error.localizedDescription, false);
}

void set_default_application_impl(::rust::Slice<const std::uint8_t> request, ::rust::Vec<std::uint8_t> &response) {
    Decoder decoder(request);
    NSString *content_type = nil;
    content_type = decoder.text();
    if (content_type == nil) {
        set_error(response, @"The default-application request is malformed.", false);
        return;
    }
    auto identity = decode_identity(decoder);
    if (!identity.has_value() || !decoder.empty()) {
        set_error(response, @"The default-application request is malformed.", false);
        return;
    }
    UTType *type = [UTType typeWithIdentifier:content_type];
    NSURL *application_url = [NSURL URLWithString:identity->bundle_url];
    NSArray<NSURL *> *candidates = type == nil ? @[] : [[NSWorkspace sharedWorkspace] URLsForApplicationsToOpenContentType:type];
    if (type == nil || !same_application(application_url, identity->bundle_identifier, identity->bundle_url) || ![candidates containsObject:application_url]) {
        set_error(response, @"The chosen application is no longer available for this file type.", false);
        return;
    }
    __block NSError *completion_error = nil;
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_main_queue(), ^{
      [[NSWorkspace sharedWorkspace] setDefaultApplicationAtURL:application_url
                                              toOpenContentType:type
                                              completionHandler:^(NSError *error) {
                                                completion_error = error;
                                                dispatch_semaphore_signal(semaphore);
                                              }];
    });
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, 30LL * NSEC_PER_SEC)) != 0)
        completion_error = [NSError errorWithDomain:@"DualPaneFileAssociations" code:1 userInfo:@{NSLocalizedDescriptionKey : @"macOS did not finish changing the default in time."}];
    NSURL *read_back = completion_error == nil ? [[NSWorkspace sharedWorkspace] URLForApplicationToOpenContentType:type] : nil;
    if (completion_error == nil && !same_application(read_back, identity->bundle_identifier, identity->bundle_url))
        completion_error = [NSError errorWithDomain:@"DualPaneFileAssociations" code:2 userInfo:@{NSLocalizedDescriptionKey : @"macOS did not report the requested application as the new default."}];
    response.clear();
    if (completion_error != nil)
        set_error(response, completion_error.localizedDescription, false);
}

} // namespace

void file_associations(::rust::Slice<const std::uint8_t> request, ::rust::Vec<std::uint8_t> &response) {
    @autoreleasepool {
        file_associations_impl(request, response);
    }
}

void open_with_application(::rust::Slice<const std::uint8_t> request, ::rust::Vec<std::uint8_t> &response) {
    @autoreleasepool {
        open_with_application_impl(request, response);
    }
}

void set_default_application(::rust::Slice<const std::uint8_t> request, ::rust::Vec<std::uint8_t> &response) {
    @autoreleasepool {
        set_default_application_impl(request, response);
    }
}

} // namespace dual_pane_desktop
// NOLINTEND(cppcoreguidelines-pro-bounds-pointer-arithmetic,cppcoreguidelines-avoid-magic-numbers,cppcoreguidelines-pro-type-reinterpret-cast,readability-magic-numbers,readability-braces-around-statements,readability-implicit-bool-conversion,modernize-use-trailing-return-type)
