# P2-M1 architecture: Decide two-pane interaction details

Status: Done
This documentation-only milestone adds no production types, bridge surfaces, or behavior. Its output is the agreed interaction specification that later milestones implement.

## Approach

1. Keep the canonical user-facing interaction decision in [mvp.md](../../../mvp.md#interaction); the overview links to it and keeps only implementation-neutral requirements and acceptance evidence.
2. Record resolved bindings under "Decided" in [AGENTS.md](../../../../AGENTS.md), and add a concise owner-decision entry to [changelog.md](changelog.md).
3. Keep this milestone independent of P2-M2 through P2-M4: it names application-level intents such as pane activation and row activation, but does not select Rust type names, application state layout, Qt widgets, bridge surfaces, or runtime mechanics.
4. Verify the documentation links and run the required formatter, linter, and test gate before marking P2-M1 Done.
