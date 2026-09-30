# P2-M3 overview: Display and activate the Standard Layout

Status: Done

P2-M3 replaces the temporary single-pane desktop shell with the two-browser Standard Layout. It keeps browser state and directory work in one pane-addressed workspace coordinator while rendering independent native list models.

## Goal

Show two real, independently usable directory listings, with the active Browser visibly distinct and all agreed browsing gestures routed through pane-addressed application commands.

## Completion checklist

- [x] The Standard Layout renders the visual Sidebar hierarchy, resizable Sidebar Splitter and Browser Divider, two Navigation Panes with read-only Path Edit Controls, and two Folder Panes with both toolbar rows, lists, and status bars. Evidence: native Widgets composition compiled and passed `make check` on 2026-10-01.
- [x] One GUI-thread coordinator owns one `Workspace`, one runtime, and independent left/right presentation state; both browsers start with separate reads at the launch location. Evidence: `WorkspaceSession::start` dispatches both pane-addressed reads through one runtime.
- [x] Pointer activation ordering, empty-space clearing, Up, Option+F, row movement, Left, Right, Return, Command-Up, and specified no-op gestures follow [mvp.md](../mvp.md#interaction). Evidence: native `ListingView` routes those gestures only through pane-addressed model calls; the P2-M3 review reverified the macOS Command mapping and synchronized pointer focus, active Folder Pane styling, and application activation.
- [x] Tests demonstrate cross-browser loading, navigation, selection, failures, and cancellation isolation. Evidence: the five `two_pane_workspace.rs` integration tests passed on 2026-10-01.
- [x] Documentation links and terminology are current; `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass. Evidence: all commands exited 0 on 2026-10-01.
