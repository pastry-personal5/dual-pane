# P3-M1: UX documentation and terminology refactor

Status: Active

This milestone makes product behavior and UX documentation distinct sources of truth, then aligns the current source tree with the canonical Browser and Folder Items vocabulary. It preserves behavior and makes no new product or interaction decisions.

## Scope

In scope: the documentation migration, link updates, source/test/bridge terminology rename, and verification. Out of scope: rendering deferred Browser Tabs or Sidebar groups; new operations, persistence, sorting/filtering, and watcher behavior.

## Completion checklist

- [x] Product behavior and all three UX documents own every former active scope section exactly once.
- [x] The superseded scope document is archived and active/internal links resolve.
- [x] Source, tests, desktop modules, bridge exports, object names, and accessibility names use the canonical terms.
- [x] Legacy-component searches are clean outside archive/history and namespace/technical exemptions.
- [x] Existing behavior tests pass after renamed public interfaces are updated.
- [x] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass.
- [ ] Native-window launch confirms the two-Browser layout, interaction, styling, and accessibility labels remain intact.
