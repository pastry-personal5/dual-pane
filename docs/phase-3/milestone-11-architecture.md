# P3-M11 architecture: Native Quick Look

Status: Active

P3-M11 is a macOS desktop concern. It reads the byte-exact path of the Folder Items cursor, asks the system Quick Look Panel to preview one URL, and manages a short held-key lifecycle. It adds no durable workspace state.

## Boundary and build

The domain, application, and workspace bridge stay unchanged. The adapter view model derives a byte-exact absolute path from the location and entry name. `FolderItemsListModel` exposes that path as a `QByteArray` role and the directory kind as a separate role; the existing display-path role is lossy for non-UTF-8 names and is not used for preview identity. The Qt view reads the native roles when Space is first pressed. No `ActionId` is added because this is a fixed native gesture, not a configurable shortcut.

The desktop crate adds one private C++ facade header and one Objective-C++ implementation unit. `build.rs` compiles that unit as Objective-C++ and emits the macOS framework link for `QuickLookUI`; it does not add a Cargo dependency, build tool, or non-macOS target. The facade keeps Foundation, AppKit, and Quick Look types out of `desktop_window.cpp` and out of every Rust bridge.

The facade owns a `QuickLookPreviewController` for the workspace native window. After the Qt window exists, the Objective-C++ implementation obtains its macOS `NSView` through the workspace `QWidget::winId()` native handle and its `NSWindow` through that view. It inserts an `NSResponder` controller immediately after the window's current first responder and restores the exact prior successor only if it still owns that link on teardown. Quick Look searches from the active first responder; using that responder directly avoids assuming which Qt native views are on its chain. The controller implements the Quick Look panel-control and data-source methods. It constructs one `NSURL` with `fileURLWithFileSystemRepresentation:isDirectory:relativeToURL:` so filenames do not pass through display text. It calls `updateController`, presents the shared `QLPreviewPanel`, then verifies controller and data-source ownership. A hidden panel can leave `currentController` nil until it becomes key, so checking before presentation falsely reports failure. The facade changes the panel's datasource or frame only while it is the current controller. One workspace means one controller and one preview item; another application's use of the shared panel is never modified.

## Gesture lifecycle

`FolderItemsList::keyPressEvent` recognizes only `Qt::Key_Space` with no user modifiers and `!event->isAutoRepeat()`. It samples the cursor item and starts a precise 512 ms single-shot timer without opening a panel. A matching Qt key-up before expiry stops the timer and asks the facade to present the normal sized system panel. The gesture tags callbacks with a monotonically increasing preview generation. When the timer fires first, the facade installs an AppKit local key-up monitor before making the panel key. That monitor sees the release even though Quick Look has taken focus; it reports only the physical Space release matching the active generation.

Before the timer expires, the matching key release stops the timer and opens the normal sized panel for system controls and dismissal. At expiry, the facade resolves the panel, workspace, or main screen's `visibleFrame`, applies it before it makes the panel key, and lets the system perform its native Quick Look opening animation; it never enters macOS full screen or performs a post-open resize. If no usable screen frame exists, it opens the normal sized panel. The matching release then orders the held panel out and clears the controller's URL and held state. A close, application deactivation, unrelated key-window activation, responder-control loss, or facade destruction invalidates the generation, stops the timer, removes the monitor, and orders the held panel out. The expected key-window transfer from the workspace to its Quick Look Panel does not interrupt the hold.

The facade reports only mechanics Dual Pane can know: panel acquisition, responder control, normal or screen-frame presentation, and dismissal. Quick Look has no reliable preflight for every file provider or format, so an item that opens to system "no preview" feedback is not an application failure. A failure to acquire or present the panel reaches a desktop-local three-second cue over the source Browser Status Bar; the cue then redraws the model's current status text. It is presentation state, not an application rejection or Notice. The controller does not navigate links, mutate the cursor or selection, subscribe to listing changes, persist panel geometry, or intercept ordinary system controls.

## Verification

Keep the lifecycle state machine behind a narrow fakeable facade: fake normal and enlarged presentation, local-release callback, panel-control loss, and the local Status Bar cue. Desktop tests assert that an initial press samples one cursor path without presenting it, an early release opens the normal panel, a held timer opens the enlarged panel, and a stale callback cannot affect a newer preview generation. They also prove no cursor, modifier, and auto-repeat paths make no facade call, and that a cue restores the latest model text after its timer.

The native check is required because a shared system panel, responder-chain selection, key-window transfer, panel screen choice, and Quick Look providers cannot be established by a fake. Run it only against disposable test-owned files in a temporary directory; inspect folders, packages, and links without mutating user data. Then run the repository gate named in the overview.

## Risks

- Quick Look is a shared, system-owned panel. Correct responder-chain ownership and teardown prevent Dual Pane from overwriting another controller's datasource or delegate.
- The panel can become key before the original Space release, so the AppKit local monitor is required for reliable hold behavior.
