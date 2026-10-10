# P3-M15 architecture: Hide macOS housekeeping items

Status: Planned

P3-M15 makes a narrowly defined display policy an application-owned setting. The application decides the effective policy and refreshes affected locations; the desktop worker applies the byte-level predicate while reading. Qt renders the setting and never owns the filter or durable state.

## Ownership and data flow

Add a typed preference to `SettingsState` and `SettingsSnapshot` with the owner-recorded default. Its command is accepted only after settings load has completed, changes effective state immediately, increments the existing settings revision, and schedules the ordinary settings save. The SQLite settings driver gains one versioned migration and round-trips the value with the existing snapshot; reset seeds the compiled default.

The preference is projected through `WorkspaceChrome`, the presenter, bridge, and P3-M14 General Settings page. The checkbox sends only the typed command and rerenders from the projection. A rejected or unavailable interaction leaves the current effective value unchanged. Existing persistence failure behavior remains authoritative: in-memory state survives for the launch, Settings becomes unavailable, and Notices exposes recovery.

Extend the application’s directory-read work contract with an immutable listing-policy value, rather than having the desktop read mutable settings state. On a valid preference change, restart every open tab’s read at its requested location. Each read therefore carries both the current request token and policy; the existing token checks reject stale earlier-policy results. The normal Folder Items reload path reconciles selection and cursor against the entries that remain visible.

## Desktop filtering

Compose two worker-local predicates at the existing `ListingRules` seam:

1. `is_launch_temporary` always suppresses only the current launch’s exact operation temporary pattern.
2. The approved housekeeping matcher suppresses its byte-exact names and prefixes only when the preference says not to show them.

Apply the composed predicate before `Entry` construction, metadata reads, package recognition, and sorting. This avoids rows, icons, preview requests, or display-derived operations for omitted items and preserves lossless byte-name behavior. The matcher must compare raw bytes, so `Icon` plus carriage return and a non-UTF-8 sibling do not rely on lossy text conversion.

The listing predicate is not passed to operation scans or executors. A visible artifact stays an ordinary entry; an omitted artifact is never deleted or otherwise modified by the feature. The established operation-temporary predicate remains active even when housekeeping items are revealed.

## Verification and sequencing

First record the owner decisions. Then add the application value, command, snapshot, migration, and temporary-directory tests; extend the work request and application refresh behavior; wire the desktop predicate and its unit tests; finally expose the General checkbox and run bridge/desktop/native checks. The complete repository gate remains the milestone exit evidence.
