# P3-M9 architecture

Status: Planned

P3-M9 turns the P3-M2 workspace-session handoff into a working save and restore path. The application owns the snapshot, including a Qt-free window layout, as well as the one-time restoration, the discard rules, timeout protection, the Reset Settings defaults, and the startup Notices rule. The settings driver stores the session in its own tables and transaction. Presentation reveals the window at its restored layout, fits it to the connected displays, and renders the Waiting Window and Notices Startup Checkbox.

## Starting point

- `WorkspaceSnapshot`, `BrowserSnapshot`, and `TabSnapshot` exist in `settings.rs` but nothing uses them, and `TabSnapshot` still carries the abandoned `filter` field.
- The settings database is at schema version 3, with no session or filter table. Its `setting_marker(key, value)` table already holds `favorites_initialized`.
- `WorkspaceSession::start` navigates both Browsers to the launch directory before settings load. `settings_loaded` refreshes every tab and also runs for the reload after Reset Settings.
- `Workspace::with_home` creates placeholder tabs 0 and 1 without a location. `BrowserTabs` can only be created from one tab, and closing a Browser's last tab navigates its replacement to Home.
- A tab whose first read fails loses its pending location, so its label is empty and `Command+R` has nothing to retry.
- `Output::NoticeAdded { open }` opens Notices only for failures. The Notices Startup Checkbox is not rendered.
- The window is shown at once at 1100 × 680 points. The Sidebar Splitter and Browser Divider are `ThinSplitter`s at their built-in positions, and nothing saves any of these.
- A successful Reset Settings replaces settings and Favorites but leaves tabs and the window alone.

## Launch sequence

1. Home is the only fallback, and `Workspace::with_home` already holds it, so the constructor's synchronous `SettingsLoadFailed` with no settings worker knows where to fall back. `start` stops navigating, and `BrowserStartup` drops its working-directory location because no startup path uses it any more.
2. The workspace stays hidden until the first settings-load answer or ten seconds. Connect the restore and reveal handlers before starting settings work: the no-worker failure is synchronous. If a short delay passes without an answer, presentation shows the Waiting Window and animates it without blocking the GUI thread. Until an answer or timeout, each Browser holds its placeholder tab, which reports as loading and reads nothing. Commands that need a location stay unavailable. A success, load failure, no-worker failure, or timeout ends the wait and reveals the restored workspace or Home fallback.
3. The first answer or timeout completes launch restoration exactly once, guarded by a launch flag. Restoration and the fallback act only on an untouched placeholder, one with no location and no pending read. A Browser navigated before the load keeps its tab, and that tab is refreshed with the loaded sort as today. The refresh loop at the end of `settings_loaded` therefore covers only such tabs on the first load, because restored and fallback reads already use the loaded sort memory. A late answer after a timeout may load settings and Favorites but never replaces visible tabs, active Browser, or layout. A reload after Reset Settings only replaces settings and keeps the refresh loop.
4. The desktop starts a single-shot timer when the workspace window appears, and about five seconds later its elapsed event ends the startup period if launch work has not already ended it. A Reset remains unavailable after a timeout until the original load answers: the single settings worker must finish its in-flight read before it can safely move the database aside. Once a late success has made settings `Loaded`, Reset is available and clears the session-write protection only if it succeeds.

Track restoration completion separately from `SettingsStatus`: the timeout completes presentation startup but leaves the original worker load pending. Its eventual success may move settings to `Loaded`; its eventual failure may move them to `LoadFailed`. A separate session-write protection flag survives either late result and is cleared only by a successful, explicit Reset Settings. The first-answer, timeout, and reset-reload paths are distinguished by a launch generation, so a late result cannot run restoration twice or affect a newer database.

## Restoration

The settings load result carries the session separately from the settings: `Event::SettingsLoaded { snapshot, session }`, where `session` is absent, saved, or damaged.

