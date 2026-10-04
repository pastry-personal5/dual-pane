# P3-M11: Native Quick Look

Status: Active

This milestone adds macOS Quick Look to the Folder Pane without making Dual Pane a viewer. A Space tap previews the cursor item after key-up in the normal sized system panel; holding it for 512 ms opens it at the largest usable screen frame until release. [Architecture](milestone-11-architecture.md) describes the desktop-only approach; this checklist defines completion.

## Scope and fixed inputs

Use the [included product scope](../product-behavior.md#included-product-scope), [Quick Look interaction](../ux-gui.md#quick-look), [input and active-Browser state](../ux-gui.md#input-and-active-browser-state), [outer-ring ownership](../architecture.md#34-frameworks-and-drivers--replaceable-mechanics), and [P3-M8 opening boundary](milestone-08-architecture.md#application-contract-extensions). This milestone runs before P3-M10.

In scope: native macOS Quick Look for the current Folder Items cursor; files, folders, packages, and links; immediate normal sized panel presentation; a 512 ms held-Space resize to the largest usable screen frame; matching-key release and interruption dismissal; Status Bar feedback when the system panel cannot be acquired or presented; native panel lifecycle; an Objective-C++ adapter dynamically linked with `QuickLookUI.framework`; automated seam tests; a native human check; and the documentation changes in this plan.

Out of scope: a built-in viewer; multi-item previews; previewing a selection rather than the cursor; resolving a link before preview; Quick Look preference, saved geometry, history, or action binding; a context-menu entry; a workspace command, event, output, work request, projection, persistence, or settings change; automatic refresh; and any P3-M10 watcher work.

## Decisions recorded 2026-10-04

| Topic | Decision |
|---|---|
| Target | Plain Space acts only with Folder Items List focus and previews its cursor row. Selection does not change the target. |
| Eligibility | Every listed item kind is handed unchanged to macOS Quick Look. It decides whether and how the item is previewed. |
| Tap | The initial key press captures the cursor target only. A matching release before 512 ms presents the system panel at its normal size. |
| Hold | At 512 ms, a still-held Space presents the panel at the largest usable frame on its current screen. Its frame is set before macOS performs the panel's native opening animation. Releasing that press dismisses the panel. |
| Focus and interruption | Native Quick Look owns focus and controls while open. Switching to another application or unrelated window, or loss of panel control while held, dismisses it; the expected transfer from the workspace to its Quick Look Panel does not. |
| Failure | No cursor and modified or repeated Space are silent no-ops. Failure to acquire or present the panel briefly reports in the source Browser Status Bar. Unsupported content is system-owned Quick Look feedback. |
| Integration | Use Apple's dynamically linked `QuickLookUI.framework` through an Objective-C++ desktop adapter; no crate or third-party dependency is added. |

## Completion checklist

- [x] Product scope, GUI interaction, terms, Phase 3 ordering, index, and changelog record the P3-M11 decisions; Quick Look is no longer out of scope.
- [x] The desktop links `QuickLookUI.framework` dynamically and owns the shared Quick Look Panel only while the workspace responder chain selects its controller; no Quick Look type crosses the desktop boundary.
- [x] Plain, initial Space in a focused Folder Items List captures one cursor path. Its matching early release opens a single-item preview at the panel's normal size; timer expiry while still held opens it at the largest usable screen frame through macOS's native opening animation. Missing cursor, modified Space, repeated Space, and input outside the list do nothing; previewing changes no workspace state.
- [x] A deterministic 512 ms held-key lifecycle leaves the normal sized panel open after an early release, opens a held panel at the largest usable screen frame, dismisses it on matching release after the threshold, retains the normal panel when no usable frame is known, and dismisses safely on interruption.
- [x] Automated desktop seam tests cover target extraction, modifier and repeat filtering, deferred normal and enlarged presentation, early release, timer expiry, release, interruption, controller ownership, and Status Bar failure feedback without calling a real Quick Look service. File-system fixtures, if needed, stay in temporary directories.
- [ ] A human macOS check covers regular files, folders, packages, links, an unpreviewable item, normal-panel sizing and controls after a tap, held large-panel opening with the menu bar and Dock visible, held-release dismissal, and application-switch interruption.
- [x] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make check` pass; acceptance evidence is recorded here before the milestone is marked Done.

## Acceptance evidence

Automated evidence, 2026-10-04:

- `cargo test -q -p dual-pane-desktop quick_look_gesture` passed the fakeable desktop lifecycle seam. It verifies cursor-path delivery; no cursor, modifier, and repeat filtering; early-release retention; held enlargement; matching and stale release behavior; interruption; controller-acquisition refusal; and Status Bar failure feedback without a Quick Look provider.
- A native reproduction with a disposable 64×64 PNG showed a key and main workspace, a responder-chain controller, but `currentController == nil` after `updateController` on the hidden panel. After `makeKeyAndOrderFront:`, Quick Look called `beginPreviewPanelControl:` and set both the current controller and data source. The PNG rendered in the system panel; closing, Escape, and reopening worked. This establishes panel-acquisition timing, not item rejection, as the cause of the misleading Status Bar failure.
- The native handoff now uses byte-exact paths. An adapter regression test covers invalid-UTF-8 bytes in both a parent component and an image name, directory-kind delivery, and missing rows.
- Review follow-up: the gesture seam rejects stale release callbacks and exercises native release and interruption callbacks. The failure cue holds for its full duration while model status changes, then restores the latest status text.
- The C++ formatter and linter now include the Quick Look header and Objective-C++ unit; the earlier gate had omitted both, so its passing result did not validate those files.
- After the review follow-up, `make check` and direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed. The gate now includes formatting and clang-tidy for the new Objective-C++ unit and facade header.
- Revised gesture evidence, 2026-10-04: `quick_look_gesture` now proves that Space-down invokes no native presentation, early key-up invokes exactly one normal-size presentation, timer expiry invokes exactly one enlarged presentation, and stale callbacks cannot affect a newer held gesture. `make check` passed after the revision.

The remaining native scenarios, especially the tap opening, 512 ms held large-panel opening and dismissal, and app-switch interruption, still need a human check. The milestone remains Active until that check and the full gate are complete.
