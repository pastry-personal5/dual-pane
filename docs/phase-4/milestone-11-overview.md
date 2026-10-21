# P4-M11: Modularize the desktop GUI code

Status: Done

This milestone split the desktop crate's two outsized files — the 5,771-line `desktop_window.cpp`, which defined roughly thirty unrelated Qt widget classes in one anonymous namespace, and the handful of 1,000+ line Rust bridge/model files it talks to — into smaller, feature-area modules. It also moved every per-action fact of the folder-item action pattern into one catalogue table in the application crate, so adding a future action no longer means editing a parallel list in each layer. It was a structural refactor: no product behavior, setting, or shortcut changed. It ran right after P4-M10 and ahead of P4-M6 through P4-M8, the same way P4-M9 ran ahead of them.

## Scope

In scope:

- Splitting `desktop_window.cpp` into feature-area `.hpp`/`.cpp` modules under `cpp/src` and `cpp/include/dual_pane_desktop/`, wired into `build.rs` and the C++ format and lint scripts.
- Splitting `runtime.rs`, `settings_storage.rs`, `operation_step.rs`, `browser_session.rs`, `workspace_bridge.rs`, and `folder_items_list_model.rs` into smaller modules along matching feature boundaries.
- One action catalogue table for the folder-item action pattern that the application crate, the Context Menus Settings page, the Shortcuts Settings page, and the Folder Items Context Menu all read from.

Out of scope:

- Any change to product behavior, Browser behavior, Settings behavior, shortcuts, or persisted data formats.
- Splitting any other large Rust crate (`dual-pane-domain`, `dual-pane-application`, `dual-pane-adapters` are already Qt-free and already organized into modules).
- Designing seams for other recurring multi-file patterns (for example, adding a new setting or a new Favorites feature).
- New automated tests beyond what is needed to exercise the moved code through its existing call sites.

## Evidence for the seam

`2df5353` (context-menu foundation) and `5bd296a` (configurable context menus and settings page) each touched `desktop_window.cpp`, `browser_session.rs`, `folder_items_list_model.rs`, `settings_storage.rs`, and `workspace_bridge.rs` to add or finish one feature. Later P4-M10 work made the desktop side generic: `b0e3ffb` added the twelfth action, Open in Finder, without touching `settings_storage.rs`, `workspace_bridge.rs`, `folder_items_list_model.rs`, or the menu code. What it still edited was per-action data spread across the Qt-free application crate (`ActionId::ALL`, name, scope, default shortcut, Context Menu group, eligible-action list, availability `match`, invocation `match`) and one hard-coded action list in the C++ Shortcuts page. That remaining spread is what this milestone's catalogue table replaces.

## Owner decisions

The owner decided the P4-M11 scope on 2026-10-21: split both the C++ and Rust desktop code, group the C++ split by feature area rather than one class per file, include a seam design (not just a structural move), and treat the existing `make check` gate plus the five offscreen native scripts as the acceptance bar, with no new tests required.

When implementation found that the desktop-side touch points already read the catalogue generically, the owner chose the full action table over a narrower fix on 2026-10-21: one table in the application crate holding every per-action fact, read by the Workspace, the Settings pages, and the menu.

## Completion checklist

- [x] `desktop_window.cpp` is split into feature-area modules (window shell, toolbar and sidebar, tabs strip, folder pane, favorites panel, operation panels, settings window, shortcuts, plus shared helpers and the offscreen checks) with matching headers under `cpp/include/dual_pane_desktop/`, and `build.rs` plus the C++ format and lint scripts list the new files. No class's behavior changes.
- [x] `runtime.rs`, `settings_storage.rs`, `operation_step.rs`, `browser_session.rs`, `workspace_bridge.rs`, and `folder_items_list_model.rs` are split into modules along the same feature boundaries used on the C++ side, so a feature's C++ and Rust code are easy to find together.
- [x] A single folder-item action table exists (id, name, scope, Shortcuts page category, factory shortcuts, and for menu actions: group, row position, default visibility, target, availability rule, and invocation) and the Workspace, the Context Menus and Shortcuts Settings pages, and the Folder Items Context Menu read from it instead of each keeping their own list.
- [x] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.
- [x] All five native scripts pass: `check-settings-window.sh`, `check-text-editor-focus.sh`, `check-application-title-row.sh`, `check-folder-items-context-menu.sh`, `check-single-instance.sh`.
- [x] `make check` passes.
- [x] `docs/README.md` and `docs/phase-4/changelog.md` are updated for the new module layout and seam.

## Acceptance evidence

- `make check` passed: Rust formatting, C++ formatting, Clippy with warnings denied, clang-tidy on every new header and source, and the full test suite. The desktop crate still has 229 `#[test]` functions, all moved verbatim.
- The five native scripts exited 0 against the split build, both before and after the action table replaced the per-action matches.
- `desktop_window.cpp` shrank from 5,776 to about 450 lines; the largest new C++ file is `folder_pane.hpp` at about 1,500 lines. Of the six split Rust files, the largest is now `folder_items_list_model.rs` at about 510 lines; their test modules moved to `tests.rs` files beside them.
- The action table's order and position invariants are compile-time assertions in `crates/dual-pane-application/src/actions.rs`.
