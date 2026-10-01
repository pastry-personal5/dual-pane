# P3-M2: Configs, Defaults, Settings and persistent storage

Status: Done

This milestone establishes how Phase 3 defines defaults, owns settings, and stores durable state. It delivers a responsive, versioned storage foundation for folder sort memory, Favorites, future shortcut customization, and later workspace restoration.

Supersession note: P3-M4 removed text filtering from Phase 3 behavior and restored session state. Filter references below record the completed P3-M2 storage handoff and acceptance evidence, not a current Phase 3 feature requirement. Follow the [current product scope](../product-behavior.md#included-product-scope) and [P3-M4 handoff](milestone-04-architecture.md#contract-to-later-milestones); later code and persisted action compatibility work must preserve existing records without presenting filtering as a Phase 3 feature.

## Scope

In scope: an inventory of Phase 3 configuration and state; application-owned settings values and compiled defaults; stable canonical action IDs; a preloaded persistent action/shortcut catalogue; a SQLite settings driver under Application Support; asynchronous load and coalesced save; automatic schema upgrades, validation, explicit reset, and recoverable errors; and tests using temporary directories. The first durable values are user shortcut overrides, a shared location-keyed history of up to 100 explicitly chosen folder sorts, and the ordered Favorite Group → Favorite Item records. The Settings window, shortcut editor, and Sidebar are not part of this milestone; SQLite is the sole live configuration store, so no app-managed human-readable configuration file is read or watched.

The inventory distinguishes five kinds of data:

| Kind | Owner and P3-M2 outcome |
|---|---|
| Product rules and operational configuration | Keep fixed safety rules and runtime limits in their owning modules. Document their defaults and whether later settings may change them; do not expose a user preference merely because a constant exists. |
| User settings | Provide typed values, compiled defaults, validation, and durable overrides. Initial consumers are shortcut bindings and folder sort choices. |
| Favorites | Define Qt-free persistence records, validation outcomes, and an ordered database contract for one level of Favorite Groups containing Favorite Items. P3-M5 owns the domain state and edit rules; P3-M6 owns Sidebar interactions. |
| Workspace session | Define a compatible storage contract for open tabs and their order, active tab, current locations, tab-local filter state, and active Browser. Restored tabs resolve sort from the per-folder settings; P3-M9 supplies and restores the snapshot. Back/forward history is session-only. |
| Operation recovery | Keep the runtime's safety journal separate from ordinary settings and session snapshots; the file-operation milestones own its records and recovery behavior. |

## Settings inventory

| Category | Owner and compiled default | Persistence rule | Delivery |
|---|---|---|---|
| Safety policy and runtime limits | Their existing domain/runtime owners; no user override | Not settings data | Current and file-operation milestones |
| Shortcut bindings | Application action catalogue; each canonical action's compiled default | One binding row per catalogue action; invalid rows fall back individually | P3-M2, editor later |
| Folder sort | Application `SortSpec`; natural Name ascending | One explicit choice per byte-exact location, including Name ascending; 100-location LRU | P3-M2, displayed P3-M5/P3-M6 |
| Favorites | P3-M2 records; fresh-profile hierarchy in product behavior | Ordered Group → Item tables and initialized marker | P3-M2 records; state P3-M5; Sidebar P3-M6 |
| Filter and workspace session | Application snapshot types | Reserved session tables; replaceable transaction later | P3-M5/P3-M9 |
| Operation recovery | Runtime safety journal | Separate from settings/session database writes | P3-M7/P3-M8 |

`NewFolder` is a stable canonical action with default `Command+Shift+N`. Its command, UI, and shortcut editor arrive later; its persisted default binding is preloaded now and an override remains inert until the action is delivered. Folder sort memory stores `Name`, `Type`, `Modified`, or `Size` with an explicit ascending or descending direction; an unrecorded folder uses natural Name ascending. Both Browsers and all tabs share one sort choice for the same byte-exact logical location.

The [Favorites policy](../product-behavior.md#favorites) defines the fresh-profile items and the launch-time removal rule. P3-M2 prepares typed, durable records and validation results; P3-M5 applies them to domain state, and P3-M6 connects that state to Sidebar startup and editing.

Out of scope: a Settings window, shortcut editor, new menu or toolbar controls, Sidebar editing controls, folder metadata columns, sort comparators beyond today's Name order, directory creation, tab restoration, and operation-recovery execution. P3-M3/P3-M4 settle visible interactions, P3-M5/P3-M6 add tab and Favorites behavior, and P3-M9 persists and restores workspace sessions.

## Completion checklist

- [x] The Phase 3 settings inventory documents the owner, compiled default, persistence rule, and delivery milestone for each settings or durable-state category, including future sort/filter state and session recovery.
- [x] Qt-free application values define validated settings, folder sort specifications, canonical UpperCamelCase action IDs, and default shortcut bindings. A new settings database preloads one binding for every catalogue action, including future inactive actions; invalid individual overrides fall back to their compiled defaults.
- [x] Explicit folder sort choices are shared by byte-exact logical location across Browsers and tabs, retained for 100 most recently used locations, and default to Name ascending after a miss or eviction.
- [x] Qt-free durable Favorites records round-trip stable group/item IDs, one-level hierarchy, order, names, and byte-exact locations without introducing the P3-M5 domain state or mixing Favorites into the replaceable workspace-session snapshot.
- [x] Typed Favorites load/save and target-validation results support the product's fresh-profile and launch-removal rules without probing paths or changing the Sidebar on the GUI thread.
- [x] The future workspace-session contract represents open tab order, active tab per Browser, current locations, tab-local filter state, and active Browser; it excludes back/forward history.
- [x] The application handles typed settings load, update, and save results, rejects stale results, applies in-memory changes immediately, and issues debounced, coalesced saves plus an orderly-quit save request without blocking the GUI thread.
- [x] A versioned SQLite driver stores settings, folder-sort records, and Favorites records under Application Support, automatically migrates supported older schemas, reserves a migration path for later session tables, and reports corruption, newer unsupported versions, and I/O failures without silently discarding the last good data.
- [x] A storage failure produces a recoverable, user-visible error and an explicit Reset Settings option. Reset never runs automatically: after confirmation it preserves the failed database for diagnosis, creates a fresh current schema with preloaded action bindings and fresh-profile Favorites, and reports the reset result.
- [x] Tests cover defaults and initial action-binding preload, action-ID stability, shortcut conflicts, byte-exact location round trips including non-UTF-8 names, ordered Favorites round trips, intentionally empty Favorites without reseeding, 100-location LRU behavior, transactional supported-schema migration, failed writes, explicit reset preservation, and responsive worker delivery using temporary directories.
- [x] Existing keyboard and sorting behavior is preserved; the full gate passes: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check`.

## Owner decisions

The owner approved retaining an explicit natural Name ascending selection as one of the 100 LRU entries, and approved `rusqlite` linked to the system SQLite library.
