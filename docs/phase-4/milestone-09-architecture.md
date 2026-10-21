# P4-M9 architecture

Status: Done

A launcher process either forwards its resolved command-line target to the instance that owns the profile's socket, or becomes that instance. The receiving instance turns the request into the existing `OpenLaunchTarget` command plus a window-activation output; the application, not the launcher, chooses the Browser. The [overview checklist](milestone-09-overview.md#completion-checklist) defines acceptance.

## Starting point (before P4-M9)

- **Launch.** `main.rs` calls `launch_target::parse` before Qt starts. It resolves the path with `fs::canonicalize`, and the result reaches `WorkspaceBridge::start` inside `BrowserStartup`, which submits `Command::OpenLaunchTarget { folder, select }`.
- **Application.** `Workspace::apply_launch_target` looks only in `BrowserSide::Left`, activates a matching tab or opens one (or navigates at the tab limit), and sets `pending_selection` for a file. The command stores the target in a single `Option` and applies it once `launch_completed`, so a later target replaces an earlier one that has not yet applied.
- **Profile.** Debug builds take the profile directory from `DUAL_PANE_TEST_PROFILE`; otherwise it is `HOME`. `main.rs` computes it once.
- **Journal.** Every launch has a random `LaunchId` carried in temporary names and journal records. `Journal::sweep` removes the temporaries and records of **every other launch** (`launch <> current`), and it runs at startup through `operation_lane.rs`.

## Journal safety

This is a prerequisite, not a polish item. Today a second concurrent instance already runs the sweep against the first instance's live records, and removes `.partial` files of a copy that is still running. With `--new-instance` supported, that becomes a normal path, so it needs a real answer before any forwarding code.

Accepted design (owner decision row 6): each instance, including `--new-instance`, holds an exclusive **launch-liveness lock** on `launch-<hex>.lock`, next to the journal. This is separate from the one per-profile rendezvous lock below. Open the regular lock file read-write with mode 0600, then acquire it before the journal can open, record, or sweep; the owning file handle stays alive until operation workers have stopped. If creation, validation, or acquisition fails, the journal is unavailable and operations keep their existing fail-closed behavior.

The sweep groups records by launch and tries to lock that launch's liveness file once. `WouldBlock` proves that the launch is alive, so the whole group is skipped. An acquired lock proves that no lock-aware process is alive under that ID; the sweep holds it while removing all of the launch's leftovers and records, then unlinks the liveness file before closing the handle. This ordering prevents a second sweeper from observing an unlocked name halfway through the group.

- The change is centred in `operation_journal.rs`, with lifetime wiring through `workspace_bridge.rs` and shutdown of the operation lane. The journal keeps the one SQLite file. No GUI-thread path waits for a file lock.
- Missing, malformed, symlinked, unreadable, or otherwise untrustworthy liveness files make the associated records ineligible for automatic deletion, so doubt never deletes. This conservatively retains records created before the lock-aware format instead of guessing that an older concurrently running build is dead, and reports the retained group through the existing sweep-failure Notice path.
- A normal owner may remove its liveness file only after its operation workers have stopped and the journal proves that launch has no records. Otherwise the file remains for a later sweep. Free orphan lock files with no records may be pruned while locked.
- Every listing and operation scan recognizes the complete, strict reserved name `.dual-pane-<32 lowercase hex>-<operation>-<n>.partial`, regardless of launch ID. This keeps a live second instance's internal files out of the UI and out of recursive operation input. Names that do not match the entire grammar remain visible user files.
- Tests, in temporary directories: a live second launch's records and `.partial` files survive another launch's sweep; a lock-aware launch whose holder is gone is swept; missing and untrusted liveness files leave records alone; two sweepers cannot split one launch group; and exact foreign-launch temporary names are hidden while near matches are not.
- The owner rejected the alternatives (only the registered instance sweeps, or `--new-instance` needs a separate profile).

## Rendezvous

A small pure Rust module in the desktop crate, `instance.rs`, owns the process-level protocol. It uses `std::os::unix::net` and `std::fs::File::try_lock` (stable on the installed Rust 1.98 toolchain). The standard API maps `try_lock` to nonblocking exclusive `flock` on Unix and releases the lock when the final file descriptor closes. Any direct `flock`, peer-credential call, or other libc call would have to live in `native_calls.rs`; the plan avoids needing one.

