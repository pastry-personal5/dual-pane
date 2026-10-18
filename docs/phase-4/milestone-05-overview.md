# P4-M5: Deliver clipboard commands

Status: Done

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
| Cue reach and lifetime | Every visible tab whose byte-exact row path and kind match a captured Cut root shows the cue. Clipboard replacement, a later Copy or Cut, or terminal consumption of a started Cut clears it. Clarified by the owner on 2026-10-18: each trigger applies even while the Cut's Paste still runs, so the cue shows only the Cut the clipboard holds now. |
| Feedback | Successful Copy and Cut preparation and successful Paste preparation receive brief acknowledgement. Clipboard replacement is silent. An attempted unavailable Paste reports its specific reason for about three seconds. |
| Changed sources | Paste keeps the frozen byte-exact source paths and captured destination. A missing or renamed root, or one whose kind changed, is rejected without substitution. Supported content changed in place at the same path and kind is operated on as it exists when scanned. |
| Stale private data | A valid current private token wins over colocated file URLs. After relaunch or for a stale or unknown private token, valid colocated file URLs may use the safeguarded external Copy path. Cut cannot fall through because it publishes no file URLs. |
| Finder acceptance | Finder automation is opt-in, uses only a disposable profile and temporary folders, and warns that it replaces and cannot reliably restore the global clipboard. It remains outside the normal gate. |

## Completion checklist

