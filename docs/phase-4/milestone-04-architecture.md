# P4-M4 architecture

Status: Planned

Clipboard transfer is another entry point to the existing application-owned file-operation workflow. The [overview checklist](milestone-04-overview.md#completion-checklist) defines acceptance; P4-M1 fixes the transfer and safety boundary, and M4 planning settles repeat and preflight details.

## Transfer intent

Add typed Copy, Cut, Paste, and clipboard-change inputs in `crates/dual-pane-application/src/input.rs`. The application captures source location, exact selected Item names and kinds, and the clipboard generation or representation accepted at that moment. A Cut is a pending move intent, not an immediate filesystem mutation. Paste validates the active tab's confirmed destination and creates an `OperationIntent` using the same safety checks as Copy/Move to Other Browser. An operation keeps its frozen intent after it starts; a later clipboard replacement affects only a future Paste.

Do not make the clipboard a second file-operation executor. Existing `operations.rs`, `operation_lane.rs`, `operation_step.rs`, and the safety journal continue to own scan, conflict decisions, replacement, cross-volume moves, cancellation, and cleanup. If the current operation API assumes the other Browser as destination, extend its application-owned request narrowly to accept a validated destination. Do not derive a source from a visible row label or follow symlinks during recursive work.

The desktop adapter may report that clipboard content changed or supply a typed, byte-exact file-source snapshot, but the application validates it before producing work. A valid private representation supplies Dual Pane's Copy or Cut intent. A Finder file Copy supplies external sources for a Copy intent only; it cannot authorize a move or deletion. Validate source identities and kinds, the clipboard generation, and the active tab's confirmed destination before creating the request. Reject malformed, unavailable, and non-file payloads, and freeze source and destination when work starts. Finder file references can remain eligible after Dual Pane relaunches while the clipboard is unchanged, unlike private intent. For Copy in Dual Pane, expose the validated, frozen source paths to the desktop so M5 can publish a Finder-compatible representation without starting an operation. Qt-free tests use synthetic clipboard events and temporary-directory operation fixtures; they prove that source deletion follows only a verified move and that stale or replaced clipboard contents cannot retarget an active job.
