#import <AppKit/AppKit.h>
#import <QuickLookUI/QuickLookUI.h>

#include "dual_pane_desktop/quick_look_preview.hpp"

#include <QtWidgets/QWidget>

#include <cmath>

// Objective-C blocks and the surrounding desktop code use native and Qt
// spellings that clang-tidy's C++ style checks cannot rewrite usefully.
// NOLINTBEGIN(modernize-redundant-void-arg,readability-braces-around-statements,modernize-use-trailing-return-type,readability-trailing-comma)
@interface DualPaneQuickLookController : NSResponder <QLPreviewPanelDataSource>
@property(nonatomic, strong) NSURL *item;
@property(nonatomic, copy) void (^released)(void);
@property(nonatomic, copy) void (^interrupted)(void);
@property(nonatomic, copy) void (^ended)(void);
@property(nonatomic, weak) NSWindow *workspaceWindow;
@property(nonatomic, weak) NSResponder *responderAnchor;
@property(nonatomic, weak) NSResponder *previousResponder;
@property(nonatomic, strong) id keyObserver;
@property(nonatomic, strong) id deactivateObserver;
@property(nonatomic, strong) id releaseMonitor;
@property(nonatomic) BOOL held;
@end

@implementation DualPaneQuickLookController
- (NSInteger)numberOfPreviewItemsInPreviewPanel:(QLPreviewPanel *)panel {
    return self.item == nil ? 0 : 1;
}
- (id<QLPreviewItem>)previewPanel:(QLPreviewPanel *)panel previewItemAtIndex:(NSInteger)index {
    return index == 0 ? self.item : nil;
}
- (BOOL)acceptsPreviewPanelControl:(QLPreviewPanel *)panel {
    return YES;
}
- (void)beginPreviewPanelControl:(QLPreviewPanel *)panel {
    panel.dataSource = self;
}
- (void)endPreviewPanelControl:(QLPreviewPanel *)panel {
    if (panel.dataSource == self)
        panel.dataSource = nil;
    if (self.ended != nil)
        self.ended();
}
@end

namespace dual_pane_desktop {

constexpr unsigned short space_key_code = 49;

class QuickLookPreviewController::Implementation final {
  public:
    explicit Implementation(QWidget *widget) : widget_(widget), controller_([DualPaneQuickLookController new]) {}
    ~Implementation() { dismiss(); }
    Implementation(const Implementation &) = delete;
    auto operator=(const Implementation &) -> Implementation & = delete;
    Implementation(Implementation &&) = delete;
    auto operator=(Implementation &&) -> Implementation & = delete;

    auto present(const QuickLookItem &item, bool enlarged, std::function<void()> released, std::function<void()> interrupted) -> bool {
        @autoreleasepool {
            if (widget_ == nullptr || widget_->window() == nullptr)
                return false;
            // Qt exposes the Cocoa NSView as an integer-sized native handle.
            auto *view = (__bridge NSView *)(reinterpret_cast<void *>(widget_->window()->winId())); // NOLINT(bugprone-casting-through-void,cppcoreguidelines-pro-type-reinterpret-cast,performance-no-int-to-ptr)
            auto *window = view.window;
            if (window == nil)
                return false;
            NSResponder *anchor = window.firstResponder;
            if (anchor == nil)
                return false;
            if (item.path.isEmpty() || item.path.at(0) != '/' || item.path.contains('\0'))
                return false;
            NSURL *url = [NSURL fileURLWithFileSystemRepresentation:item.path.constData() isDirectory:item.directory relativeToURL:nil];
            if (url == nil)
                return false;
            controller_.workspaceWindow = window;
            controller_.responderAnchor = anchor;
            // Quick Look searches from the current first responder. Qt may
            // have more than one native view, so the window's content view
            // is not necessarily on that responder's path.
            controller_.previousResponder = anchor.nextResponder;
            controller_.nextResponder = controller_.previousResponder;
            anchor.nextResponder = controller_;
            controller_.item = url;
            controller_.held = enlarged;
            controller_.released = [callback = std::move(released)] { callback(); };
            controller_.interrupted = [callback = std::move(interrupted)] { callback(); };
            controller_.ended = ^{
              control_ended();
            };
            install_monitors();
            QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
            if (panel == nil) {
                dismiss();
                return false;
            }
            [panel updateController];
            if (enlarged) {
                // Set the usable frame before the system shows the panel. The
                // following native show operation supplies the natural macOS
                // opening animation instead of a visually surprising resize.
                NSScreen *screen = panel.screen;
                if (screen == nil)
                    screen = window.screen;
                if (screen == nil)
                    screen = NSScreen.mainScreen;
                if (screen != nil) {
                    const NSRect frame = screen.visibleFrame;
                    if (std::isfinite(frame.origin.x) && std::isfinite(frame.origin.y) && std::isfinite(frame.size.width) && std::isfinite(frame.size.height) && frame.size.width > 0 && frame.size.height > 0)
                        [panel setFrame:frame display:NO animate:NO];
                }
            }
            // A hidden panel may leave currentController nil until becoming
            // key. Check acquisition only after presentation has triggered
            // beginPreviewPanelControl: and installed our data source.
            presented_ = YES;
            [panel makeKeyAndOrderFront:nil];
            if (panel.currentController != controller_ || panel.dataSource != controller_) {
                dismiss();
                return false;
            }
            return YES;
        }
    }

