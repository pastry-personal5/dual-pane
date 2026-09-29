# P1-M1 overview: Architecture design and review

Status: Done

## Goal

Draft a target architecture for the project, check it against Clean Architecture's dependency rule, and resolve enough open questions to let P1-M2 start without redoing this design work.

## Scope

**In:**
- [architecture.md](../architecture.md): the target technical architecture.
- Deciding the minimum macOS and Qt 6 versions.
- Deciding the Clean Architecture dependency direction.
- Setting up [docs/roadmap.md](../roadmap.md) and this phase's plan docs.

**Out:**
- Any code. This milestone produces no crate, no `build.rs`, no C++.
- Product scope and behavior, now defined in [mvp.md](../mvp.md).
- Planning P1-M2 or any later milestone in detail.

## Completion checklist

- [x] [architecture.md](../architecture.md) records a coherent Clean Architecture boundary, has no phase or milestone plan, and has been reviewed.
- [x] The minimum macOS and Qt 6 versions are recorded in AGENTS.md under "Decided".
- [x] `docs/roadmap.md` exists and lists only Phase 1.
- [x] This phase's `phase.md` lists P1-M1 (this milestone) and P1-M2 (a one-line stub).
- [x] The gate passes: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- [x] `docs/README.md` indexes every active documentation file.
