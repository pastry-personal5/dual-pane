# P2-M3 architecture: Standard Layout desktop coordinator

Status: Done

The Qt Widgets boundary has a single GUI-thread `WorkspaceSession`: one application `Workspace`, one worker runtime, and one side-specific presenter per Browser. Native `ListingModel` instances are thin left/right projections; each resets only from its own presenter and sends side-addressed input through the shared coordinator.

## Native composition

The Standard Layout uses nested horizontal splitters. The outer splitter separates the Sidebar from the browser area (Sidebar Splitter); the inner splitter separates Left Browser and Right Browser (Browser Divider). Both use the same custom three-pixel native handle returned from `QSplitter::createHandle`; each handle paints the inactive Browser color at rest, adds its gray border only while hovered and not pressed, and suppresses the border throughout a primary-button drag. Splitter sizes are deliberately session-only. Each Browser renders its read-only path, concise folder label, neutral Up Button, list, and status label. Browser Tabs have no rendered state in this milestone.

## Input and rendering

Native pointer and focus events first focus and activate the relevant Browser, then issue selection, activation, clear-selection, or parent-navigation commands. Keyboard handling is confined to the list and uses the current model row count and selected row; exact Left and Command-Up navigate to the parent, while exact Right and Return activate the selected row. It emits no command for unsupported modifiers. The presenter owns location, status, entries, selection, and a pane-local listing revision, so output for one pane cannot reset or repaint the other pane’s native model. Folder Pane and selected-row colors are driven by explicit pane-active and window-active state.

## Threading

The worker wakes the Qt drain scheduler only. The scheduler drains the shared coordinator on the GUI thread and refreshes both native projections; directory I/O and cancellation remain in `Runtime`. The coordinator mutex only protects bridge entry during this GUI-thread use and never surrounds directory I/O.