- **Identity and names.** Canonicalize the existing profile directory once and use its raw path bytes as the profile identity; when no profile exists because `HOME` is absent, use a distinct fixed no-profile identity. The socket and rendezvous lock use two domain-separated, hand-written 64-bit FNV-1a values plus a format version under `std::env::temp_dir()`, keeping the socket below macOS's 104-byte `sun_path` limit without relying on Rust's unstable `DefaultHasher` output. The protocol also carries the exact identity, so a hash collision is refused rather than crossing profiles. Before use, verify that the temporary directory is a real owner-private directory; create endpoint files with mode 0600 and fail closed with a `--new-instance` hint if those conditions cannot be established. The owner-only directory and socket make a separate peer-credential API unnecessary.
- **Acquire.** The launcher first tries to connect. On success it forwards (below). On `ENOENT` or `ECONNREFUSED` it opens the regular rendezvous lock read-write and tries `try_lock`. If it gets the lock it deletes any stale socket, binds, listens, and keeps the lock for its life: it is the receiver. If the lock is held but the connect still fails, another launcher is starting, so it retries connecting with a short backoff for a bounded time, then exits with status 1.
- **Order.** The receiver binds before `run_desktop`, so launches made while it is still starting queue in the listen backlog and are answered once its accept thread runs. `OpenLaunchTarget` already waits for `launch_completed`.
- **Cleanup.** Shutdown first refuses new requests and closes the listener, then stops the accept worker, removes the socket while still holding the rendezvous lock, and finally closes the lock handle. A crash leaves a socket that refuses connections and a lock the kernel released, which the next launcher replaces. Holding the lock through unlink prevents an old receiver from deleting a successor's socket.
- **`--new-instance`.** The launcher skips this rendezvous only. It runs the existing startup path and never registers as the receiver, but it still owns a journal liveness lock.

### Protocol

One connection carries one request and one reply. Both are binary, since paths are bytes.

| Part | Content |
|---|---|
| Request | Magic and version `DPL1`, a kind byte (`Open` or `Raise`), the length-prefixed canonical profile bytes, then for `Open` a length-prefixed folder path and an optional length-prefixed file name. Integer encoding and a fixed whole-frame limit are specified in `instance.rs`; oversized, trailing, truncated, unknown-kind, wrong-profile, or wrong-version frames are refused. |
| Reply | One status byte (`accepted`, `refused`, or `shutting-down`). It is written only after the GUI/application owner has handled the input enough to accept or refuse it, not merely after the socket worker has decoded it. |
| Launcher | Sends, waits within a deadline, and exits 0 only on `accepted`. A reset or timeout retries the same idempotent request at most once; after an ambiguous attempted forward it never silently elects itself receiver. A refusal or exhausted retry prints a message on standard error and exits 1. |

`Raise` and repeated delivery of the same `Open` are application-idempotent: the latter finds the tab created or targeted by the first delivery instead of opening another. The receiver converts raw bytes with `native_location::location_from_path`; it does not repeat blocking path I/O on the GUI thread. The launcher already resolved and checked the path before contact. If the target changes afterward, the receiving tab's ordinary asynchronous navigation and selection behavior reports the resulting error or absence.

## Receiving side

