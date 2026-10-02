# P3-M7 architecture

Status: Done

P3-M7 introduces a pure operation state machine in the domain and application rings. It prepares validated, typed requests and consumes typed results; P3-M8 supplies the native scanner, executor, and interface.

## Ownership and state

The domain owns byte-exact operation targets, operation IDs, kind and decision policy. The application owns the set of jobs, their generation tokens, confirmation and decision state, progress and terminal outcomes. A start command captures the confirmed source location and selected entry names immediately. Copy and move capture the other Browser's active tab and confirmed destination; a pending or failed destination is unavailable. Rename captures one entry and the proposed name; New Folder captures only its confirmed folder and proposed name. Whitespace-only names and source/destination equality reject without work.

Each accepted intent has a monotonically increasing workspace-local ID. Permanent deletion first enters a confirmation state with its frozen target count; only a matching explicit confirmation issues a scan. A scan result carries the operation ID and generation and an ordered plan whose recursive entries identify links as links. The application rejects plans outside the frozen roots, duplicate entries, missing roots, non-directory ancestors, and incorrect destination mapping. Progress counts planned entries; skipping a directory accounts for its unprocessed descendants. A step result either advances progress, stops at one typed decision, reports a name collision to an inline editor, or ends. A decision command must name the waiting operation, item, and unique decision token; only regular-file conflicts can use apply to all, and one failed automatic attempt on an item returns to a decision. Cancellation invalidates the generation, requests worker cleanup, and stays in Cancelling until the matching cleanup result arrives. The final outcome distinguishes clean cancellation, partial work, failure, and uncertain cleanup. Late results cannot reopen or retarget a cancelled or completed job.

Application output describes operation state, permitted choices, and progress without presentation wording. Work requests are typed scan, execute, and cancel requests, with immutable captured context. P3-M8 will adapt them to bounded workers and present them through Operation Panels and Notices; until then no UI gesture starts a job and the desktop runtime answers an accidental request with a typed executor-unavailable event. M8 also owns native identity checks, pre-mutation revalidation, a recovery-journal ID unique across launches, inline editor and Status Bar presentation, Notices, and affected-listing refresh.

## Verification

Exercise the state machine with Qt-free tests and synthetic scan/step results. Assert exact frozen targets after source and destination changes; both confirmation paths; invalid, stale, and cross-job decisions; conflict-only apply-to-all; recoverable-error choices; concurrent jobs; cancellation and partial outcomes. Then run the repository gate named in the overview.