- **Saved session.** For each Browser, a new domain constructor builds `BrowserTabs` from an ordered list of fresh `TabId`s taken from `next_tab` plus the active index. It refuses an empty list, more than `BrowserTabs::LIMIT` tabs, or an out-of-range active index; the driver rejects such rows first, so a refusal means a damaged session. The placeholder is dropped, never read. The saved active Browser is applied after tab construction and wins over any focus event that activated the other Browser before restoration. Restored reads use the folder-sort memory and start in one deterministic order: that Browser's active tab, the other Browser's active tab, then every remaining tab in Left-then-Right Browser and strip order. The read's token is recorded as the tab's restoration token.
- **No session, failed load, or no worker.** Each untouched placeholder navigates to Home.
- **Damaged session.** As above, plus a Session Not Restored Notice.
- **Load timeout.** At ten seconds, each untouched placeholder navigates to Home and a settings-load timeout Notice explains that the saved session remains protected. The workspace appears at its default layout. Even if the original load later succeeds, session writes remain disabled for this launch, including at quit; a deliberate successful Reset Settings clears this protection for the fresh database. A late failure keeps the in-memory defaults and all settings saves blocked. A late success can apply settings and Favorites and probe their targets, but cannot replay the saved session or layout.

`Output::TabsChanged`, `ActiveBrowserChanged`, and `TabViewChanged` already describe the tabs. A new `Output::SessionRestored { active_browser, layout }`, emitted once for every outcome of the first answer or timeout, tells presentation to apply the saved layout, or the default when `layout` is `None` as in every fallback, then reveal the workspace and move keyboard focus once. Any Waiting Window closes during that transition. A later load result cannot emit this output again.

A restored tab whose read never answers, such as one on an unresponsive volume, stays loading. No timer discards it; the person can close it or navigate it elsewhere.

## Discards and active tab

A failed read whose token equals the tab's restoration token, with `ItemMissing`, `NotADirectory`, `PermissionDenied`, `PrivacyRestricted`, or `Unknown`, discards the tab. The discard uses the existing `BrowserTabs::close` rule, nearest on the right and then on the left. Applying it at each discard gives the nearest surviving tab whatever order the results arrive in, because the active tab only ever moves past tabs that turned out to be discarded. If the tab was the Browser's last, `replace_final` creates a clean tab that navigates to Home, the same replacement `close_tab` makes. Each discard adds its own Tab Discarded Notice with the location and error kind, and requests a session save.

Restoration ends for a tab, without a discard, on success, on `Busy` or `Internal`, on a cancellation, when the tab closes, or when a read the person asked for replaces it: navigation, Back, Forward, Up, a Favorite, or `Command+R`. After that, the tab follows the ordinary error rules and is never discarded. A re-read the workspace starts on its own for the same location, such as `refresh_location` after a `LocationInvalidated`, carries the restoration mark to the new read's token. An invalidation, such as the Trash folder's after a Move to Trash, therefore cannot save an unrestorable tab from its discard.

## Never-confirmed tabs

`TabState` keeps the location it was asked to open until a read confirms one. After a failure that keeps the tab, that location still labels the tab, enters the session snapshot, and is what `refresh_tab` rereads. This fixes the same gap for a new tab whose first read fails.

## Window layout

`WindowLayout` is an application value with no Qt type:

- the normal frame (x, y, width, height) in logical points, which is the frame before any zoom or full screen;
- the window state: normal, zoomed, or full screen;
- the Sidebar Splitter's and Browser Divider's pane sizes in points.

The workspace holds the last reported layout, or `None` until presentation has measured the default or restored window. A persisted session always has a layout row. Typed values keep the stored format independent of Qt's opaque `saveGeometry` and `QSplitter::saveState` bytes and make validation testable.

Presentation reports a layout through `Command::UpdateWindowLayout { layout }` once after first reveal, when a move or resize ends, when zoom or full screen changes, when a splitter drag is released, and once more from the close event before quitting. The first report includes a fitted frame if the saved frame was offscreen; it makes the initial snapshot eligible for saving. The command bumps the session revision. The close event's report is handled before `QuitAccepted`, including after a quit confirmation, so the final flush contains it. During full screen or zoom, the reported normal frame is the window's `normalGeometry()`.

