# P4-M11: Modularize the desktop GUI code

Status: Planned

This milestone splits the desktop crate's two outsized files — the 5,771-line `desktop_window.cpp`, which defines roughly thirty unrelated Qt widget classes in one anonymous namespace, and the handful of 1,000+ line Rust bridge/model files it talks to — into smaller, feature-area modules. It also adds one registration seam for folder-item actions so that adding a future action no longer requires editing five files at once. It is a structural refactor: no product behavior, setting, or shortcut changes. It starts right after P4-M10 finishes and runs ahead of P4-M6 through P4-M8, the same way P4-M9 ran ahead of them.

## Scope

In scope:

- Splitting `desktop_window.cpp` and its single header into feature-area `.hpp`/`.cpp` pairs under `cpp/src` and `cpp/include/dual_pane_desktop/`, wired into the existing CMake source list.
- Splitting `runtime.rs`, `settings_storage.rs`, `operation_step.rs`, `browser_session.rs`, `workspace_bridge.rs`, and `folder_items_list_model.rs` into smaller modules along matching feature boundaries.
- A single registration seam for folder-item actions (the context-menu/action pattern) that the context-menu UI, the Context Menus Settings visibility list, and the Folder Items model all read from.

Out of scope:

- Any change to product behavior, Browser behavior, Settings behavior, shortcuts, or persisted data formats.
- Splitting any other large Rust crate (`dual-pane-domain`, `dual-pane-application`, `dual-pane-adapters` are already Qt-free and already organized into modules).
- Designing seams for other recurring multi-file patterns (for example, adding a new setting or a new Favorites feature); this milestone's seam work targets only the folder-item action pattern that caused the P4-M10 foundation and delivery commits to each touch five files.
- New automated tests beyond what is needed to exercise the moved code through its existing call sites.

## Evidence for the seam

`2df5353` (context-menu foundation) and `5bd296a` (configurable context menus and settings page) each touched `desktop_window.cpp`, `browser_session.rs`, `folder_items_list_model.rs`, `settings_storage.rs`, and `workspace_bridge.rs` to add or finish one feature. That five-file footprint for one action is the shotgun-surgery pattern this milestone's seam addresses.

## Owner decisions

The owner decided the P4-M11 scope on 2026-10-21: split both the C++ and Rust desktop code, group the C++ split by feature area rather than one class per file, include a seam design (not just a structural move), scope the seam to the folder-item action pattern only, and treat the existing `make check` gate plus the five offscreen native scripts as the acceptance bar, with no new tests required.

## Completion checklist

- [ ] `desktop_window.cpp` is split into feature-area modules (at least: window shell, toolbar and sidebar, tabs strip, folder pane, favorites panel, operation panels, settings window, shortcuts) with matching headers under `cpp/include/dual_pane_desktop/`, and the CMake source list is updated. No class's behavior changes.
- [ ] `runtime.rs`, `settings_storage.rs`, `operation_step.rs`, `browser_session.rs`, `workspace_bridge.rs`, and `folder_items_list_model.rs` are split into modules along the same feature boundaries used on the C++ side, so a feature's C++ and Rust code are easy to find together.
- [ ] A single folder-item action registration seam exists (id, label, shortcut key, availability/visibility rule, and the bridge call it issues) and the context-menu UI, Context Menus Settings visibility list, and Folder Items model all read from it instead of each keeping their own list.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.
- [ ] All five native scripts pass: `check-settings-window.sh`, `check-text-editor-focus.sh`, `check-application-title-row.sh`, `check-folder-items-context-menu.sh`, `check-single-instance.sh`.
- [ ] `make check` passes.
- [ ] `docs/README.md` and `docs/phase-4/changelog.md` are updated for the new module layout and seam.

## Acceptance evidence

Not yet started.
