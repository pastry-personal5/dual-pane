# Hidden files and folders research

Status: Active

This note identifies the different reasons an item can be absent from a macOS file manager and records the evidence behind P3-M15's three visibility settings. The [P3-M15 overview](phase-3/milestone-15-overview.md#owner-decisions) owns the approved product behavior; this note explains the research and limits of the name list.

## Three different mechanisms

1. **A leading period is a name convention.** It covers useful project and personal data such as `.git`, `.gitignore`, `.env`, `.ssh`, and `.zshrc`. Apple notes that a leading period can make a resource hidden independently of its [`isHiddenKey`](https://developer.apple.com/documentation/foundation/urlresourcekey/ishiddenkey) value. Qt's [`QDir::Hidden`](https://doc.qt.io/qt-6/qdir.html) describes Unix hidden entries as names starting with a period. A dotfile is not automatically macOS debris.
2. **macOS has item-level invisibility metadata.** Apple's [`isHiddenKey`](https://developer.apple.com/documentation/foundation/urlresourcekey/ishiddenkey) describes a resource normally not displayed to users. Apple separately documents [`UF_HIDDEN`](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemDetails/FileSystemDetails.html) as a GUI hiding hint; older Finder information has a [`kIsInvisible` bit](https://developer.apple.com/library/archive/technotes/tn/tn1150.html). These are not equivalent to a leading period. An item can be named `Private Folder` and be invisible.
3. **Some exact names identify platform metadata or service storage.** A name rule can suppress known Finder sidecars and volume service folders without suppressing other dotfiles. This is a product policy, not an assertion that any matching item is safe to delete.

Apple's shallow [`contentsOfDirectory`](https://developer.apple.com/documentation/foundation/filemanager/contentsofdirectory%28at%3Aincludingpropertiesforkeys%3Aoptions%3A%29) documentation is a useful warning against assuming one system enumeration rule is a complete product policy: it says `._` resource-fork entries are omitted even without the `skipsHiddenFiles` option, while other hidden files are returned. Dual Pane currently uses Rust directory enumeration, so P3-M15 must define its own explicit behavior.

## Evidence and candidate names

| Item | Evidence | P3-M15 treatment |
|---|---|---|
| `.DS_Store` regular file | [Apple Support](https://support.apple.com/en-gb/102064) describes Finder's use of `.DS_Store` while browsing. | Hide under the macOS metadata/service setting. |
| `._*` regular file | [Apple's directory enumeration docs](https://developer.apple.com/documentation/foundation/filemanager/contentsofdirectory%28at%3Aincludingpropertiesforkeys%3Aoptions%3A%29) call these resource-fork entries. The [AppleDouble format](https://datatracker.ietf.org/doc/html/rfc1740) separates data and resource information. | Hide every regular file with the raw byte prefix `._`. A matching name alone does not prove valid AppleDouble contents, so reveal must remain possible. |
| `Icon` followed by carriage return, regular file | [Apple Developer Forums discussion](https://developer.apple.com/forums/thread/811273) identifies `Icon\r` as a custom folder icon file. This is supporting evidence, not a normative file-format specification. | Hide the exact name only; `Icon`, `Icon.png`, and `Icon\n` are different names. |
| `.AppleDouble` directory | The [Netatalk manual](https://netatalk.io/2.0/Netatalk-Manual2.0.5.pdf) documents companion `.AppleDouble` directories for AFP metadata. This is a legacy sharing convention rather than a guarantee about modern APFS. | Hide only an exact-name directory. |
| `.localized` regular file | [Apple's localization guidance](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemAdvancedPT/LocalizingtheNameofaDirectory/LocalizingtheNameofaDirectory.html) says system-defined folders can carry an empty `.localized` file. The same document also describes a `.localized` **directory** containing translation files for custom folders. | Hide only an exact-name regular file. Keep the `.localized` directory visible. |
| `.fseventsd` directory | [Apple's FSEvents guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/FileSystemEventSecurity/FileSystemEventSecurity.html) identifies this as event-log storage at a volume root and says applications should not work with its contents directly. | Hide an exact-name directory only when it is a direct child of a volume root. |
| `.Spotlight-V100`, `.Trashes`, `.TemporaryItems`, `.DocumentRevisions-V100` directories | An [Apple-hosted project's macOS ignore list](https://github.com/apple/ml-symphony/blob/main/.gitignore) groups these names as items that might appear at a volume root; [Apple's XNU source](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_fsevents.c) also recognizes `.Spotlight-V100` in event handling. An ignore list is supporting evidence, not a platform contract about each name's kind or placement. Apple's [item-replacement directory example](https://developer.apple.com/documentation/Foundation/FileManager/url%28for%3Ain%3AappropriateFor%3Acreate%3A%29) shows `TemporaryItems` without the leading period and in a different location, illustrating why exact name and placement both matter. | The owner approved these exact directories as volume-root service entries for the first release. Do not generalize to similarly named folders elsewhere. |

Other names were considered but are outside the approved first set. Apple's localization guidance makes `.localized` directories an especially clear false positive. The Apple-hosted ignore list also mentions `.VolumeIcon.icns` and `.apdisk`, but an ignore list is broader than this file manager's hide policy. A `__MACOSX` directory from an unpacked archive, `.metadata_never_index`, `.vol`, application caches, editor swap files, Git internals, and Windows `desktop.ini`/`Thumbs.db` require separate evidence and product decisions before adding them. Microsoft's [`desktop.ini` documentation](https://learn.microsoft.com/en-us/windows/win32/shell/how-to-customize-folders-with-desktop-ini) illustrates that a platform metadata file can also be intentional folder configuration; it is not part of this macOS-specific list.

## Approved visibility model

P3-M15 has three independent, global General settings:

| Setting | Compiled default | Matching rule |
|---|---|---|
| Show macOS metadata and service items | Off | The exact typed and placed items in the table above. |
| Show dotfiles | On | Every file, directory, or link whose raw name starts with `.`; `.` and `..` are never Folder Items. |
| Show items marked invisible by macOS | Off | The item's own macOS invisible status, independent of its name. |

An item appears only if every applicable setting permits it. For example, revealing `.DS_Store` requires both **Show macOS metadata and service items** and **Show dotfiles**. This overlap should be explained beside the Settings controls. Showing a matching item makes it an ordinary selectable Item; hiding one changes only the Folder Items view. It never removes or edits the on-disk object.

The existing current-launch Dual Pane operation-temporary suppression is unconditional and is not controlled by these settings. Previous-launch leftovers remain visible unless they independently match one of the three approved rules. Hidden paths remain valid explicit destinations: a saved tab, Favorite, or typed path can still open a folder even if its row is omitted from its parent.

## Product and safety consequences

- A visible Folder Items count and size total describe visible rows. A hidden child still exists on disk. Recursive Copy, Move, Trash, and Permanent Delete must inspect the actual filesystem and preserve their existing conflict and safety rules; a hidden child may appear in operation progress or a conflict decision.
- A hidden destination collision must still be detected by the operation workflow. Hiding an entry must not make a conflicting name appear available to New Folder or Rename.
- Switching from shown to hidden cannot leave an actionable stale row on screen. The application should immediately mark affected listings as transitioning, clear their visible rows, and read again under the new policy. If a read fails, an old broader listing must not reappear under the stricter setting.
- A change affects both Browsers and all open tabs. The request's captured policy and token, rather than mutable desktop state, decide which result can be applied. Rapid toggles and watch invalidations need the same cancellation and coalescing behavior as other listing restarts.
- Match exact case-sensitive raw bytes and entry kind. Do not use lossy display names, glob rules, or prefix matches beyond `._`. A service folder is matched only when its parent is proven to be that volume's root; an uncertain mount-root result should not be guessed from `/Volumes` or a string prefix.
- On the listing worker, check macOS invisibility on the entry itself without following a symlink to its target. Apple's `isHiddenKey` describes whether an item is normally displayed, which may include a leading-period name; P3-M15 must prove that its native check can identify the **separate metadata reason** before using it for the third switch. The native verification should compare the URL resource key, `UF_HIDDEN`, Finder information, links, and non-UTF-8 names. A metadata-read failure needs an explicit listing outcome; silently treating unknown as visible would contradict the default.

## Repository fit

`folder_items.rs` currently calls a `ListingRules::hidden` name predicate before classifying an entry; production supplies only `is_launch_temporary`. Type-aware service matching and item-level invisibility require a second worker-side classification stage. `Workspace::start_read` currently passes location, sort, token, and previous rows. P3-M15 adds an immutable three-value visibility policy to that request and through the runtime's listing job. The application settings snapshot and SQLite driver own persistence; Qt only presents the three controls.

The change belongs after P3-M14 in the phase sequence. P3-M14 supplies the live General Settings page, while P3-M15 owns these three new values, their migration, and their listing effects.