Applying a layout happens once, before the workspace window is revealed. The desktop fits the normal frame to the current displays and applies it, then stages the requested zoom or full-screen state and both splitters' sizes. Full screen opens in its own Space; the native check verifies the transition because macOS may complete it asynchronously. `QSplitter::setSizes` scales saved sizes when the window is narrower than when they were saved.

Fitting is a Qt-free function over the saved frame and the available display rectangles, with the main display first, so it can be tested without Qt. It picks the display with the largest overlap, or the main display when nothing overlaps, shrinks the frame only if it is larger than that display, and moves it fully inside. The desktop supplies `QScreen::availableGeometry` for each screen.

## Reset Settings

A successful reset, `Event::SettingsReset`, also returns the workspace to its defaults. It closes every tab in both Browsers and gives each Browser one clean Home tab with fresh IDs. It makes the Left Browser active, clears the pending selection and pending open, and cancels the old tabs' reads. Running file operations continue, as they do when their source tab closes, and inline editors on closed tabs close. It sets the layout to `None` and emits `Output::LayoutReset`. Presentation answers by leaving full screen or zoom, then applying the default size centered on the display the window is on and the default splitter sizes. It reports the result as a layout update.

The reload that follows never restores from the database or reopens startup Notices. Once it is `Loaded`, the workspace requests a session save, which writes the Home tabs and the current layout into the fresh database. A failed reset changes none of this. The Reset Settings confirmation says that open tabs close and the window layout returns to its defaults.

## Session snapshot and saves

`Workspace::session_snapshot` builds a `WorkspaceSnapshot` from the domain tab order, the active tabs, the active Browser, and the reported layout. A tab's location is its confirmed folder, or its requested folder if it never confirmed one. A pending navigation away from a confirmed folder does not count until it succeeds. After restoration every tab has a location, so each Browser contributes at least one tab; the builder returns no snapshot rather than a Browser with zero tabs or a missing layout row, which the next launch would report as a damaged session.

