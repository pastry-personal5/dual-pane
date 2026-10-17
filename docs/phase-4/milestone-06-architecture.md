# P4-M6 architecture

Status: Planned

Path editing supplies a typed navigation request to the existing tab workflow. The [overview checklist](milestone-06-overview.md#completion-checklist) defines acceptance; P4-M1 fixes syntax and basic draft behavior, while M6 planning settles validation and focus details.

## Path input and navigation

The Qt Path Edit Control keeps its draft text and focus locally while editing. On submit, `browser_session.rs` sends a typed command through the controller; the application parses or validates the representation as far as it can without I/O, then requests a worker read through the existing navigation path. Filesystem existence, directory kind, access, and symlink resolution stay off the GUI thread. The application's request token rejects a result superseded by a newer navigation. A successful read commits the location and history; a failed read retains the last successful listing and gives the approved editor and status feedback.

Keep `Location` byte-exact. The visible path text may be a lossy display string today, so the design must define how the editor round-trips a displayed path containing undecodable bytes without silently navigating to a different path. Use a native path entry or explicit rejection rather than inventing a display-string identity. Tests separate pure parsing rules from temporary-directory checks and exercise a delayed result after tab switching.
