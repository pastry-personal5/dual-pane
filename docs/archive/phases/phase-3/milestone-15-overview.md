# P3-M15: File and folder visibility

Status: Done

This milestone adds three persistent General Settings controls for macOS metadata/service items, dotfiles, and items marked invisible by macOS. Both Browsers and every open tab apply each choice immediately while file operations continue to act on the actual filesystem.

## Owner decisions

The owner approved the visibility rules on 2026-10-10 after review of the [research](../../../research/hidden-files-research.md), and confirmed the uncertain-classification behavior on 2026-10-12:

| Decision | Approved behavior |
|---|---|
| Controls | Three independent, global General Settings switches: **Show macOS metadata and service items**, **Show dotfiles**, and **Show items marked invisible by macOS**. These are proposed control labels; the visible description explains that an item matching multiple hidden rules needs each relevant switch enabled to appear. |
| Defaults | Metadata/service items hidden; other dotfiles shown; macOS-invisible items hidden. A fresh profile and Reset Settings use these same defaults. |
| Named metadata files | Exact `.DS_Store`, `Icon` plus carriage return (`Icon\r`), and `.localized` regular files; regular files whose raw names start `._`. |
| Named metadata directory | Exact `.AppleDouble` directory. A `.localized` directory remains visible unless another rule applies. |
| Volume-root service directories | Exact `.fseventsd`, `.Spotlight-V100`, `.Trashes`, `.TemporaryItems`, and `.DocumentRevisions-V100` directories, only when their parent is proven to be the current volume's root. |
| Dotfile rule | Any item kind with a leading `.` byte in its own name. It includes folders and links; `.` and `..` never enter Folder Items. |
| macOS invisible rule | The item's own platform invisibility metadata, regardless of name. A leading dot alone does not activate this rule; a link's target does not lend its status to the link. Native verification settles the exact API or combination needed to recognize supported macOS metadata. |
| Matching | Case-sensitive raw bytes and the stated kind/location constraints. Similar names are ordinary Items. The rules combine: every applicable switch must permit the row. |
| Uncertain classification | If a required native invisibility check or volume-root check cannot determine whether a candidate should be hidden, the complete listing fails with a visible error. Refresh can retry it; the uncertain item is never silently treated as visible. Checks unnecessary under the effective switches do not block a listing. |

The current-launch operation-temporary exclusion remains unconditional. Earlier-launch leftovers remain visible according to the three settings. The named set is versioned product policy: new names require a separate documented decision rather than a silent list expansion.

## Scope and sequencing

Use the [P3-M2 settings boundary](milestone-02-architecture.md#configuration-and-ownership), [P3-M14 General Settings page](milestone-14-overview.md), [automatic refresh](../../../ux-gui.md#automatic-refresh), [operation safeguards](../../../product-behavior.md#file-operation-safeguards), and the [architecture plan](milestone-15-architecture.md). P3-M13 and P3-M14 precede implementation. P3-M14 does not own these three controls or their database migration.

In scope: typed settings/defaults, SQLite migration and reset, three accessible controls with explanatory text, worker-side filtering, native macOS invisibility detection, active/inactive tab refresh, stale-result rejection, selection/cursor behavior, and verification.

Out of scope: automatic cleanup; user-defined patterns; per-folder or per-tab overrides; changing Finder preferences; changing recursive operations, conflict detection, or deletion safeguards; and adding another dependency without approval.

## Completion checklist

- [x] A temporary-directory macOS probe records how `NSURLIsHiddenKey`, `UF_HIDDEN`, Finder information, leading-dot names, links, and non-UTF-8 paths behave; it proves that a plain dotfile remains visible when only the macOS-invisible switch is off. The architecture chooses a byte-exact worker-side implementation and records its supported metadata sources, error behavior, and support limit.
- [x] Qt-free application settings and snapshots own three independent Boolean values with the approved defaults. Commands accept healthy loaded settings, ignore unchanged values, update effective state immediately, and request a coalesced save.
- [x] A transactional SQLite migration loads older profiles with the new defaults, round-trips all eight switch combinations, validates stored values, preserves existing settings and sessions, and resets the switches to the approved defaults only on explicit Reset Settings.
- [x] General Settings shows three separately labeled, keyboard accessible controls and concise help for overlapping rules. The controls reflect effective state, apply immediately, persist across relaunch, and obey existing settings-health dismissal and failure behavior.
- [x] Each `ReadDirectory` request carries an immutable visibility policy. The worker matches only the approved raw names, kinds, root placement, dot prefix, and native invisible status; it always suppresses current-launch operation temporaries. A required classification failure has its own retryable listing error and never proves that a saved tab's folder is unrestorable.
- [x] Changing any switch invalidates all open tabs in both Browsers. When a change hides any class of item, old rows leave interaction and display immediately; a reveal-only change may retain the current rows while loading. Failed or saturated rereads never restore rows from a broader policy. Rapid toggles, watch invalidations, in-flight reads, and a late settings load after the startup timeout converge on the latest policy without replacing visible tabs or defeating session write protection.
- [x] A successful refresh restores only matching selection, cursor, and scroll state. A hidden row cannot remain selected, be opened, enter Quick Look, or become an operation source while a stricter-policy read is pending or after it fails. Explicit navigation, Favorites, and saved tabs can still reach a hidden folder by path. Visible counts and totals use visible rows.
- [x] File-operation source scans include all on-disk descendants of a selected folder, hidden destination collisions still reach the existing conflict workflow, and showing or hiding a row never deletes or edits its object. Operation progress and error handling remain truthful when hidden children are involved.
- [x] Focused tests use temporary directories for exact names and kinds, dotfiles and links, non-UTF-8 names, all switch combinations, stale and failed reads, reveal-only and stricter transitions, saturated admission and eventual retry, selection/cursor reconciliation, saved-tab retention after classification failure, operation conflicts, persistence, reset, and Settings accessibility. An injected mount-root classifier tests service names both at and below a simulated volume root, plus uncertain classification.
- [x] Native macOS verification in disposable locations covers the three defaults, independent live toggles in both Browsers, an actual macOS-invisible item, relaunch persistence, and a visible `.git`/`.env` with default settings. Injected mount-root tests cover volume service names without touching a real volume's system folders.
- [x] Affected product/UX/README documentation and acceptance evidence are updated. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass before P3-M15 is marked Done.

Acceptance evidence: the [native probe record](../../../research/hidden-files-research.md#native-verification-on-2026-10-12), focused Rust application/desktop tests, `scripts/check-settings-window.sh`, and the full `make check` plus direct Rust commands passed on 2026-10-12. A disposable native app profile showed the three defaults, `.git` and `.env` in both Browsers, `.DS_Store` and a `UF_HIDDEN` file omitted, each independent toggle updating both Browsers, and the chosen settings and Browser paths surviving a confirmed quit and relaunch. The disposable profile and app wrapper were removed after verification.
