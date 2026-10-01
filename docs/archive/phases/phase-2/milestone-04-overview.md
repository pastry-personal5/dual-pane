# P2-M4 overview: Verify cross-pane independence and recovery

Status: Done

This milestone makes directory reads independent across the two Browsers and verifies pane-local loading, errors, cancellation, and late-result handling with automated tests.

## Goal

A blocked or failed read in one Browser must not delay or replace a valid listing in the other Browser.

## Scope

Use one supervised listing queue and worker per pane while retaining the shared GUI event receiver, coalesced wake, global tokens, and bounded drain. Keep application commands and events unchanged. Verify behavior at the runtime and `WorkspaceSession` boundaries without GUI automation or new dependencies.

## Completion checklist

- [x] Each pane has its own listing queue and supervised worker; a blocked read in one pane does not delay the other pane, and startup failure closes any queue already started. Evidence: `blocked_left_read_does_not_delay_right_read` and `startup_closes_first_queue_when_second_supervisor_fails` passed on 2026-10-01.
- [x] Runtime cancellation requires both pane and token, and each accepted read has exactly one terminal result through cancellation, failure, and worker recovery. Evidence: the wrong-pane, queued/running, late-result, queue-closure, worker-failure, and panic tests passed on 2026-10-01.
- [x] Automated runtime tests cover blocked reads, pane-local cancellation, failure and panic recovery, wrong-pane cancellation, late results, coalesced wake, and bounded delivery. Evidence: all 20 runtime tests passed on 2026-10-01.
- [x] `WorkspaceSession` tests assert both panes' paths, rows, selection, status, and listing revisions through loading, failure, cancellation, stale results, and bounded draining. Evidence: all six session tests passed on 2026-10-01.
- [x] Phase 2 exit criteria are supported by P2-M1 through P2-M4 evidence; `make check`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed on 2026-10-01.
