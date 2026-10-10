# P3-M15: Hide macOS housekeeping items

Status: Planned

This milestone adds an owner-approved, persistent display preference for a narrow set of macOS housekeeping files and folders. It keeps intentional dotfiles and all file-operation safety behavior visible and unchanged.

## Decision gate

Before implementation, record in this overview and the Phase 3 changelog:

| Decision | Required owner choice |
|---|---|
| Matching set | Confirm the research recommendation (`.DS_Store`, `Icon\r`, `._*`, and `.AppleDouble`) or explicitly name additions/removals. |
| Default and label | Confirm the default visibility and the General-page checkbox wording. |
| Platform invisibility | Decide whether a first release ignores macOS invisible metadata, or approve the native metadata work and its reveal semantics. |

No implementation may broaden the matching set to every dotfile or add a dependency/build tool without the approval required by AGENTS.md.

## Scope and fixed inputs

Use [hidden-file research](../hidden-files-research.md), the [P3-M2 settings boundary](milestone-02-architecture.md#configuration-and-ownership), [P3-M14’s General settings boundary](milestone-14-overview.md), the existing P3-M8 temporary-item safety rule, and the [file-operation safeguards](../product-behavior.md#file-operation-safeguards).

In scope: the decision gate; one application-owned General preference; schema migration, persistence, reset/default behavior, live refresh of every open tab, worker-side listing filtering, selection/cursor reconciliation, Settings presentation, Qt-free and native verification, and documentation.

Out of scope: a generic leading-dot filter; automatic cleanup or deletion; changing copy, move, rename, Trash, permanent-delete, or recursive-scan behavior; a Finder-compatibility claim; and a per-folder or per-tab override.

## Completion checklist

- [ ] The owner-complete decision table records the exact byte-name/prefix policy, default, checkbox wording, and macOS-invisible-item scope before code changes begin.
- [ ] A typed application-owned preference has a compiled default, validation, immediate in-memory update, reset behavior, and a versioned SQLite migration; failed storage retains the existing Settings-unavailable and Notices recovery behavior.
- [ ] The General Settings page exposes one accessible checkbox only while settings storage is healthy. Its change applies immediately, persists across relaunch, and is restored by Reset Settings to the recorded default.
- [ ] Every directory read carries the effective listing policy. The desktop worker filters only the owner-approved artifacts before entry classification and sorting, while the current-launch operation-temporary exclusion remains unconditional.
- [ ] Changing the setting invalidates and rereads every open tab without blocking the GUI. Request tokens prevent an earlier-policy result from replacing a later-policy listing; standard reload reconciliation retains only still-visible selection and cursor Items.
- [ ] Deliberate Unix/tool dotfiles remain listed. Filtered artifacts receive no destructive treatment, and file-operation planning/scanning keeps its existing safety semantics regardless of display visibility.
- [ ] Tests cover exact names, byte prefixes, similarly named ordinary files, non-UTF-8 names, both preference values, current-launch temporaries with reveal enabled, live all-tab refresh, stale results, selection/cursor reconciliation, migration/default/reset/relaunch, Settings accessibility, and storage-failure behavior using temporary directories.
- [ ] Native macOS verification covers both settings values, immediate two-Browser update, relaunch persistence, keyboard access, and confirmation that representative `.git`/`.env` entries remain visible.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; affected documentation and acceptance evidence are recorded before P3-M15 is marked Done.