The workspace keeps a session revision, separate from `SettingsState`'s, and bumps it on every decided trigger and the first layout report after a load-answer restoration or fallback. A trigger emits `WorkRequest::SaveSession { revision, session }` only when settings are `Loaded`, restoration has run, a layout has been reported, and the launch has not timed out, so a pending, failed, or timed-out load cannot overwrite the stored session. This first request also replaces readable but invalid session rows with the Home fallback. After timeout, even a late successful load does not lift the session-write block for that launch; ordinary settings and Favorites saves can resume, because their transaction does not touch session rows. A successful deliberate Reset Settings clears the block for the fresh database. `final_session_save` joins `final_settings_save` for the quit flush; the worker receives Quit even when only one kind of snapshot is dirty. The save delay comes from the worker's coalescing; [architecture section 8](../architecture.md#8-deliberate-non-decisions) leaves the value open.

Session save results older than a reset are ignored by the same floor rule that settings saves use. A session save failure reuses the deduplicated settings save-failure Notice.

## Startup Notices

`SettingsState` gains `hide_notices_at_startup`, which is part of `SettingsSnapshot` and saved through the existing prompt settings save. `Command::SetHideNoticesAtStartup { hide }` changes it. Presentation disables the checkbox until the first settings-load answer and the application ignores an early toggle, so a late load cannot silently replace a choice made before the stored preference was known.

The workspace tracks the startup period from construction. It ends when the settings load, restoration's reads, the Favorite launch probes, and the temporaries sweep have all answered, or on `Event::StartupPeriodElapsed`, whichever comes first. A startup notice is one produced by launch work: Tab Discarded, Session Not Restored, Settings Load Timed Out, Favorite Removed, or Favorite Probe Failed from the launch probes, Temporaries Swept, and the launch settings-load or journal-status result. Probes after a reset are not launch work. The timeout occurs before the workspace appears and queues one Notices opening until the workspace is visible, even though the saved preference is not yet known. A later load result does not repeat the timeout opening; a distinct actionable failure keeps its normal rule.

- Notices that offer an action open Notices immediately, as today.
- With the box checked, other startup notices never open Notices, even if they arrive after the period. This overrides P3-M8's rule that sweep failures open Notices.
- With the box unchecked, the first startup notice recorded during the period requests one opening. A request made before the preference loads waits for the load answer. A failed load, including a missing settings worker, leaves the default, unchecked, so its Notice still opens Notices even though `WorkerUnavailable` offers no Reset Settings.
- Every other notice keeps its P3-M8 rule.

Presentation receives the request as the existing `open` flag on `NoticeAdded`, or as a separate `Output::OpenNotices` when the request was waiting for the preference.

## Settings driver

Schema version 4 adds three tables, for example `session_browser(browser, active_position, active)`, `session_tab(browser, position, location BLOB)` keyed by Browser and position, and a single-row `session_window` holding the layout. Locations use the existing lossless `encode_location`, and the preference is stored as a `setting_marker` row. The migration from versions 1–3 creates empty tables, so an upgraded profile has no session and opens Home tabs at the default layout once.

`load_connection` validates only the settings tables, because it runs inside `migrate` and `open`, where a failure means the database is corrupt. A single read transaction then obtains both the settings snapshot and the session result before the transaction ends; the application never receives a torn launch snapshot if another process changes the database. The session reader classifies readable malformed rows or broken relationships as damaged. SQLite failures that prevent reading the session fail the whole settings load, preserve the database, and block saves; a transient I/O failure must not turn a valid session into a writable Home fallback. An empty session has no Browser, tab, or layout rows and uses the default layout. A saved session has exactly two Browser rows, no orphan rows, exactly one active Browser, contiguous positions with one to eight tabs per Browser, an active position inside the tab range, and decodable locations. It requires exactly one layout row with finite values representable by Qt's logical-point APIs, a positive frame size, a known window state, and two non-negative splitter sizes representable by `QSplitter::setSizes`. A missing or malformed layout row beside saved tabs damages the whole session.

`SettingsJob::SaveSession { revision, session }` is coalesced on the worker independently of `Save`, with the newest of each kind kept. It is written in its own transaction that deletes and reinserts only the session rows. `SettingsResult::SessionSaved` and `SessionSaveFailed` carry its revision. `Quit` carries both optional snapshots, is sent even when only a session is dirty, and the shutdown timeout covers both writes. A launch timeout does not cancel or duplicate the worker's Load: its eventual result is still delivered once for late settings handling. Reset Settings already moves the whole database aside. A successful Reset is an ordered barrier: it drops queued settings and session saves from before the reset, leaves the new database without a session, and accepts new snapshots only after the reset reload. A failed Reset keeps the old database and pending saves. The application ignores stale pre-reset results for both revisions.

## Presentation

- **Reveal.** `main` no longer shows the workspace unconditionally. Connect the handlers before `bridge.start`, because a missing settings worker answers synchronously. A short single-shot delay shows a transient Waiting Window only when the load has not answered, avoiding a flash on fast launches. A ten-second timer sends `Event::SettingsLoadTimedOut` only if no answer has arrived. `SessionRestored` stops the animation, applies the geometry, window state, and splitter sizes, reveals the workspace once, reports its actual layout, and closes Waiting. Queued startup Notices open only after the workspace is visible, without activation. A late load does not move the visible window. A Waiting-Window Quit enters the normal quit path, shuts down the settings worker within its existing bound, and never emits a fallback snapshot. Verify the macOS full-screen transition in the native check.
- **Waiting.** Paint two restrained Browser shapes and an eased moving accent path against the existing dark palette; include the text “Loading workspace” and an indeterminate accessible status. Animate with a GUI timer and no new dependency. Reduced-motion mode shows a static accent. Quit remains available; the waiting UI never asserts numerical progress or silently resets stored data. It closes on the first answer or timeout.
- **Browsers.** The presenter renders a placeholder tab as loading with no label. A discarded tab leaves the strip through the existing `TabsChanged` path; other tabs keep their view state. On `SessionRestored`, the desktop moves focus to the active Browser's Folder Items List, but only while focus is in a Folder Items List or nowhere, so it never takes focus from an editor or another window.
- **Layout reports.** The window and both `ThinSplitter`s report layout changes at the end of each move, resize, state change, or drag, which avoids a save per pixel.
- **Notices window.** The Notices window gains a `QCheckBox` with the text “Don’t show notices at startup” and the accessible name Notices Startup Checkbox, below the Notice entries. The workspace bridge exposes the preference and a toggle invokable. Opening for startup uses the existing open-without-activation path.
- **Wording.** The adapter owns the text for Tab Discarded, which names the folder and the reason, and for Session Not Restored. Neither offers Reset Settings. The Reset Settings confirmation text gains the tab and layout consequences.

## Verification

1. **Application tests:** launch sequence and the one-time guard; restoration with saved, absent, damaged, failed, and timed-out loads; deterministic read dispatch; late success and failure after timeout, including session-write protection across navigation and quit; a Browser navigated before the load; each discard and non-discard result; superseded and closed restoring tabs; a `LocationInvalidated` re-read that keeps the discard; last-tab fallback to Home; never-confirmed tab retention; layout updates, restoration, and the close-event report before quit; Reset Settings defaults, including a failed reset, Reset being unavailable while a timed-out load remains in flight, and an operation running from a closed tab; save triggers and gating; quit during restoration and while Waiting; reset reload; the startup period ending both ways; and the opening rule for each preference value, including a notice that arrives before the preference loads.
2. **Property tests** in the application's `proptest` suite: restored equals saved minus unrestorable tabs for any result order, with the policy's active tab and the saved layout, as [architecture section 7](../architecture.md#7-verification-strategy) requires.
3. **Fitting tests:** a frame that already fits, an unplugged display, a smaller display, a frame larger than every display, and no overlap.
4. **Driver tests in temporary directories:** migration from versions 1–3, round trip with non-UTF-8 locations and every window state, one consistent settings-plus-session read transaction, each damaged-row case including non-representable layout values, a session without a layout row, SQLite session-read failure without a replacement write, transaction isolation from settings, separate coalescing, session-only shutdown flush, and Reset Settings with pending saves on each side of the barrier.
5. **Presenter and bridge tests:** placeholder and discard rendering, focus request, layout commands and outputs, checkbox state and command, and wording.
6. **Earlier suites** stay green except for named, deliberate changes: `start` no longer navigates, the first-load refresh covers only tabs navigated before the load, a successful Reset Settings replaces tabs, and the startup opening rule applies until the period ends. Tests written before the startup period end it explicitly with `Event::StartupPeriodElapsed`.
7. **Human check at the end**, then the repository gate.

## Risks

- A settings database on a stalled disk leaves the Waiting Window visible for at most ten seconds, then opens Home tabs with a timeout Notice. The prior saved session and layout remain protected for the next launch. This launch's tab and layout changes are deliberately not saved unless the person resets settings, so the Notice must make that limit clear.
- Discarding on temporary unavailability loses a tab for good once the next save runs, for example when a volume is unplugged across a relaunch. This matches the owner's decision and the Favorites policy.
- Restoring up to sixteen tabs reads all of them at launch. That stays within each Browser's read admission (16 queued plus 4 lanes), and `Busy` never discards.
- Saving on every Browser activation and layout change adds small, frequent writes. Coalescing keeps them to one transaction per burst.
- Qt's macOS full-screen transition is asynchronous, so `normalGeometry()` during or right after the transition, and `showFullScreen` before the first show, need the human check. If either proves unreliable, the desktop records the normal frame itself before entering full screen.
- Moving the window onto another display changes the device pixel ratio. Logical points keep the frame meaningful, but fitting runs again only at launch.
