# P3-M2 architecture

Status: Done

P3-M2 gives Phase 3 one application-owned model for durable choices and one serialized settings service for storage. The domain and application use typed values; the desktop driver owns SQLite, the Application Support path, native errors, and I/O scheduling.

Supersession note: P3-M4 removed text filtering from Phase 3 behavior and restored session state. Filter references below preserve the historical P3-M2 contract and completed acceptance evidence; [current product scope](../product-behavior.md#included-product-scope) and the [P3-M4 handoff](milestone-04-architecture.md#contract-to-later-milestones) govern later implementation. Existing persisted records and action IDs remain untouched in this documentation milestone; any compatibility migration belongs to later implementation.

## Configuration and ownership

Defaults are compiled application values and the compatibility baseline for validation and recovery. A newly created database preloads a binding row for every catalogue action from that baseline, including actions whose commands are not delivered yet. Thereafter effective shortcut settings resolve from a validated saved binding, then their compiled default if an individual row is invalid or unavailable. Fixed file-operation safeguards remain product policy; executor capacity and native paths remain runtime configuration. Only a choice the product intends people to change becomes a user setting.

Every product-facing value at this boundary is Qt-free: settings, defaults, action IDs, normalized shortcuts, sort specifications, Favorites persistence records, workspace snapshots, and typed settings commands/results. The `SettingsGateway` contract is likewise Qt-free. The SQLite driver may live in the outer desktop crate because it owns storage and Application Support discovery, but it uses no Qt type or event loop; Qt only forwards application commands and renders resulting state. This keeps the persistence driver independently testable with temporary paths and prevents widget state from becoming configuration state.

| Data | Typed owner | Durable owner | Delivery |
|---|---|---|---|
| Shortcut overrides | Application action catalogue and settings state | Settings database | P3-M2 records and validates; a later shortcut editor exposes changes. |
| Per-folder sort specification | Domain `SortSpec`; application location-indexed memory | Settings database | P3-M2 remembers and stores; P3-M5/P3-M6 sort and display it. |
| Favorite Groups and Favorite Items | P3-M2 persistence records; P3-M5 domain hierarchy and application Favorites state | Ordered settings database tables | P3-M2 defines and tests storage; P3-M5 adds editing rules; P3-M6 delivers the Sidebar. |
| Global sort default | Application default | Settings database only if made user-configurable | P3-M4 decided the interaction; text filtering is outside Phase 3. |
| Open tab order, active tab/Browser, current locations, and window layout | Application workspace snapshot | Settings database session tables | P3-M9 saves and restores; back/forward history stays in memory for the current session. See the [P3-M9 plan](milestone-09-overview.md). |
| Operation recovery records | Runtime safety journal | Separate runtime-owned durable record | File-operation milestones; never mixed with settings transactions. P3-M8 stores it in its own SQLite file, which Reset Settings never touches. |

## Folder sort memory

`SortSpec { field, direction }` uses `Name`, `Type`, `Modified`, and `Size`, each with ascending and descending directions. The default is natural Name ascending with folders first. P3-M2 defines and persists the value without introducing the new comparators or UI. The current natural Name comparator remains the reference for Name ascending.

Application state holds a map of explicit sort choices keyed by `Location` and an LRU order. A logical location is its exact ordered byte components: the two Browsers and any tabs at the same location see one choice, while a symlink alias and its target path can have distinct choices. Revisiting a recorded folder or changing its sort refreshes its recency; choosing natural Name ascending remains an explicit record and uses an LRU slot. On the 101st recorded location, the oldest record is evicted; revisiting it uses Name ascending. The driver encodes path components losslessly into a SQLite BLOB primary key, never through a lossy display string.

P3-M5 passes the effective `SortSpec` with each tokenized directory-read request and rejects results for an obsolete specification. The later listing implementation collects non-recursive size and modification metadata on workers, uses a link's own metadata, and puts unknown metadata last. The owner-approved field order, sort controls, display formats, and narrow-Browser behavior are [planned in the GUI specification](../ux-gui.md#planned-phase-3-gui-decisions).

## Actions and shortcut settings

Persisted `ActionId` values are stable ASCII UpperCamelCase compatibility keys. The action catalogue owns each ID, availability, default shortcut, and editor eligibility. Every catalogue action is seeded as a persistent binding on first database creation and when a migration adds that action. `NewFolder` defaults to `Command+Shift+N` but is unavailable until directory creation exists. Existing `FocusOtherBrowser`, `NavigateParent`, `CloseWindow`, and `QuitApplication` actions retain their current bindings. The eight `SortBy{Name|Type|Date|Size}{Ascending|Descending}` actions have no default keyboard shortcut.

P3-M6 [supersedes the `CloseWindow` default binding](milestone-06-architecture.md#widgets-focus-and-visible-state) when it delivers `CloseTab`: migrate the workspace `Command+W` binding to the new action, retain the `CloseWindow` ID and meaning, and preserve other valid nonconflicting overrides.

Superseded in part on 2026-10-02 by [schema version 2](changelog.md#2026-10-02--pre-m6-review-fixes): normalized keys can name arrow, Return, Delete, and function keys, so the stored `NavigateParent` default is its current `Command+Up` binding; a stored NULL key means a deliberately unbound action, and unknown key text falls back to the default for that action only.

Saved shortcuts are application-owned normalized modifier/key values rather than Qt or localized strings. An `ActionId` is never localized, renamed, or reused for a different meaning; UI labels are separate presentation data. Loading validates each override independently, rejects unknown IDs, malformed or platform-reserved shortcuts, and collisions among actions active in the same context. A binding for an action that has no delivered command remains inert until that action is delivered. The [planned editor behavior](../ux-gui.md#future-shortcut-editor) uses these values and dispatches through the existing command boundary.

## Favorites hierarchy

Favorites are durable user data, separate from the replaceable workspace-session snapshot. P3-M2 defines storage records for one ordered level of Favorite Groups, each containing ordered Favorite Items. Groups and items have stable IDs; each item has a display name and a byte-exact `Location`. P3-M5 introduces the domain hierarchy, application state, and edit commands; P3-M6 owns Sidebar controls. P3-M2 does not pre-empt either milestone by introducing a second Favorites state model.

The SQLite design uses `favorite_group(id, name, position)` and `favorite_item(id, group_id, name, target BLOB, position)`. An index on each position scope makes ordered loading and reordering efficient. Enable `PRAGMA foreign_keys = ON` for every connection; the item-to-group foreign key restricts accidental group deletion until the application explicitly handles its items. Apply a group/item edit in one transaction so readers see either the old or new hierarchy. A durable `favorites_initialized` marker distinguishes a fresh profile from a person who intentionally deleted every favorite; an empty saved collection is never reseeded. SQLite's [foreign-key guidance](https://www.sqlite.org/foreignkeys.html) recommends an index on child keys, and its [transaction model](https://www.sqlite.org/lang_transaction.html) supports these atomic edits.

The [product behavior](../product-behavior.md#favorites) owns the exact initial items, target locations, and unavailable-target policy. P3-M2 defines typed load/save records and target-probe outcomes. P3-M6 resolves initial locations and probes saved targets on workers during startup, then submits each completed result to the application. Confirmed removals commit transactionally; a failed save is reported rather than silently treated as persisted. No Favorite Item is removed because the database loader or validation worker itself failed.

## Workspace-session handoff

P3-M9 persists and snapshots both Browsers' open tabs in display order, active tab per Browser, each tab's current location, the active Browser, and the window layout. Text filtering was removed from the Phase 3 contract, so no filter state is stored. It resolves a tab's sort from the location-keyed folder-sort memory. Back/forward history remains available while the app runs but is not serialized or restored. All session rows are replaced in one coalesced transaction, so a failed write leaves the previous committed snapshot intact. If a saved current location cannot be restored, discard that tab and restore the others; if a Browser has no valid saved tabs, create a clean Home tab to preserve the at-least-one-tab invariant. The Favorites tables are never cleared by a session replacement.

## Storage and lifecycle

Use a SQLite database in [Application Support](https://developer.apple.com/documentation/foundation/url/applicationsupportdirectory), with schema versioning through `PRAGMA user_version`, transactional migrations, a folder-sort table keyed by BLOB location, ordered Favorites tables, and an index supporting LRU trimming. SQLite is the sole live authoritative configuration store; no app-managed text configuration file is read or watched. The database has one ordered owner on the settings worker; the GUI thread reads effective values from application memory and never waits for SQLite. The driver uses transactions for each accepted save batch and returns typed load/save outcomes. A coalescing timer batches rapid replaceable updates; durable Favorites changes are submitted promptly as user-data changes. Orderly quit requests the latest state without waiting in the Qt event loop. The database evolves with session tables in P3-M9.

Opening an older supported schema runs its migrations transactionally and automatically. A corrupt database, newer unsupported schema, or failed write emits a typed recoverable error and offers Reset Settings; it never resets automatically. After explicit confirmation, reset preserves the unusable database under a diagnostic backup name, creates a fresh current-schema database, seeds the action bindings and fresh-profile Favorites, and reports success or failure. A failed preservation or fresh-database creation leaves the error visible and does not claim that settings were reset.

The storage format and table layout are driver details. A future schema version is reported as unsupported rather than interpreted as an older version. A corrupt database is preserved for diagnosis; the app continues with compiled defaults in memory and reports a recoverable settings error. A failed save leaves the last committed database intact and the effective in-memory choices usable. No recovery path silently resumes a file operation. Database tests use injected temporary paths; production path lookup is confined to the desktop driver.

[`rusqlite`](https://docs.rs/rusqlite/latest/rusqlite/) provides typed SQLite access; link it to the system SQLite available on supported macOS rather than adding a bundled SQLite build. The dependency and license review happens before implementation, as required by AGENTS.md. SQLite transactions and [WAL mode](https://www.sqlite.org/wal.html) support the selected asynchronous, coalesced persistence strategy.

## Sequencing

1. Inventory Phase 3 defaults and durable state, then define Qt-free setting, sort, action, and error values.
2. Add application commands/events and a non-blocking settings-service lane with ordered, coalesced saves.
3. Add and test the versioned SQLite driver with temporary paths and byte-exact location keys.
4. Wire startup load and existing actions without changing current keyboard or Name-sort behavior.
5. Record acceptance evidence and run the full gate; carry sort UI and workspace-session consumers into their assigned milestones.
