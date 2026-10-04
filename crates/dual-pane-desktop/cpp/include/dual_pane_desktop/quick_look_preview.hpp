#ifndef DUAL_PANE_DESKTOP_QUICK_LOOK_PREVIEW_HPP
#define DUAL_PANE_DESKTOP_QUICK_LOOK_PREVIEW_HPP

#include <cstdint>
#include <functional>
#include <memory>

#include <QtCore/QByteArray>

class QWidget;

namespace dual_pane_desktop {

struct QuickLookItem {
    QByteArray path;
    bool directory = false;
};

class QuickLookPreviewService {
  public:
    QuickLookPreviewService() = default;
    virtual ~QuickLookPreviewService() = default;
    QuickLookPreviewService(const QuickLookPreviewService &) = delete;
    auto operator=(const QuickLookPreviewService &) -> QuickLookPreviewService & = delete;
    QuickLookPreviewService(QuickLookPreviewService &&) = delete;
    auto operator=(QuickLookPreviewService &&) -> QuickLookPreviewService & = delete;
    [[nodiscard]] virtual auto present(const QuickLookItem &item, std::function<void()> released, std::function<void()> interrupted) -> bool = 0;
    [[nodiscard]] virtual auto maximize() -> bool = 0;
    virtual void dismiss() = 0;
};

/// Private macOS facade for the shared system Quick Look panel.  AppKit and
/// Quick Look types deliberately do not escape this desktop-only boundary.
class QuickLookPreviewController final : public QuickLookPreviewService {
  public:
    explicit QuickLookPreviewController(QWidget *workspace_widget);
    ~QuickLookPreviewController() override;
    QuickLookPreviewController(const QuickLookPreviewController &) = delete;
    auto operator=(const QuickLookPreviewController &) -> QuickLookPreviewController & = delete;
    QuickLookPreviewController(QuickLookPreviewController &&) = delete;
    auto operator=(QuickLookPreviewController &&) -> QuickLookPreviewController & = delete;

    [[nodiscard]] auto present(const QuickLookItem &item, std::function<void()> released, std::function<void()> interrupted) -> bool override;
    /// Returns false when the standard panel has no current screen.  It stays
    /// presented so the matching key release can still dismiss it.
    [[nodiscard]] auto maximize() -> bool override;
    void dismiss() override;

  private:
    class Implementation;
    std::unique_ptr<Implementation> implementation_;
};

/// Desktop-local held-Space state. The service seam keeps native panel work
/// out of the deterministic gesture tests.
class QuickLookGesture final {
  public:
    enum class Release : std::uint8_t { Ignored,
                                        Retained,
                                        Dismiss,
    };

    QuickLookGesture(QuickLookPreviewService *service, std::function<void()> failure, std::function<void(Release)> released = {}, std::function<void()> interrupted = {});
    [[nodiscard]] auto press(const QuickLookItem &item, bool plain_space, bool auto_repeat) -> bool;
    [[nodiscard]] auto timer_expired(std::uint64_t generation) -> bool;
    [[nodiscard]] auto released(std::uint64_t generation) -> Release;
    void interrupted(std::uint64_t generation);
    [[nodiscard]] auto generation() const -> std::uint64_t;
    [[nodiscard]] auto active() const -> bool;
    [[nodiscard]] auto held() const -> bool;

  private:
    QuickLookPreviewService *service_;
    std::function<void()> failure_;
    std::function<void(Release)> released_;
    std::function<void()> interrupted_;
    std::uint64_t generation_ = 0;
    bool active_ = false;
    bool held_ = false;
    bool timer_expired_ = false;
};

/// Exercises the fakeable gesture seam without a real Quick Look provider.
[[nodiscard]] auto quick_look_seam_tests() -> bool;

} // namespace dual_pane_desktop

#endif // DUAL_PANE_DESKTOP_QUICK_LOOK_PREVIEW_HPP