The accept loop is a worker under the [threading rules](../architecture.md#51-threading-and-responsiveness): it never touches Qt or workspace state. The current runtime has worker-result delivery but no external command-input path, so add a bounded driver-owned queue with the existing coalesced `schedule_gui_drain` wake. Each queued request carries a one-shot reply endpoint. The GUI drain submits the typed command to `WorkspaceSession`, then reports accepted or refused; the socket worker alone waits for that result and writes the reply. No GUI callback waits for the socket worker.

The accept loop handles one connection at a time. A short read deadline and a whole-request deadline bound how long a stalled client delays later launches; EOF before a complete frame is a refusal. On shutdown, closing the listener wakes the loop, pending reply endpoints receive `shutting-down`, and teardown does not leave a join waiting on GUI work. Queue capacity failure is a typed refusal rather than an unbounded allocation.

## Application changes

- The application chooses the Browser for `Command::OpenLaunchTarget`. Search the active Browser first, its active tab before the remaining tab order, then search the other Browser the same way. A tab whose confirmed location or current in-flight request equals the folder is a match, preventing a duplicate while it loads. Activate the match and its Browser; otherwise open in the active Browser. `apply_launch_target` replaces the hard-coded `BrowserSide::Left`.
- A new `Command::RaiseWindow` (plain launch) and an output that asks the desktop to activate its window. Both are Qt-free. `OpenLaunchTarget` emits the same output.
- The activation output asks for the target Browser's Folder Items List to take focus through the path `focus_active_list` already uses; focus follows the request, not the tab's load result. If a modal window owns focus, bring that window stack forward and preserve its focus instead of bypassing or answering it.
- Last `Open` request wins before `launch_completed`, as the single `Option` already does. Acceptance is acknowledged when the application stores the request; navigation and its activation output wait until the session is ready, so a superseded startup request never steals focus later. A plain `Raise` may bring the Waiting Window forward immediately and does not replace a pending target.
- Row 5 applies this rule to the first instance's own target too, replacing the Left-Browser-only rule.

## Raising the window

Qt documents that `activateWindow()` requires a visible window and may need `raise()` for stacking. The first attempt therefore runs on the GUI thread, makes the appropriate top-level or modal window visible, clears `Qt::WindowMinimized` while preserving maximized/full-screen flags, calls `raise()`, and then calls `activateWindow()`. It must not use unconditional `showNormal()`, which would discard the prior maximized or full-screen state.

macOS may still leave a background application behind or on another Space. This is a verification item, not an assumed Qt guarantee. If the native check fails, use the owner-approved, narrowly isolated Objective-C++ `NSApp activate` fallback (row 7), without exposing Objective-C types across the bridge. Record the macOS version and Spaces setting because the operating system controls whether activation changes Spaces.

## New instance and shared profile data

`--new-instance` is documented as an independent instance, not a second view of one session.

- **Settings and session.** Both instances open the same `settings.sqlite3`. SQLite serialises writes; the saved session is whichever instance saved last. Favorites and settings are read at launch, so a change in one instance is not seen by the other until it relaunches.
- **Journal.** Covered by [Journal safety](#journal-safety).
- **Operations.** Two instances can operate on the same files. Each operation's own safeguards are unchanged, and no cross-instance conflict detection is added.

## Sequencing

1. Journal safety and its tests.
2. Application: Browser choice for `OpenLaunchTarget`, `RaiseWindow`, the activation output, and their tests. This also settles row 5.
3. `instance.rs`: secure endpoint identity, acquire, bounded protocol, shutdown, and tests with temporary directories and `UnixListener`.
4. `main.rs` and bridge wiring: the bounded external-input queue, accept worker, acknowledgement path, `--new-instance`, and usage text.
5. Window activation in `desktop_window.cpp`, then `scripts/check-single-instance.sh`.
6. Documentation, acceptance evidence, and the owner's interactive check.

## Verification

Follow the repository's automated-first rule. The application crate carries Qt-free Browser-choice and output tests. Pure Rust unit tests beside the desktop modules cover protocol, rendezvous, liveness, and journal behavior, although building the desktop crate still requires Qt. Only whole-process facts need the built binary, so they live in the offscreen script, which uses a disposable profile and temporary folders and never real user paths. Raise and focus behavior across applications, Spaces, a minimized window, and a modal window cannot be proved offscreen, so it stays an owner-run check recorded in the overview.

## Risks

| Risk | Mitigation |
|---|---|
| A second instance deletes a live operation's temporaries. | Journal safety is step 1 and has its own tests. |
| A second instance exposes or copies a live `.partial` file. | Listings and recursive scans reserve and hide the strict temporary-name grammar across all launch IDs. |
| Window stays behind another app. | Qt-only attempt first; the approved `NSApp activate` fallback only if it fails. |
| Socket path exceeds 104 bytes. | Short hashed names in the per-user temporary directory. |
| A profile-hash collision crosses profile boundaries. | The protocol verifies the canonical profile bytes and fails closed on a mismatch. |
| Two launchers race to become receiver. | Lock-then-bind, with a bounded reconnect for the loser. |
| A hung receiver or client blocks every launch. | Bounded frame, queue, read, and acknowledgement deadlines; then exit or refuse visibly. `--new-instance` is the explicit way around a stuck receiver. |
