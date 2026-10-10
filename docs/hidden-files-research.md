# Hidden-file research

Status: Proposal

This note separates macOS housekeeping artifacts from people’s hidden files and records a conservative candidate policy for P3-M15. It is research, not a product decision; the owner decisions at the end are required before implementation.

## What the platform tells us

- [Apple’s SMB guidance](https://support.apple.com/en-gb/102064) identifies `.DS_Store` as Finder metadata used while browsing folders. It is a strong candidate to hide from ordinary file-manager listings.
- Apple documents [AppleDouble `._` files](https://developer.apple.com/forums/thread/761587) as companions used to carry resource-fork and extended-attribute data on filesystems that cannot carry that metadata natively. A sidecar is housekeeping, but it can be relevant when repairing or transferring a damaged volume.
- macOS also has an item-level invisibility attribute: Apple exposes it as [`kMDItemFSInvisible`](https://developer.apple.com/documentation/coreservices/file_metadata/mditem/file_system_metadata_attribute_keys), distinct from a leading-dot name. That is presentation metadata, not proof that an item is disposable.

The sources support treating these as display concerns only. They do not justify deleting, changing, or silently operating on an omitted item.

## Candidate classification

| Category | Examples | Recommendation | Why |
|---|---|---|---|
| Finder metadata | `.DS_Store`, `Icon` followed by carriage return | Hide by the housekeeping preference | These are Finder-created presentation artifacts, not ordinary working files. |
| AppleDouble metadata | names beginning `._`; `.AppleDouble` directories | Hide by the housekeeping preference | They commonly accompany files on non-native filesystems, but must remain revealable and untouched. |
| Volume service directories | `.fseventsd`, `.Spotlight-V100`, `.Trashes`, `.TemporaryItems`, `.DocumentRevisions-V100`, `.vol` | Do not include in the first release without an explicit owner approval | They are normally system-managed but their availability and meaning vary by volume type. A static list is easy to overreach. |
| Unix and tool dotfiles | `.git`, `.gitignore`, `.ssh`, `.zshrc`, `.env`, `.vscode` | Always show under the proposed housekeeping policy | These are commonly deliberate user/project data. A blanket leading-dot rule would make a developer file manager unexpectedly lossy to use. |
| macOS-invisible items | Any item marked invisible without a matching known name | Defer | Supporting this requires native metadata acquisition and a clear reveal policy; invisibility alone does not identify system debris. |
| Dual Pane operation temporaries | Current-launch `.dual-pane-…` temporary names | Always suppress, outside the preference | P3-M8 already requires this safety behavior so operation intermediates are never exposed as ordinary Items. Leftovers from earlier launches remain visible. |

## Recommended first policy

Use one global, persistent General setting, proposed label **Show macOS housekeeping files**, default off. It controls only the first two rows above: exact byte-name matches for `.DS_Store` and `Icon\r`, a `._` byte prefix, and `.AppleDouble`. It is not a general “show hidden files” switch.

Changing the setting applies immediately to both Browsers and every open tab. It restarts each affected directory read under the new listing policy; normal refresh reconciliation retains only Items that are still displayed. The setting affects discovery and rendering only:

- hidden-by-policy items are never selected, opened, Quick Looked, or included in a source/destination choice merely because the preference changed;
- file operations retain their existing byte-exact, safety-first behavior and do not use this display policy to skip recursive contents;
- the existing current-launch temporary filter remains mandatory when the setting is enabled; and
- a visible known artifact is ordinary, selectable data. The app never auto-deletes it or offers special cleanup.

This bounded rule has a predictable escape hatch for support and repair work without hiding `.git` and other intentional dotfiles.

## Existing implementation seam

The desktop directory reader already accepts a `ListingRules::hidden` predicate before it builds a domain `Entry`. Production currently supplies only the mandatory current-launch-temporary predicate. P3-M15 can compose the new effective setting with that predicate on the listing worker; it does not need a second list model or Qt-side row filtering. The application will need to own the typed preference and include the effective listing policy in directory-read requests so stale results from the prior value cannot replace a newer listing.

## Decisions required from the owner

1. Confirm the recommended scope, or choose whether the first release also includes volume service directories and/or all macOS-invisible items.
2. Confirm the default: hide the defined housekeeping artifacts until **Show macOS housekeeping files** is enabled.
3. Confirm whether the proposed General checkbox label is acceptable, or provide preferred product wording.

## Sources

- Apple Support, [Adjust SMB browsing behaviour in macOS](https://support.apple.com/en-gb/102064), accessed 2026-10-10.
- Apple Developer Forums, [`ditto` and AppleDouble files](https://developer.apple.com/forums/thread/761587), accessed 2026-10-10. This is supporting platform discussion, not normative API documentation.
- Apple Developer Documentation, [File System Metadata Attribute Keys](https://developer.apple.com/documentation/coreservices/file_metadata/mditem/file_system_metadata_attribute_keys), accessed 2026-10-10.
