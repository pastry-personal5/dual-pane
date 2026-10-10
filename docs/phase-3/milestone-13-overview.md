# P3-M13: Settings Window skeleton

Status: Planned

This milestone makes the existing Settings affordance usable and delivers the modal Settings Window structure, without exposing settings mutations before P3-M14. [Architecture](milestone-13-architecture.md) defines the desktop boundary; this checklist defines completion.

## Scope and fixed inputs

Use [the settings boundary](../architecture.md#6-error-settings-and-observability-boundaries), [P3-M2 settings ownership](milestone-02-architecture.md#configuration-and-ownership), and [UX terms](../ux-terms.md). P3-M13 cannot be marked Done until P3-M12 is Done.

In scope: a read-only application-to-desktop settings-interaction availability projection; enabling the existing Settings gear when settings storage is healthy; fixed `Command+,` entry; one parented window-modal Settings Window; General and Keyboard Shortcuts category placeholders; keyboard focus, Escape/title-bar dismissal, availability transitions, desktop seams, native verification, and documentation.

Out of scope: editable settings, shortcut capture, applying or persisting settings changes, a new ActionId, a menu-bar settings command, a footer button, a settings-schema change, or a new dependency.

## Completion checklist

- [ ] The Main Toolbar Settings gear and fixed `Command+,` open or raise exactly one parented, window-modal Settings Window after settings storage becomes healthy. Reopening never creates a second dialog.
- [ ] The Settings Window has a left Settings Category List with General Settings and Keyboard Shortcuts Settings entries, accessible names, descriptive placeholder pages, and initial category-list focus.
- [ ] The dialog uses no Apply, Cancel, Close, or Done footer controls. Escape and the standard title-bar close control dismiss it without modifying application state.
- [ ] Settings remains unavailable during initial load, reset/reload, timeout, and after every non-stale write or load failure from the shared settings store, including session writes. A later ordinary save does not restore it; only a successful Reset Settings followed by successful replacement load does. An unhealthy transition dismisses any open Settings Window before disabling the gear and shortcut; Notices retains the existing recovery path.
- [ ] The fixed entry does not become a customizable ActionId, is permanently reserved from P3-M14 editable bindings on macOS, does not collide with workspace binding dispatch, and cannot trigger while the modal dialog owns focus.
- [ ] Focused desktop tests cover healthy/unhealthy availability, one-window raising, keyboard and pointer entry, modality, focus, Escape, title-bar close, and failure-time dismissal. Native macOS verification covers the same behavior and accessibility.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; affected documentation and acceptance evidence are recorded before P3-M13 is marked Done.

## Acceptance evidence

- 2026-10-10 — Automated implementation evidence: the Qt-free settings-interaction availability projection, fixed `Command+,` entry, and window-modal Settings Window shell were added. `make check` passed (Rust and C++ formatting/lint plus tests).
- 2026-10-10 — Implementation review corrected Settings Window teardown: dialog completion now schedules its own deletion and relies on `QPointer`’s automatic nulling, avoiding a callback that could outlive its stack-owned pointer. Adapter coverage confirms availability changes do not rebind workspace actions; `make check` passed again.
- 2026-10-10 — P3-M13 remains Planned: P3-M12 is still Active with required native verification pending, and this milestone’s native macOS Settings Window/accessibility verification is also pending.
