#import <AppKit/AppKit.h>

#include "dual_pane_desktop/application_activation.hpp"

namespace dual_pane_desktop {
void activate_application() {
    [NSApp activate];
}
} // namespace dual_pane_desktop
