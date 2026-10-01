# P2-M4 architecture: Pane-local listing execution

Status: Done

The desktop runtime routes each pane's reads to a dedicated supervisor and queue. The two supervisors publish to one GUI event channel with one coalesced wake flag, preserving the existing bounded `WorkspaceSession` drain and application event contract.

## Runtime boundary

`Runtime` owns left and right job senders. Each supervisor processes its pane's queue in order and replaces a failed worker after bounded backoff. A job records its pane and global request token. Outstanding state stores the same pair, so a cancellation with a mismatched pane cannot claim another pane's terminal event. If the second supervisor cannot start, startup returns the error and drops the first sender.

## Verification boundary

Channel gates hold a read in one pane while the other completes. Runtime tests cover queue closure, cancellation, terminal claims, worker failure and panic recovery. `WorkspaceSession` tests drive pane-addressed results and inspect both presenters after each transition, including a small drain slice. A late or wrong-pane event must leave the current listing and revision intact.
