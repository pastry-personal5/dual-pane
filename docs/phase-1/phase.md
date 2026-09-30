# Phase 1: Foundations

Status: Active
Goal: One pane lists a directory's folders and files and lets a person change directory.

## Exit criteria
- The application window shows one pane that lists the folders and files in its current directory.
- A person can change the pane's directory by entering a listed folder and by going to the parent directory, and the listing updates to the new directory.
- Listing a directory does not block the interface, and a directory that cannot be listed is reported to the person instead of failing silently.

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

### P1-M3: Inner crates for one-pane browsing
Status: Done
Goal: Add the Qt-free domain, application, and adapter crates with tested one-pane listing and directory-change behavior.
Plan: [overview](milestone-03-overview.md), [architecture](milestone-03-architecture.md)
Notes: The three crates depend only on the standard library, each other, and the `proptest` dev-dependency. The desktop crate is not wired to them yet.

### P1-M4: Qt listing model and worker delivery
Status: Done
Goal: Show one pane's listing in a Rust Qt list model, produced on a bounded worker and delivered to the GUI thread in coalesced, bounded slices, using a 100,000-row synthetic source.
Plan: [overview](milestone-04-overview.md), [architecture](milestone-04-architecture.md)
Notes: `make check` passed after `cxx` was bumped to 1.0.202 to match CXX-Qt's code generator. Manual `make run` evidence confirmed the 100,000-row synthetic listing, smooth scrolling, and normal process return on close.

### P1-M5: Real directory browsing in one pane
Status: Planned
Goal: Replace the synthetic listing with real directories and let a person change directory, completing the Phase 1 exit criteria.
Plan: [overview](milestone-05-overview.md) (partial; the architecture doc is written before implementation)
