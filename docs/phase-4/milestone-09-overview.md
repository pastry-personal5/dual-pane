# P4-M9: Single-instance command-line launch

Status: Done

This milestone makes `dual-pane <folder>` open the folder in the instance that is already running, activating the tab that shows it when there is one, and bringing that window forward. It was completed independently ahead of the remaining Phase 4 milestones. The [architecture plan](milestone-09-architecture.md) defines the forwarding channel, the journal safety change it requires, and the application changes; this checklist is the definition of done.

## Scope

In scope:

- Forwarding a command-line launch to a running instance of the same profile, and exiting the launcher.
- Finding an existing tab on the folder in either Browser and activating and focusing it, else opening a tab.
- Bringing the running window to the front, and a plain `dual-pane` that only does that.
- The `--new-instance` option.
- Making the startup sweep of operation leftovers safe while more than one instance runs.
- Launch races, stale sockets, a hung or quitting instance, and per-profile isolation for disposable test profiles.

Out of scope:

- Opening several paths in one launch (the one-path limit stays).
- Opening files with Dual Pane from Finder or `open`, URL schemes, and a document type or `QFileOpenEvent` handler.
- Sharing one session between simultaneous instances. A `--new-instance` instance is independent, and its saved session follows last save wins (see [architecture](milestone-09-architecture.md#new-instance-and-shared-profile-data)).
- Changing how the launcher resolves paths (`fs::canonicalize`, following links to a physical path).
- A remote or cross-user channel.

## Owner decisions

The 2026-10-19 interview settled rows 1 to 4. Plan review proposed rows 5 to 7, and the owner accepted them as recommended the same day.

| # | Topic | Decision |
|---|---|---|
| 1 | Which Browser | The running instance searches both Browsers for a tab on the folder, active Browser first. A match becomes the active tab of its Browser, that Browser becomes active, and its Folder Items List takes focus. With no match, a new tab opens in the active Browser; at the tab limit, that Browser's active tab navigates there, as today. |
| 2 | Channel | A Unix domain socket and a lock file, using the Rust standard library only. No new crate, build tool, or Qt module. |
| 3 | Plain `dual-pane` | With a running instance, it brings the window forward, restoring it if minimized, and changes no tabs. With none, it starts normally. |
| 4 | `--new-instance` | An explicit option that starts an independent instance. It neither forwards nor registers as the receiver, and it may be combined with a path. |
| 5 | Cold launch | The first instance applies the same rule as row 1 when it opens its command-line target, instead of today's Left-Browser-only rule. One rule for both paths is simpler to explain. Rejected alternative: keep Left Browser for a cold launch. |
| 6 | Shared startup sweep | Each instance holds a launch-liveness lock, and a startup sweep removes leftovers only of launches whose lock is free. Rejected alternatives: only the registered instance sweeps, or `--new-instance` requires a separate profile. See [Journal safety](milestone-09-architecture.md#journal-safety). |
| 7 | Window raise fallback | If Qt alone cannot bring a background window forward, the owner has approved the Objective-C++ `NSApp activate` fallback, which changes the build. It is used only after a native check shows Qt alone fails. |

## Completion checklist

- [x] The owner has answered rows 5 to 7, and the accepted rows are recorded in the [changelog](changelog.md) and as a planned note in [product behavior](../product-behavior.md#command-line-launch), all dated 2026-10-19. UX GUI has no command-line launch section to change.
- [x] **Journal safety first.** Every instance, including `--new-instance`, acquires its per-launch liveness lock before its journal can record or sweep and holds it until operation workers stop. A sweep groups records by launch and never removes a live launch's records or temporaries. Tests show that a second launch started during a running operation leaves that operation's journal records and `.partial` temporaries untouched, that a crashed lock-aware launch's leftovers are swept, and that missing, malformed, unreadable, or locked liveness files fail closed.
- [x] Listings and operation scans hide the exact reserved temporary-name shape for every launch, not only the current launch, so one instance cannot expose or copy another live instance's `.partial` file. Near-match names remain ordinary user files.
- [x] The launcher resolves the path against its own working directory as today, reports argument and path errors to standard error with exit status 2 before contacting anything, and sends the resolved path as raw bytes, including a name that is not valid UTF-8.
- [x] With no instance running, the launcher becomes the receiver: it takes the rendezvous lock, binds an owner-only socket before Qt starts, and removes the socket before releasing that lock on a normal quit. A stale socket and rendezvous lock left by a crash are replaced without deleting an endpoint owned by a live process.
- [x] With an instance running, the launcher sends one bounded, versioned request, waits until the GUI/application owner accepts it, and exits with status 0. It starts no window and no second instance.
- [x] A forwarded folder that a Browser shows in a tab activates and focuses that tab. The Browser is made active, and a tab the other Browser shows is used only when the active Browser has none. A forwarded file selects the file as today, in the tab it lands on.
- [x] A forwarded folder that no tab shows opens a tab in the active Browser, or navigates the active tab at the tab limit.
- [x] A forwarded request that arrives before the first instance finishes starting waits for the session restore and applies once. If several arrive before that, the last one wins.
- [x] A plain `dual-pane` brings the window forward and changes no tab. A request arriving during a Waiting Window, an Operation Decision Card, or a quit confirmation applies without dismissing or answering it; a modal window retains focus instead of the Folder Items List stealing it.
- [x] `--new-instance` starts an independent instance whatever is running, does not touch the rendezvous socket or rendezvous lock, still participates in journal liveness, and does not stop a later plain launch from forwarding to the registered instance.
- [x] Failure handling is bounded: a stalled or malformed client delays the one-at-a-time accept loop only until its per-connection deadline; a receiver that is committed to quitting refuses new work; and a launcher that loses the startup race waits for the winner's socket. An unacknowledged forward is retried at most once as the same idempotent request, then prints a message and exits with status 1 without silently becoming a new receiver.
- [x] Canonical profile identity separates endpoints. The owner-private rendezvous directory and socket permissions are verified rather than assumed, and a hash collision or mismatched profile identity fails closed instead of sending a request to another profile. A debug `DUAL_PANE_TEST_PROFILE` directory works however long its path is because the socket path itself stays below macOS's 104-byte limit.
- [x] The window comes to the front from another application's focus, is restored from the Dock without losing its maximized or full-screen state, and activates correctly from another Space under the supported macOS setting. The implementation tries Qt first and invokes the approved Objective-C++ fallback (row 7) only when the application is still inactive on the next GUI turn; the observed system behavior is documented.
- [x] Application tests cover the either-Browser search, active-tab then active-Browser preference, a matching in-flight tab, tab-limit behavior, last-wins before launch completion, and raise-only behavior. Pure Rust desktop tests cover the protocol (valid, truncated, oversized, wrong version, non-UTF-8 bytes, profile mismatch), rendezvous races, timeouts, shutdown, and stale-socket recovery; an acknowledgement follows the application's accepted request. Desktop checks cover modal-focus preservation.
- [x] A new `scripts/check-single-instance.sh` runs the offscreen Qt build with disposable `DUAL_PANE_TEST_PROFILE` directories and temporary folders. With bounded polling and cleanup traps, it proves that a forwarded target reaches the first process while only one process remains, a plain launch changes no tabs, a simultaneous startup elects one receiver, two profiles remain isolated, a bad path exits 2, `--new-instance` leaves two processes, a stale socket is replaced, and the socket is gone after quit. It is listed in [AGENTS.md](../../AGENTS.md) and README commands.
- [x] Interactive acceptance, run by the owner after the automated checks, covers raising and focusing from another app, across Spaces, from a minimized window, and while a modal quit confirmation is open. Record the macOS version, relevant Spaces setting, and result here.
- [x] The "Every launch starts its own instance" sentence in [product behavior](../product-behavior.md#command-line-launch), the usage text, the README, the [UX terms](../ux-terms.md) if a name is added, and the [docs index](../README.md) are updated with the implementation.
- [x] Direct `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, and so do `make check` and `scripts/check-single-instance.sh`. Record evidence here before marking P4-M9 Done.

## Acceptance evidence

Automated evidence on 2026-10-19:

- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed.
- `make check` passed, including Rust and C++ formatting, Clippy, clang-tidy, and all tests.
- `scripts/check-single-instance.sh` passed its isolated offscreen process scenarios: forwarding, raise-only tab preservation, startup election, profile isolation, argument failure, explicit independent instance, stale-socket replacement, and normal cleanup.
- Application tests cover either-Browser lookup and preference, in-flight reuse, tab-limit navigation, pre-restore last-wins behavior, raise-only behavior, and file selection. Desktop unit tests cover raw-byte framing and rejection, profile isolation, short endpoints, stale cleanup, acknowledged forwarding, launch-liveness and crash sweeping, untrusted liveness paths, and strict temporary-name filtering.
- Review checks also cover a launch back to a displayed folder while navigation away is pending, a byte-dripping client that cannot extend the request deadline, and a plain launch that preserves the complete saved Browser/tab state. The activation callback checks for a modal or Waiting Window again on its next GUI turn before moving list focus. `make check`, the three direct Cargo commands, and `scripts/check-single-instance.sh` passed after these changes.

Additional automated evidence on 2026-10-21:

- The application operation-workflow test proves that a forwarded raise leaves an Operation Decision Card pending and unanswered.
- `scripts/check-single-instance.sh` now begins with a real-widget offscreen check. It proves that the Waiting Window stays visible, an existing quit-style modal retains its focused button, a modal that appears before the deferred activation prevents Folder Items focus, a normal Browser request focuses its list only when no blocking surface exists, and a raise-only request changes no Browser focus.
- The native clipboard tests serialize their process-global AppKit pasteboard seam; all eight passed three consecutive runs after an intermittent overlap was reproduced during the gate.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `make check`, and `scripts/check-single-instance.sh` passed on the complete worktree.

Owner-run native acceptance passed on 2026-10-21 with macOS 26.7.1 (build 25G241). The `com.apple.dock` `workspaces-auto-swoosh` preference was unset, so the system default governed the Mission Control “When switching to an application, switch to a Space with open windows for the application” behavior. Raising and focusing from another application, activation from another Space, Dock-minimized restoration without losing the prior window state, and forwarding while the quit confirmation remained open and focused all passed. The Objective-C++ fallback remains narrowly conditional: it runs only on the next GUI turn when the Qt raise/activate attempt still leaves the application inactive.
