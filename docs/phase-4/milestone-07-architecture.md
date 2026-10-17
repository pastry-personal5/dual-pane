# P4-M7 architecture

Status: Planned

Drag and drop supplies a validated transfer intent to the same application-owned file-operation workflow used by Clipboard Paste. The [overview checklist](milestone-07-overview.md#completion-checklist) defines acceptance; P4-M1 fixes internal-only directions, targets, and Move/Option-Copy, while M7 planning settles edge cases and feedback.

## Payload and target

The Qt Folder Items view begins a drag only from the approved selection and emits a desktop-owned representation that preserves exact source identity without relying on display labels. A drop target maps to a Browser's confirmed folder, a folder row, or a Browser Tab's confirmed folder, including an inactive tab. Hovering an inactive tab does not activate it. The adapter reports the source snapshot, proposed action, target identity, and payload generation to the application. The application checks the target and creates the same frozen operation intent as P4-M4. A hover is preview feedback only; no file mutation occurs until an accepted drop produces a command.

Reject stale rows, changed source folders, external payloads, self and descendant targets, and unavailable destinations before work is submitted. Never infer safe move behavior from a visual modifier alone; the accepted M1 action and the application validation determine it. Do not follow symlinks recursively. The operation lane continues to own conflicts, atomic replacement, cross-volume moves, progress, and cleanup. Tests use synthetic drag events for state ordering and temporary-directory fixtures for file outcomes. Native checks verify target highlighting, modifier feedback, pointer cancellation, and rejection of Finder drags.