    void dismiss() {
        @autoreleasepool {
            remove_monitors();
            controller_.held = NO;
            const bool was_presented = presented_;
            presented_ = false;
            if (was_presented) {
                QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
                if (panel != nil && panel.currentController == controller_ && panel.dataSource == controller_)
                    [panel orderOut:nil];
            }
            if (controller_.responderAnchor != nil && controller_.responderAnchor.nextResponder == controller_)
                controller_.responderAnchor.nextResponder = controller_.previousResponder;
            controller_.responderAnchor = nil;
            controller_.item = nil;
            controller_.released = nil;
            controller_.interrupted = nil;
            controller_.ended = nil;
        }
    }

  private:
    void install_monitors() {
        __weak DualPaneQuickLookController *weak = controller_;
        controller_.releaseMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyUp
                                                                           handler:^NSEvent *(NSEvent *event) {
                                                                             DualPaneQuickLookController *controller = weak;
                                                                             if (controller != nil && controller.held && event.keyCode == space_key_code) {
                                                                                 controller.held = NO;
                                                                                 if (controller.released != nil)
                                                                                     controller.released();
                                                                             }
                                                                             return event;
                                                                           }];
        controller_.deactivateObserver = [[NSNotificationCenter defaultCenter] addObserverForName:NSApplicationDidResignActiveNotification
                                                                                           object:nil
                                                                                            queue:nil
                                                                                       usingBlock:^(NSNotification *) {
                                                                                         // Once an early release leaves the panel open, its lifetime is
                                                                                         // entirely system-owned. Only an in-progress hold is interrupted.
                                                                                         if (controller_.held)
                                                                                             interrupt();
                                                                                       }];
        controller_.keyObserver = [[NSNotificationCenter defaultCenter] addObserverForName:NSWindowDidBecomeKeyNotification
                                                                                    object:nil
                                                                                     queue:nil
                                                                                usingBlock:^(NSNotification *note) {
                                                                                  DualPaneQuickLookController *controller = weak;
                                                                                  if (controller == nil || !controller.held)
                                                                                      return;
                                                                                  QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
                                                                                  if (note.object != panel && note.object != controller.workspaceWindow)
                                                                                      interrupt();
                                                                                }];
    }
    void remove_monitors() {
        if (controller_.releaseMonitor != nil)
            [NSEvent removeMonitor:controller_.releaseMonitor];
        controller_.releaseMonitor = nil;
        NSNotificationCenter *center = [NSNotificationCenter defaultCenter];
        if (controller_.deactivateObserver != nil)
            [center removeObserver:controller_.deactivateObserver];
        if (controller_.keyObserver != nil)
            [center removeObserver:controller_.keyObserver];
        controller_.deactivateObserver = nil;
        controller_.keyObserver = nil;
    }
    void interrupt() {
        if (!presented_)
            return;
        auto callback = controller_.interrupted;
        dismiss();
        if (callback != nil)
            callback();
    }
    void control_ended() {
        if (!presented_)
            return;
        auto callback = controller_.interrupted;
        dismiss();
        if (callback != nil)
            callback();
    }

    QWidget *widget_;
    DualPaneQuickLookController *controller_;
    bool presented_ = false;
};

QuickLookPreviewController::QuickLookPreviewController(QWidget *workspace_widget) : implementation_(std::make_unique<Implementation>(workspace_widget)) {}
QuickLookPreviewController::~QuickLookPreviewController() = default;
auto QuickLookPreviewController::present(const QuickLookItem &item, bool enlarged, std::function<void()> released, std::function<void()> interrupted) -> bool { return implementation_->present(item, enlarged, std::move(released), std::move(interrupted)); }
void QuickLookPreviewController::dismiss() { implementation_->dismiss(); }

QuickLookGesture::QuickLookGesture(QuickLookPreviewService *service, std::function<void()> failure, std::function<void(Release)> released, std::function<void()> interrupted) : service_(service), failure_(std::move(failure)), released_(std::move(released)), interrupted_(std::move(interrupted)) {}

auto QuickLookGesture::press(const QuickLookItem &item, bool plain_space, bool auto_repeat) -> bool {
    if (!plain_space || auto_repeat || item.path.isEmpty() || active_)
        return false;
    const auto next = generation_ + 1;
    active_ = true;
    held_ = true;
    hold_elapsed_ = false;
    generation_ = next;
    item_ = item;
    return true;
}

