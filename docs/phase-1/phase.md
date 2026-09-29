# Phase 1: Foundations

Status: Active
Goal: Prove the Rust/Qt stack works end to end.

## Exit criteria
- To be refined as later milestones are planned; only the milestones below are planned in detail so far.

## Milestones

### P1-M1: Architecture design and review
Status: Done
Goal: Draft and review a target architecture for the project, check it against Clean Architecture's dependency rule, and resolve enough open questions to unblock P1-M2.
Plan: [overview](milestone-01-overview.md), [architecture](milestone-01-architecture.md)
Notes: Produced [architecture.md](../architecture.md). The current product scope is [mvp.md](../mvp.md).

### P1-M2: Stack spike
Status: Done
Goal: A Cargo-only build that links Qt Widgets and shows an empty window, conforming to [architecture.md](../architecture.md).
Plan: [overview](milestone-02-overview.md), [architecture](milestone-02-architecture.md)
Notes: `make check` passed with Qt 6.11.2 and Homebrew LLVM 23.1.2. Manual `make run` evidence confirmed the empty `Dual Pane` window and normal process return on close.
