# P4-M8 architecture

Status: Planned

The advanced Path Edit Control renders the active tab's confirmed location and sends typed navigation commands for breadcrumb and column targets. Folder-option enumeration stays on a worker. The [overview checklist](milestone-08-overview.md#completion-checklist) defines acceptance; M1 fixes the basic concept and collapse-after-selection, while M8 planning settles the remaining interactions before implementation.

## Modes and ownership

The active Browser Tab's confirmed `Location` is the single source of truth for breadcrumb components and selected column rows. The Qt desktop owns visual expansion, scroll positions, hover, and text-editor draft; the application owns tab navigation, history, request identity, and the outcome of a submitted target. A breadcrumb or column click carries the exact target `Location`, not an elided label or a string assembled from visible component names. The target is validated through the same application navigation transition and worker read used by P4-M6. Only a successful navigation makes a new path confirmed.

Keep three presentation modes coherent: collapsed breadcrumb, expanded column navigator, and typed-path editor. Rendering a new active tab rebuilds the breadcrumb from that tab's confirmed location. A pending navigation can show a target cue but must not claim that the path changed. A collapse does not cancel the tab's ordinary navigation; it cancels only option-list requests no longer needed. M8 planning decides whether expanded state is Browser-local or tab-local and whether it is restored after relaunch.

## Folder-option requests

Add a purpose-specific request for the visible columns' child folders rather than enumerating ancestor directories on the GUI thread. A request includes Browser, Tab ID, path component identity, visibility policy, and a generation tied to the current expanded view. The desktop worker returns only the folder options and typed errors required for that column, with exact native paths; it does not decide which target is selected. Limit pending requests and payload size per Browser. Collapse, tab switch, path change, or a newer request invalidates old generations; their late results cannot repopulate the control.

Respect the current file and folder visibility settings when listing options, without treating a hidden option as an invalid path if it is already a confirmed component. Root, repeated folder names, symlinked folders, packages, unreadable parents, and paths with undecodable bytes need explicit tests. Native probes, metadata, and icon work remain off the GUI thread. A per-column loading, empty, or retryable error state leaves the last successful Folder Items and confirmed path untouched.

## Desktop composition and verification

Extend the Navigation Pane, which `Browser` in `crates/dual-pane-desktop/cpp/include/dual_pane_desktop/folder_pane.hpp` composes, with accessible breadcrumb controls, an Expand/Collapse button, and a vertically expanding column area. Reuse the existing dark Browser surface, focus indicator, and splitter conventions; the expanded area has a maximum that leaves the Folder Pane usable. At narrow widths, scroll columns horizontally or show the focused column rather than compressing every label beyond use. Keyboard arrows move through visible options, Return activates a target, and Escape follows the M8-approved focus and collapse order.

Keep Qt model and widget changes on their affinity thread. Qt-free tests prove exact breadcrumb targets, token rejection, and tab-local navigation. Native checks verify pointer targets, keyboard and VoiceOver access, two-Browser isolation, pending and error feedback, layout at supported widths, and cancellation of stale column reads using a disposable profile and temporary directories.