auto QuickLookGesture::hold_elapsed(std::uint64_t generation) -> bool {
    if (!held_ || generation != generation_ || hold_elapsed_)
        return false;
    hold_elapsed_ = true;
    if (service_->present(item_, true, [this, generation] {
                              const auto result = released(generation);
                              if (released_)
                                  released_(result); }, [this, generation] {
                              interrupted(generation);
                              if (interrupted_)
                                  interrupted_(); }))
        return true;
    active_ = false;
    held_ = false;
    failure_();
    return false;
}

auto QuickLookGesture::released(std::uint64_t generation) -> Release {
    if (!held_ || generation != generation_)
        return Release::Ignored;
    held_ = false;
    if (!hold_elapsed_) {
        if (!service_->present(item_, false, {}, [this, generation] {
                                  interrupted(generation);
                                  if (interrupted_)
                                      interrupted_(); })) {
            active_ = false;
            failure_();
            return Release::Ignored;
        }
        return Release::Retained;
    }
    active_ = false;
    return Release::Dismiss;
}

void QuickLookGesture::interrupted(std::uint64_t generation) {
    if (generation != generation_)
        return;
    active_ = false;
    held_ = false;
    hold_elapsed_ = false;
}

auto QuickLookGesture::generation() const -> std::uint64_t { return generation_; }
auto QuickLookGesture::active() const -> bool { return active_; }
auto QuickLookGesture::held() const -> bool { return held_; }

namespace {
class FakeQuickLookPreview final : public QuickLookPreviewService {
  public:
    auto present(const QuickLookItem &item, bool enlarged, std::function<void()> released, std::function<void()> interrupted) -> bool override {
        ++present_calls;
        last_item = item;
        last_enlarged = enlarged;
        release = std::move(released);
        interrupt = std::move(interrupted);
        return presentation_succeeds;
    }
    void dismiss() override { ++dismiss_calls; }

    bool presentation_succeeds = true;
    int present_calls = 0;
    int dismiss_calls = 0;
    QuickLookItem last_item;
    bool last_enlarged = false;
    std::function<void()> release;
    std::function<void()> interrupt;
};
} // namespace

auto quick_look_seam_tests() -> bool { // NOLINT(readability-function-cognitive-complexity)
    FakeQuickLookPreview preview;
    int failures = 0;
    int retained = 0;
    int dismissed = 0;
    int interruptions = 0;
    QuickLookGesture gesture(&preview, [&failures] { ++failures; }, [&](QuickLookGesture::Release release) {
        if (release == QuickLookGesture::Release::Retained) {
            ++retained;
        } else if (release == QuickLookGesture::Release::Dismiss) {
            ++dismissed;
            preview.dismiss();
        } }, [&interruptions] { ++interruptions; });
    // No cursor, modifiers, and auto-repeat never reach the native facade.
    if (gesture.press({}, true, false) || gesture.press({QByteArray("/tmp/item"), false}, false, false) || gesture.press({QByteArray("/tmp/item"), false}, true, true) || preview.present_calls != 0)
        return false;
    const QByteArray exact_path("/tmp/image\xFF.png");
    if (!gesture.press({exact_path, false}, true, false) || preview.present_calls != 0 || !gesture.active() || !gesture.held())
        return false;
    const auto first = gesture.generation();
    // A repeated initial press cannot replace the active preview's callbacks.
    if (gesture.press({QByteArray("/tmp/other"), false}, true, false) || preview.present_calls != 0)
        return false;
    if (gesture.released(first) != QuickLookGesture::Release::Retained || preview.present_calls != 1 || preview.last_item.path != exact_path || preview.last_item.directory || preview.last_enlarged || !gesture.active() || gesture.held())
        return false;
    const auto stale_interrupt = preview.interrupt;
    preview.interrupt();
    if (gesture.active() || interruptions != 1)
        return false;
    if (!gesture.press({QByteArray("/tmp/folder"), true}, true, false))
        return false;
    const auto second = gesture.generation();
    stale_interrupt();
    if (!gesture.held() || dismissed != 0 || !gesture.hold_elapsed(second) || preview.present_calls != 2 || !preview.last_item.directory || !preview.last_enlarged || gesture.hold_elapsed(second))
        return false;
    preview.release();
    if (dismissed != 1 || preview.dismiss_calls != 1 || gesture.active())
        return false;
    preview.presentation_succeeds = false;
    if (!gesture.press({QByteArray("/tmp/failure"), false}, true, false) || gesture.hold_elapsed(gesture.generation()) || failures != 1 || preview.present_calls != 3 || gesture.active())
        return false;
    return gesture.released(first) == QuickLookGesture::Release::Ignored && retained == 0;
}

} // namespace dual_pane_desktop
// NOLINTEND(modernize-redundant-void-arg,readability-braces-around-statements,modernize-use-trailing-return-type,readability-trailing-comma)
