# P4-M5: Deliver clipboard commands

Status: Active

This milestone exposes Copy, Cut, and Paste through the desktop interface and the approved macOS clipboard behavior. The [architecture plan](milestone-05-architecture.md) defines native delivery; this checklist is the definition of done.

## Scope

In scope: context-menu and keyboard entry points, private clipboard writing and observation, Finder-compatible file Copy publishing and reading, Paste availability and feedback, owner-approved Cut cue, shortcut-editor integration, accessibility, and native verification.

Out of scope: replacing the existing direct other-Browser commands, new file-operation conflict policy, importing non-file clipboard content, exporting Cut as a Finder move, and external drag and drop.

## Owner decisions

On 2026-10-18, the owner accepted the following behavior for M5.

| Topic | Accepted behavior |
|---|---|
| Command placement | Copy Items, Cut Items, and Paste Items appear only in the Folder Items Context Menu. A selected-row menu starts with those three commands, then the existing direct other-Browser and file commands. An empty-space menu offers Paste Items, a separator, then New Folder. Paste always targets the active tab's confirmed folder, never a right-clicked folder row. |
| Pending Cut cue | Captured Cut roots have muted icon and text while selection and focus styling remain intact, and accessible row text includes “pending cut.” Matching source views show a low-priority persistent count such as “3 items ready to move”; loading, errors, and transient feedback take precedence. |
| Cue reach and lifetime | Every visible tab whose byte-exact row path and kind match a captured Cut root shows the cue. Clipboard replacement, a later Copy or Cut, or terminal consumption of a started Cut clears it. |
| Feedback | Successful Copy and Cut preparation and successful Paste preparation receive brief acknowledgement. Clipboard replacement is silent. An attempted unavailable Paste reports its specific reason for about three seconds. |
| Changed sources | Paste keeps the frozen byte-exact source paths and captured destination. A missing or renamed root, or one whose kind changed, is rejected without substitution. Supported content changed in place at the same path and kind is operated on as it exists when scanned. |
| Stale private data | A valid current private token wins over colocated file URLs. After relaunch or for a stale or unknown private token, valid colocated file URLs may use the safeguarded external Copy path. Cut cannot fall through because it publishes no file URLs. |
| Finder acceptance | Finder automation is opt-in, uses only a disposable profile and temporary folders, and warns that it replaces and cannot reliably restore the global clipboard. It remains outside the normal gate. |

## Completion checklist

- [x] Before implementation, the owner decides [P4-M5 menus, Cut cue, and clipboard feedback](milestone-01-overview.md#deferred-owner-decisions), and the accepted behavior is recorded in [UX GUI](../ux-gui.md).
- [ ] Copy, Cut, and Paste use the M1-approved shortcut scopes and defaults in the M5-approved menus, with stable action IDs that do not collide with text editing or reserved macOS shortcuts.
- [ ] Copy publishes both Dual Pane's private transfer intent and a Finder-compatible representation of all exact selected Items, including files, folders, packages, symbolic links, and multiple Items. The Finder representation remains usable after Dual Pane quits or relaunches until the clipboard is replaced; Cut publishes only the private intent. Paste into Dual Pane accepts a valid private intent or Finder-copied file references. The private intent expires on relaunch; Finder-sourced Paste is a Copy and remains eligible while its clipboard content is unchanged. Unsupported, malformed, stale, or non-file contents cannot start a Dual Pane operation and receive M5-approved feedback.
- [x] Availability and Cut feedback track clipboard ownership, selection, active tab, and destination changes without implying a move completed before Paste succeeds.
- [ ] Clipboard and inline-editor keyboard focus are correctly separated; copy or paste of text in a focused editor retains ordinary text-editing behavior.
- [ ] Native macOS checks use temporary source and destination folders to Copy one and multiple files, folders, packages, and symbolic links in Dual Pane and Paste them into Finder before and after Dual Pane quits or relaunches, until clipboard replacement; verify the resulting Item types, contents, and unchanged sources. Copy the same Item types in Finder and Paste them into either Dual Pane Browser, including after Dual Pane relaunches; verify safe Copy results and unchanged Finder sources. Also cover Copy, Cut, Paste within and across Browsers, private-intent expiry on relaunch, clipboard replacement by another app, rejection of malformed or non-file content, conflict decision, and cancellation in a disposable profile; record results here.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M5 Done.

## Acceptance evidence

Recorded 2026-10-18. Items marked open need native macOS interaction that automated checks cannot establish; they await the owner's decision on how to verify them.

**Checked: availability and Cut feedback.** `crates/dual-pane-application/tests/clipboard_transfer.rs` covers an early observation of Dual Pane's own write (`observing_our_own_write_before_the_lane_reports_it_keeps_the_publication`), a publication older than an observed replacement (`a_publication_older_than_an_observed_replacement_never_becomes_current`), cue lifetime through a started Cut (`a_started_cut_keeps_its_cue_until_the_operation_reaches_a_terminal_result`), replacement clearing (`pending_cut_projection_is_published_and_cleared_by_replacement`), and stale-token fall-through (`unknown_private_token_falls_back_to_colocated_file_urls_but_private_only_rejects`). `crates/dual-pane-adapters/tests/tabbed_view_models.rs` covers exact path-and-kind matching in both Browsers, accessible “pending cut” text, status priority, and in-place repaint without a model reset. `a_late_failure_older_than_the_observed_generation_changes_nothing` keeps the observed generation from moving backwards. `browser_session::tests::clipboard_menu_availability_tracks_selection_destination_and_the_observed_offer` covers menu enablement.

**Checked: gate.** `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` (449 passed, 0 failed) pass directly, and `make check` passes, as does `scripts/check-settings-window.sh`.

**Open: shortcuts and menus.** Automated: action IDs, default shortcuts, Folder Items List scope, append-only migration that leaves conflicting, customized, and unbound profiles alone (`settings.rs` tests), and the shortcut editor's File Actions grouping (`scripts/check-settings-window.sh`). Not automated: the menu order and enablement in the running Folder Items Context Menu, and the keys firing in the app.

**Open: Copy, Cut, and Paste representations.** Automated against an isolated unique pasteboard (`clipboard_native::tests`): one file-URL item per root in clipboard order, a nine-byte private token with no path, Cut publishing no file URL, byte-exact round trips for invalid UTF-8 and precomposed and decomposed names, Finder-style URL reading, a system-written file reference URL resolved to its path and a dangling one refused, malformed, non-file, mixed, and wrong-version content rejected as a whole, and generations on failure. Not automated: that Finder itself accepts the representation, including after Dual Pane quits, and the exact URL forms a real Finder Copy writes.

**Open: focus separation.** Folder Items shortcuts are `Qt::WidgetShortcut` on the focused list and are disabled while a name editor is open, so the filter row, path editor, and rename editor keep ordinary text editing. The live check in the running app remains.

**Open: native macOS checks.** `scripts/clipboard-acceptance.sh` is the opt-in guided script for the scenarios in the checklist. Its `--self-test` passes; its scenarios have not been run, because they replace the global clipboard and need the owner's gestures in Dual Pane and Finder.
