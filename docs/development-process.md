# Development process

Status: Active

Development proceeds in numbered **phases**, and each phase is split into numbered **milestones**. This doc defines how phases and milestones work and where their plans live. General doc rules are in [AGENTS.md](../AGENTS.md) under "Documentation".

## Phases and milestones

- **Phases** are numbered Phase 1, Phase 2, Phase 3, and so on. A phase is a coherent product increment. It has a one-sentence goal and a list of exit criteria.
- **Milestones** are numbered Milestone 1, 2, 3, and so on. Numbering restarts in each phase. A milestone is a small, verifiable step toward its phase's goal. It should fit in one reviewable change or a short series of changes.
- **IDs:** `P<phase>-M<milestone>`. For example, `P1-M2` is Phase 1, Milestone 2. Put the ID in PR titles and in commit message bodies.
- **IDs are stable.** Never renumber a milestone. New milestones get the next free number, even when they will run before an existing one. A dropped milestone keeps its ID. Mark it `Dropped` with a one-line reason instead of deleting it.
- **One active phase at a time.** Inside it, work on milestones in order unless the phase doc says otherwise.

### Status values

| Item      | Values                                  |
|-----------|-----------------------------------------|
| Phase     | `Planned`, `Active`, `Done`             |
| Milestone | `Planned`, `Active`, `Done`, `Dropped`  |

### Definition of done

A **milestone** is done when all of these hold:

1. Every item in its completion checklist is checked with the required acceptance evidence.
2. The gate in [AGENTS.md](../AGENTS.md) (`make check`) passes.
3. Docs are updated in the same change, including the milestone's status in its phase doc.

A **phase** is done when every milestone is `Done` or `Dropped` and the phase's exit criteria hold.

## Where plans live

Each phase gets its own directory, created when that phase is planned.

| File                                          | Contents |
|------------------------------------------------|----------|
| `docs/roadmap.md`                               | Every phase, one line each: ID, title, status, and a link to its phase doc. This is the only place that says which phase is active. |
| `docs/phase-N/phase-N.md`                       | The phase doc: goal, exit criteria, and short entries for its milestones. |
| `docs/phase-N/milestone-NN-overview.md`         | One milestone's goal, scope, and completion checklist, in detail. |
| `docs/phase-N/milestone-NN-architecture.md`     | The technical approach for implementing that milestone. |
| `docs/phase-N/changelog.md`                     | A chronological log of decisions, owner calls, and design changes made during the phase, most recent entry first. |

`N` is the phase number (`phase-1`, `phase-2`, …). Every phase overview filename repeats that number: `docs/phase-N/phase-N.md`; generic `phase.md` filenames are not allowed. `NN` is the milestone number, zero-padded to two digits (`milestone-01`, `milestone-02`, …), matching the `M` in its `P<phase>-M<milestone>` ID.

A phase doc looks like this:

```markdown
# Phase 1: <title>

Status: Active
Goal: <one sentence>

## Exit criteria
- <observable outcome>

## Milestones

### P1-M1: <title>
Status: Done
Goal: <one sentence>
Plan: [overview](milestone-01-overview.md), [architecture](milestone-01-architecture.md)
Notes: <decisions made, links to PRs; optional>
```

Keep milestone entries in the phase doc short — a link to its overview and architecture docs, not the design itself.

### Before starting a milestone

Write its two plan docs first:

- **`milestone-NN-overview.md`**: the milestone's goal, scope (what's in and out), and verifiable completion checklist.
- **`milestone-NN-architecture.md`**: the technical approach — affected modules, new types or bridge surfaces, and any sequencing within the milestone.

Link both from the milestone's entry in the phase doc. Decide any raised "Undecided" item from AGENTS.md first.

### Changelog

Each phase directory has a `changelog.md`. Add a short entry for anything a commit message won't make obvious: an owner decision, a design change from a milestone's architecture doc, an item moved from "Undecided" to "Decided", or a dropped or reordered milestone. Newest first:

```markdown
# Phase 1 changelog

Status: Active

Chronological record of decisions, owner calls, and design changes made during Phase 1. Newest entry first.

## Entries

- 2026-09-28 — <who> — <what changed and why, with a link to the PR/doc if any>
```

## Doc status and archiving

Placement, format, and archiving steps are in [AGENTS.md](../AGENTS.md) under "Documentation".

- **Status values** for docs other than phase plans: `Draft`, `Proposal`, `Active`, `Superseded by <link>`, or `Archived`.
- **Archive** a completed phase plan, a replaced design, or a spike whose conclusions became decisions, and fix its links in `docs/roadmap.md`.
- **Proposals are not decisions.** Record answered open questions under "Decided" in AGENTS.md.
