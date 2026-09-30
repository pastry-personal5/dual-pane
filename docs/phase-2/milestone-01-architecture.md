# P2-M1 architecture: Decide two-pane interaction details

Status: Planned
This documentation-only milestone adds no production types, bridge surfaces, or behavior. Its output is the agreed interaction specification that later milestones implement.

## Approach

1. Keep the complete interaction decision in the P2-M1 overview while the milestone is planned, so its checklist is the executable definition of done.
2. When executing P2-M1, record the user-facing rules in [mvp.md](../mvp.md), record resolved bindings under "Decided" in [AGENTS.md](../../AGENTS.md), and add a concise owner-decision entry to [changelog.md](changelog.md).
3. Keep this milestone independent of P2-M2 through P2-M4: it names application-level intents such as pane activation and row activation, but does not select Rust type names, application state layout, Qt widgets, bridge surfaces, or runtime mechanics.
4. Verify the documentation links and run the required formatter, linter, and test gate before marking P2-M1 Done.
