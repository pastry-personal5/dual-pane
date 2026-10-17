# P4-M2: Model tab-local text filtering

Status: Done

This milestone adds the Qt-free state and projection for the M1-approved text filter. It supplies a safe command and view contract for the desktop control in [P4-M3](milestone-03-overview.md). The [architecture plan](milestone-02-architecture.md) defines the implementation boundary; this checklist is the definition of done.

## Scope

In scope: per-tab filter text, name matching, visible row mapping and revision, sort and refresh reconciliation, selection, cursor, range anchor and scroll policy, summary data, and state lifetime.

Out of scope: recursive or content search, indexing, filesystem changes, and the desktop filter control and native event delivery, which belong to P4-M3. M2 does not add a persisted filter field or a second directory read.

## Owner decisions

The owner accepted these M2 behaviors on 2026-10-16. The [M1 policy](milestone-01-overview.md#owner-decisions) still governs the filter's purpose, tab lifetime, and removal of hidden selection and cursor state.

| Topic | Accepted behavior |
|---|---|
| Unicode and accents | Match canonically equivalent Unicode text without regard to case or accents/diacritics. Use full Unicode case folding, including expansions such as `ß`/`ss`. |
| Query whitespace | Match leading and trailing spaces literally. Only an empty query shows all Items. |
| Names with invalid UTF-8 | Match the name as displayed, including replacement characters. Keep its exact original bytes as the Item identity for every action. Two names that display alike remain different Items. |
| Hidden range anchor and scroll | Clear an anchor hidden by the filter. If the top row disappears, scroll to the nearest surviving row in the previous sort order; keep the same row and pixel offset when it survives. Do not select a replacement row. |
| Summary | While filtered, show the visible match count out of the current folder total; selected count and selected/total size refer to visible matches. Distinguish zero matches from an empty folder. |
| History and failed reads | Any successful change to a different confirmed folder clears that tab's filter, including Back and Forward. A failed navigation, same-folder refresh, sort, or tab switch keeps it. Returning later through history does not restore an old query. |
| Pending projection | Clear the Folder Items List while a filter projection runs and show an updating cue. Keep the authoritative snapshot and existing visit state until the result arrives; the temporary blank list must not clear selection or cursor. Block Item and selection-dependent actions while pending. |
| Dependency | The owner approved adding MIT-licensed `caseless` 0.2.2 for full Unicode case folding during M2 implementation. No dependency was added during this plan review. |

## Completion checklist

- [x] The owner decides all [M2 behavior and dependency questions](#owner-decisions); the answers are recorded in [product behavior](../product-behavior.md), [UX GUI](../ux-gui.md), and this plan before feature code depends on them.
- [x] Each tab owns transient filter text and a projection of its current sorted snapshot; tabs in either Browser remain independent, and session restoration starts with an empty filter.
- [x] A filter edit changes no on-disk content and starts no directory read. Empty text restores the whole current listing in sort order; matching and no-match state follow the accepted Unicode and invalid-byte rules.
- [x] While projection work runs, visible rows clear with an updating state and Item actions blocked, but the authoritative snapshot and visit state remain intact. Only the accepted result reconciles selection and cursor; a failed projection stays blank with retryable feedback.
- [x] A visible-row revision accompanies row and selection-dependent gestures, with exact Item identity for row gestures. The application rejects an event from an earlier projection even if its row number is valid now; range selection and Select All use only visible rows, and operation capture contains only selected visible Items.
- [x] Selection, cursor, anchor, scroll, Preview Pane target, and summary data reconcile when filtering hides Items. Clearing the filter does not silently reselect Items that were removed from selection.
- [x] Successful different-folder navigation, Back, and Forward clear the filter atomically with the new listing; failed navigation, refresh, sort, tab switch, and current-folder watcher updates retain it. Visibility-setting changes cannot redisplay rows forbidden by the new policy.
- [x] Rapid edits, delayed projection or directory results, tab close/switch, and two tabs at one folder cannot publish an obsolete projection or route an action to a different Item. Projection work stays bounded under the [GUI responsiveness rule](../architecture.md#51-threading-and-responsiveness).
- [x] Qt-free tests cover the [scenario matrix](milestone-02-architecture.md#verification-scenarios), including non-UTF-8 names that display alike, Unicode matching, no matches, visible-only actions, and refresh/navigation races.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M2 Done.

## Acceptance evidence

The owner accepted the behavior and `caseless` 0.2.2 dependency choice above on 2026-10-16. The implementation keeps filter state in each tab, performs Unicode projection on bounded per-Browser worker lanes, and validates each completion against the exact authoritative listing before publishing it. Visible revisions and byte-exact row identities protect gestures and operation capture; tests cover matching, reconciliation, navigation and refresh races, tab lifetime, projection failure, and stale or malformed completions. On 2026-10-17, direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` passed.
