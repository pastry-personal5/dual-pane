#ifndef DUAL_PANE_DESKTOP_LOCATION_WATCHER_HPP
#define DUAL_PANE_DESKTOP_LOCATION_WATCHER_HPP

#include "rust/cxx.h"

#include <cstdint>
#include <memory>

namespace dual_pane_desktop {

struct WatchPublisher;

class NativeLocationWatcher final {
  public:
    explicit NativeLocationWatcher(::rust::Box<WatchPublisher> publisher);
    ~NativeLocationWatcher();
    NativeLocationWatcher(const NativeLocationWatcher &) = delete;
    auto operator=(const NativeLocationWatcher &) -> NativeLocationWatcher & = delete;
    NativeLocationWatcher(NativeLocationWatcher &&) = delete;
    auto operator=(NativeLocationWatcher &&) -> NativeLocationWatcher & = delete;

    [[nodiscard]] auto watch(std::uint64_t subscription_id, ::rust::Slice<const std::uint8_t> path) -> bool;
    void unwatch(std::uint64_t subscription_id);

  private:
    class Impl;
    std::unique_ptr<Impl> impl_;
};

[[nodiscard]] auto start_native_location_watcher(::rust::Box<WatchPublisher> publisher) -> std::unique_ptr<NativeLocationWatcher>;

} // namespace dual_pane_desktop

#endif // DUAL_PANE_DESKTOP_LOCATION_WATCHER_HPP