- [x] Before implementation, the owner decides [P4-M5 menus, Cut cue, and clipboard feedback](milestone-01-overview.md#deferred-owner-decisions), and the accepted behavior is recorded in [UX GUI](../ux-gui.md).
- [x] Copy, Cut, and Paste use the M1-approved shortcut scopes and defaults in the M5-approved menus, with stable action IDs that do not collide with text editing or reserved macOS shortcuts.
- [x] Copy publishes both Dual Pane's private transfer intent and a Finder-compatible representation of all exact selected Items, including files, folders, packages, symbolic links, and multiple Items. The Finder representation remains usable after Dual Pane quits or relaunches until the clipboard is replaced; Cut publishes only the private intent. Paste into Dual Pane accepts a valid private intent or Finder-copied file references. The private intent expires on relaunch; Finder-sourced Paste is a Copy and remains eligible while its clipboard content is unchanged. Unsupported, malformed, stale, or non-file contents cannot start a Dual Pane operation and receive M5-approved feedback.
- [x] Availability and Cut feedback track clipboard ownership, selection, active tab, and destination changes without implying a move completed before Paste succeeds.
- [x] Clipboard and inline-editor keyboard focus are correctly separated; copy or paste of text in a focused editor retains ordinary text-editing behavior.
- [x] Native macOS checks use temporary source and destination folders to Copy one and multiple files, folders, packages, and symbolic links in Dual Pane and Paste them into Finder before and after Dual Pane quits or relaunches, until clipboard replacement; verify the resulting Item types, contents, and unchanged sources. Copy the same Item types in Finder and Paste them into either Dual Pane Browser, including after Dual Pane relaunches; verify safe Copy results and unchanged Finder sources. Also cover Copy, Cut, Paste within and across Browsers, private-intent expiry on relaunch, clipboard replacement by another app, rejection of malformed or non-file content, conflict decision, and cancellation in a disposable profile; record results here.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, plus `make check`, pass. Record evidence here and update affected docs before marking P4-M5 Done.

## Acceptance evidence

Recorded 2026-10-18. On 2026-10-18 the owner removed the human-verification gate: the items below that were still open at that point were checked without further manual runs, and the remaining native scenarios stay available in the opt-in `scripts/clipboard-acceptance.sh`.

**Checked: availability and Cut feedback.** `crates/dual-pane-application/tests/clipboard_transfer.rs` covers an early observation of Dual Pane's own write (`observing_our_own_write_before_the_lane_reports_it_keeps_the_publication`), a publication older than an observed replacement (`a_publication_older_than_an_observed_replacement_never_becomes_current`), cue lifetime through a started Cut (`a_started_cut_keeps_its_cue_until_the_operation_reaches_a_terminal_result`), replacement clearing (`pending_cut_projection_is_published_and_cleared_by_replacement`), the owner's 2026-10-18 clarification that replacement, a newer Copy, or a newer Cut clears a running Cut's cue (`replacing_the_clipboard_clears_a_running_cuts_cue`, `a_newer_copy_clears_a_running_cuts_cue`, `a_newer_cut_alone_holds_the_cue_while_an_earlier_cut_runs`), stale-token fall-through (`unknown_private_token_falls_back_to_colocated_file_urls_but_private_only_rejects`), Paste disabled for a private-only offer that is not this launch's current, unreserved transfer (`a_private_only_offer_enables_paste_only_for_this_launchs_unreserved_current_transfer`), and a read that finds an unreported replacement (`a_read_that_finds_an_unreported_replacement_adopts_it_so_a_retry_succeeds`). `crates/dual-pane-adapters/tests/tabbed_view_models.rs` covers exact path-and-kind matching in both Browsers, accessible “pending cut” text, status priority, and in-place repaint without a model reset. `a_late_failure_older_than_the_observed_generation_changes_nothing` keeps the observed generation from moving backwards. `browser_session::tests::clipboard_menu_availability_tracks_selection_destination_and_the_observed_offer` covers menu enablement.

**Checked: gate.** Rerun 2026-10-18 after the review fixes: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` (475 passed, 0 failed, 1 ignored) pass directly, and `make check` passes, as does `scripts/check-settings-window.sh`.

**Checked (owner waived the remaining manual check): shortcuts and menus.** Automated: action IDs, default shortcuts, Folder Items List scope, append-only migration that leaves conflicting, customized, and unbound profiles alone (`settings.rs` tests), the shortcut editor's File Actions grouping (`scripts/check-settings-window.sh`), Qt key sequences that name Command as `Ctrl+` (`delivered_shortcuts_map_to_qt_key_sequences`), and shortcut capture that records Qt's Control as Command and Meta as Control, matching those sequences (`captured_qt_modifiers_become_the_keys_a_person_held`). Native: Command+C, Command+X, and Command+V fired in both Browsers throughout the owner's acceptance run below. Not yet established: the menu order and enablement in the running Folder Items Context Menu; the `context-menus` scenario of `scripts/clipboard-acceptance.sh` covers them.

**Checked: Copy, Cut, and Paste representations.** Automated against an isolated unique pasteboard (`clipboard_native::tests`): one file-URL item per root in clipboard order, a nine-byte private token with no path, Cut publishing no file URL, byte-exact round trips for invalid UTF-8 and precomposed and decomposed names, Finder-style URL reading, a system-written file reference URL resolved to its path and a dangling one refused, malformed, non-file, mixed, and wrong-version content rejected as a whole, generations on failure, and a read that kept changing reported as busy. Feedback: `an_unavailable_paste_reports_its_specific_reason_briefly_in_the_pasting_browser` (`crates/dual-pane-adapters/tests/tabbed_view_models.rs`) shows the specific reason for unsupported content and for a stale private transfer, in the pasting Browser only, until its status token expires. Native: in the owner's run recorded under native macOS checks, Finder accepted Dual Pane Copy, including after Dual Pane quit and relaunched, Dual Pane accepted Finder Copy, including after relaunch, an expired Cut moved nothing, and plain text created nothing.

**Checked (owner waived the remaining manual check): focus separation.** Folder Items shortcuts are `Qt::WidgetShortcut` on the focused list, and every workspace shortcut except Quit is disabled while the filter row or a rename editor has focus, so those editors keep ordinary text editing. The path editor arrives in P4-M6. The live check remains; the `text-editor-focus` scenario of `scripts/clipboard-acceptance.sh` covers the filter row and the rename editor.

**Checked (owner waived the remaining manual runs): native macOS checks.** Run 2026-10-18 by the owner with `scripts/clipboard-acceptance.sh` (debug build, disposable profile, temporary folders; fixtures: a plain file, a precomposed-name file, a folder with nested files, a `.app` package, a valid and a dangling symbolic link). Every check passed on the first run except one, which passed when rerun alone:

- Copy in Dual Pane, Paste in Finder: one file, then every fixture type (PASS); the same after Dual Pane quit and after relaunch (PASS); Dual Pane sources unchanged (PASS).
- Copy in Finder, Paste in the Right Browser: one file, then every fixture type (PASS); the same after Dual Pane relaunched (PASS); Finder sources unchanged (PASS).
- Copy across Browsers (PASS); Cut and Paste across Browsers moved the file and removed the source (PASS); a same-folder Paste created nothing (PASS).
- A conflict answered with Cancel left the destination item and sources unchanged (PASS).
- An expired Cut after relaunch moved nothing and left the source (PASS); a Copy survived relaunch as a safeguarded external Copy (PASS).
- Cut published no file URL and left its sources in place (PASS); pasting plain-text clipboard content created nothing (PASS).
- Clipboard replacement by another application: the first run recorded FAIL (`plain.txt` appeared in the destination). The cause was not identified; the Paste path, the pasteboard adapter, and the unit tests handle a replacement correctly. The dedicated `scripts/clipboard-replacement-check.sh` then passed, with the file URL gone after the replacement and nothing pasted.

The owner closed this item without further runs. These two things were not established by the recorded runs, and the owner accepted that:

- Cancellation of a running Paste. Only the conflict-decision Cancel ran. The new `cancel-running-paste` scenario cancels a Paste of many small files and verifies that no partial artifact remains, that completed items are whole, and that the sources are unchanged.
- An explained replacement result. The dedicated script ran on its own, not in the original sequence, where `replacement` followed `private-expiry`. The scenario now records the run as invalid when the Paste happened before the replacement or when the Copy left no file URL to replace. Rerun `scripts/clipboard-acceptance.sh private-expiry replacement` in that order.

Also not established: the wording of each refusal message in the running app. The scenarios no longer need folders to be reopened between scenarios. The temporary folder is the profile's Home, Dual Pane launches with the Left Browser at the source, and staging empties the folders in place.
