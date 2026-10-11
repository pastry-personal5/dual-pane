# P3-M15 architecture: File and folder visibility

Status: Planned

P3-M15 keeps visibility choices in the Qt-free application, captures them with each directory read, and evaluates them on the listing worker. The Qt Settings page renders three application-owned values; it does not filter rows or persist settings itself.

## Settings and presentation

Add a small typed `VisibilityPolicy` with `show_macos_items = false`, `show_dotfiles = true`, and `show_macos_invisible = false` compiled defaults. Keep it in `SettingsState` and `SettingsSnapshot`, expose three typed commands, and reject changes while settings are not loaded or Settings interaction is unavailable. A changed value increments the existing revision once and requests the ordinary coalesced settings save; unchanged input neither rereads nor saves.

The SQLite settings driver adds one schema migration and stores all three values in the existing settings database. Loading a supported older schema supplies the compiled defaults without resetting other settings, Favorites, or the workspace session. Invalid stored values are a typed settings-load failure, not a guessed Boolean. Existing explicit Reset Settings seeds defaults and retains its backup and workspace-reset contract. A failed save leaves the effective in-memory policy for this launch, closes Settings, and reports through Notices as it does for other settings.

Project the values through `WorkspaceChrome`, `WorkspacePresenter`, and `WorkspaceBridge` to P3-M14's General page. Each labeled checkbox sends its corresponding command. Help text states that an item matching both a macOS named rule and the dotfile rule appears only when both switches show it. The page reports no local persistence or matching authority.

## Read lifecycle and bounded delivery

Extend `WorkRequest::ReadDirectory`, the runtime `Job`, and `FolderItemsSource` with a copy of `VisibilityPolicy`. `Workspace::start_read` captures it alongside sort and request token. The worker neither reads mutable settings nor changes the policy of an already admitted job. A native event returns entries under the captured policy; the application accepts them only for the matching current token.

On any actual policy change, enumerate every open tab's current requested location, including inactive tabs and pending navigation targets. Cancel or invalidate prior reads. Immediately clear presented rows and mark the tab as updating visibility so a newly hidden row cannot remain actionable while the worker runs. Keep only the tab's path/history and enough prior selection/cursor identity to reconcile when a successful new listing arrives. A failed read stays empty with an actionable error and manual Refresh; it cannot republish rows from the former, broader policy. Decide the exact empty/loading presentation in the existing output/view-model boundary rather than adding Qt-only state. A late successful settings load after the startup timeout may change the effective policy; refresh the already-visible Home tabs without restoring the saved tabs or enabling session writes.

The domain allows at most eight tabs per Browser, while each Browser's current read lane admits 16 queued plus its running workers. Rapid toggles can still leave cancelled jobs awaiting terminal delivery, so do not rely on the tab limit alone. Deduplicate replacements by tab and policy revision, prioritize active tabs, and retry bounded admission failures for remaining tabs; no tab may remain indefinitely on an old policy. Watch invalidations that arrive during a visibility reread coalesce into at most one subsequent reread under the latest policy. Ordinary navigation, sort changes, and closing a tab supersede obsolete visibility work.

## Worker classification

Retain `is_launch_temporary` as the first unconditional check. After `DirEntry::file_type` supplies the entry's own kind, apply the exact-name/type predicate. The five approved service folder names also need proof that the directory being read is the current volume root. Use worker-side native mount information, not a `/Volumes` string rule. If mount-root identity cannot be established while the macOS-items switch is off, return a typed listing failure rather than guess the placement of a service item. Do not follow a matching symlink to hide its target.

For entries surviving the name checks, obtain the item's own macOS invisibility metadata when that switch is off. First verify `NSURLIsHiddenKey` against `UF_HIDDEN`, Finder `kIsInvisible`, plain dot names, links, and non-UTF-8 names in a temporary directory. Do not let a URL key's broad “normally hidden” result make every plain dotfile invisible under the third switch; the third switch acts only on actual item metadata. A native helper, if needed, accepts filesystem bytes and runs only on a listing worker. Do not rely on lossy `QString` conversion or `QDir::Hidden` for this independent setting. A failed status lookup returns a typed listing failure rather than assuming the item is visible; disappeared entries keep the reader's existing missing-entry handling.

The final decision is the union of applicable hide rules:

```text
hide = current_launch_temporary
    OR (macos_named_item AND NOT show_macos_items)
    OR (name_starts_with_period AND NOT show_dotfiles)
    OR (own_macos_invisible_status AND NOT show_macos_invisible)
```

Skip a hidden entry before `Entry` construction, package recognition, sorting, row diff, icons, or Quick Look requests. Keep byte-exact names through the predicate. The exact matcher handles regular files, directories, and links only as recorded in the [owner decision table](milestone-15-overview.md#owner-decisions).

## File operations and verification

Do not pass `VisibilityPolicy` to recursive scans or the native executor. Existing operations plan from selected visible roots and scan all descendants on disk. Destination existence checks remain independent of the listing, so an omitted item still causes a conflict or name rejection. Explicit path navigation and Favorites likewise do not consult the policy.

Build in this order: verify native invisibility with temporary fixtures and separately validate mount-root detection on disposable test locations; add values, migration, and Qt-free tests; extend requests and runtime source; apply worker classification; implement all-tab policy transitions; then wire General controls and test native interaction. Inject root identity for unit tests; do not create or change macOS service folders on a real volume. The full repository gate and native checks complete the milestone.
