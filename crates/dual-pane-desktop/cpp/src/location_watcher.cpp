#include "dual_pane_desktop/location_watcher.hpp"
#include "dual-pane-desktop/src/location_watcher.cxxqt.h"

#include <CoreServices/CoreServices.h>
#include <dispatch/dispatch.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <limits>
#include <memory>
#include <unordered_map>
#include <utility>
#include <vector>

namespace dual_pane_desktop {
namespace {
constexpr std::uint32_t root_changed = 1;
constexpr std::uint32_t recovery_required = 1U << 1U;
constexpr CFTimeInterval event_latency_seconds = 0.2;

void queue_barrier(void *unused) { static_cast<void>(unused); }

auto file_system_string(::rust::Slice<const std::uint8_t> path) -> CFStringRef {
    if (std::find(path.begin(), path.end(), std::uint8_t{0}) != path.end()) {
        return nullptr;
    }
    std::vector<char> bytes;
    bytes.reserve(path.size() + 1);
    for (const auto byte : path) {
        bytes.push_back(static_cast<char>(byte));
    }
    bytes.push_back('\0');
    const auto *value = CFStringCreateWithFileSystemRepresentation(kCFAllocatorDefault, bytes.data());
    if (value == nullptr) {
        return nullptr;
    }
    const auto capacity = CFStringGetMaximumSizeOfFileSystemRepresentation(value);
    if (capacity <= 0) {
        CFRelease(value);
        return nullptr;
    }
    std::vector<char> round_trip(static_cast<std::size_t>(capacity));
    if (CFStringGetFileSystemRepresentation(value, round_trip.data(), capacity) == 0U) {
        CFRelease(value);
        return nullptr;
    }
    const auto length = std::strlen(round_trip.data());
    const bool same = length == path.size() && std::equal(path.begin(), path.end(), round_trip.begin(), [](std::uint8_t left, char right) -> bool { return left == static_cast<std::uint8_t>(right); });
    if (!same) {
        CFRelease(value);
        return nullptr;
    }
    return value;
}
} // namespace

class NativeLocationWatcher::Impl final {
  public:
    explicit Impl(::rust::Box<WatchPublisher> publisher) : publisher_(std::move(publisher)), queue_(dispatch_queue_create("dev.dual-pane.location-watcher", DISPATCH_QUEUE_SERIAL)) {}

    ~Impl() {
        while (!streams_.empty()) {
            unwatch(streams_.begin()->first);
        }
        if (queue_ != nullptr) {
            dispatch_sync_f(queue_, nullptr, queue_barrier);
        }
#if !OS_OBJECT_USE_OBJC
        if (queue_ != nullptr) {
            dispatch_release(queue_);
        }
#endif
    }
    Impl(const Impl &) = delete;
    auto operator=(const Impl &) -> Impl & = delete;
    Impl(Impl &&) = delete;
    auto operator=(Impl &&) -> Impl & = delete;

    [[nodiscard]] auto watch(std::uint64_t subscription_id, ::rust::Slice<const std::uint8_t> path) -> bool {
        const auto existing = streams_.find(subscription_id);
        if (queue_ == nullptr || existing != streams_.end()) {
            return existing != streams_.end();
        }
        const auto *const location = file_system_string(path);
        if (location == nullptr) {
            return false;
        }
        const void *value = location;
        const auto *const paths = CFArrayCreate(kCFAllocatorDefault, &value, 1, &kCFTypeArrayCallBacks);
        CFRelease(location);
        if (paths == nullptr) {
            return false;
        }
        auto context = std::make_unique<CallbackContext>();
        context->owner = this;
        context->subscription_id = subscription_id;
        FSEventStreamContext stream_context = {0, context.get(), nullptr, nullptr, nullptr};
        const auto flags = static_cast<FSEventStreamCreateFlags>(kFSEventStreamCreateFlagWatchRoot | kFSEventStreamCreateFlagNoDefer);
        auto *stream = FSEventStreamCreate(kCFAllocatorDefault, callback, &stream_context, paths, kFSEventStreamEventIdSinceNow, event_latency_seconds, flags);
        CFRelease(paths);
        if (stream == nullptr) {
            return false;
        }
        FSEventStreamSetDispatchQueue(stream, queue_);
        if (FSEventStreamStart(stream) == 0U) {
            FSEventStreamInvalidate(stream);
            FSEventStreamRelease(stream);
            return false;
        }
        streams_.emplace(subscription_id, Stream{stream, std::move(context)});
        return true;
    }

    void unwatch(std::uint64_t subscription_id) {
        const auto found = streams_.find(subscription_id);
        if (found == streams_.end()) {
            return;
        }
        FSEventStreamStop(found->second.stream);
        FSEventStreamInvalidate(found->second.stream);
        FSEventStreamRelease(found->second.stream);
        if (queue_ != nullptr) {
            dispatch_sync_f(queue_, nullptr, queue_barrier);
        }
        streams_.erase(found);
    }

  private:
    struct CallbackContext {
        Impl *owner = nullptr;
        std::uint64_t subscription_id = 0;
    };
    struct Stream {
        FSEventStreamRef stream = nullptr;
        std::unique_ptr<CallbackContext> context;
    };

    static void callback(ConstFSEventStreamRef stream, void *information, std::size_t count, void *paths, const FSEventStreamEventFlags flags[], const FSEventStreamEventId event_ids[]) {
        static_cast<void>(stream);
        static_cast<void>(paths);
        static_cast<void>(event_ids);
        const auto *context = static_cast<const CallbackContext *>(information);
        if (context == nullptr || context->owner == nullptr) {
            return;
        }
        std::uint32_t published = 0;
        for (std::size_t index = 0; index < count; ++index) {
            const auto event_flags = static_cast<std::uint32_t>(flags[index]); // NOLINT(cppcoreguidelines-pro-bounds-pointer-arithmetic)
            if ((event_flags & static_cast<std::uint32_t>(kFSEventStreamEventFlagRootChanged)) != 0U) {
                published |= root_changed;
            }
            const auto discontinuity_flags = static_cast<std::uint32_t>(kFSEventStreamEventFlagMustScanSubDirs) | static_cast<std::uint32_t>(kFSEventStreamEventFlagEventIdsWrapped);
            if ((event_flags & discontinuity_flags) != 0U) {
                // A dropped/coalesced history or wrapped event ID leaves this
                // stream unable to prove continuity. Rust will reconcile the
                // listing and establish a fresh stream before restoring the
                // Native status.
                published |= recovery_required;
            }
        }
        context->owner->publisher_->native_invalidated(context->subscription_id, published);
    }

    ::rust::Box<WatchPublisher> publisher_;
    dispatch_queue_t queue_ = nullptr;
    std::unordered_map<std::uint64_t, Stream> streams_;
};

NativeLocationWatcher::NativeLocationWatcher(::rust::Box<WatchPublisher> publisher) : impl_(std::make_unique<Impl>(std::move(publisher))) {}
NativeLocationWatcher::~NativeLocationWatcher() = default;

auto NativeLocationWatcher::watch(std::uint64_t subscription_id, ::rust::Slice<const std::uint8_t> path) -> bool { return impl_->watch(subscription_id, path); }
void NativeLocationWatcher::unwatch(std::uint64_t subscription_id) { impl_->unwatch(subscription_id); }

auto start_native_location_watcher(::rust::Box<WatchPublisher> publisher) -> std::unique_ptr<NativeLocationWatcher> { return std::make_unique<NativeLocationWatcher>(std::move(publisher)); }

} // namespace dual_pane_desktop
