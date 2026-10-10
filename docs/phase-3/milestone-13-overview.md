# P3-M13: Settings Window skeleton

Status: Planned

This milestone makes the existing Settings affordance usable and delivers the modal Settings Window structure, without exposing settings mutations before P3-M14. [Architecture](milestone-13-architecture.md) defines the desktop boundary; this checklist defines completion.

## Scope and fixed inputs

Use [the settings boundary](../architecture.md#6-error-settings-and-observability-boundaries), [P3-M2 settings ownership](milestone-02-architecture.md#configuration-and-ownership), and [UX terms](../ux-terms.md). P3-M12 is complete before this milestone begins.

In scope: enabling the existing Settings gear when settings storage is healthy; fixed `Command+,` entry; one parented window-modal Settings Window; General and Keyboard Shortcuts category placeholders; keyboard focus, Escape/title-bar dismissal, availability transitions, desktop seams, native verification, and documentation.

Out of scope: editable settings, shortcut capture, applying or persisting settings changes, a new ActionId, a menu-bar settings command, a footer button, or a new dependency.

## Completion checklist

- [ ] The Main Toolbar Settings gear and fixed `Command+,` open or raise exactly one parented, window-modal Settings Window after settings storage becomes healthy. Reopening never creates a second dialog.
- [ ] The Settings Window has a left Settings Category List with General Settings and Keyboard Shortcuts Settings entries, accessible names, descriptive placeholder pages, and initial category-list focus.
- [ ] The dialog uses no Apply, Cancel, Close, or Done footer controls. Escape and the standard title-bar close control dismiss it without modifying application state.
- [ ] Settings remains unavailable during initial load and whenever settings storage is unhealthy. A transition to an unhealthy state disables the gear and shortcut and dismisses any open Settings Window; Notices retains the existing recovery path.
- [ ] The fixed entry does not become a customizable ActionId, does not collide with workspace binding dispatch, and cannot trigger while the modal dialog owns focus.
- [ ] Focused desktop tests cover healthy/unhealthy availability, one-window raising, keyboard and pointer entry, modality, focus, Escape, title-bar close, and failure-time dismissal. Native macOS verification covers the same behavior and accessibility.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; affected documentation and acceptance evidence are recorded before P3-M13 is marked Done.
