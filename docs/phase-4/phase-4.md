# Phase 4: Everyday File Workflows

Status: Active
Goal: Help people find, navigate to, and arrange files through tab-local text filtering, clipboard transfers, direct and visual path navigation, and safe drag and drop.

Phase 3 delivered the underlying Browser, selection, navigation, and file-operation workflows. Phase 4 has completed P4-M1, the Qt-free filtering model in P4-M2, the clipboard-transfer model in P4-M4, and clipboard delivery in P4-M5; P4-M3 remains active. Phase 4 does not change the safeguards in [product behavior](../product-behavior.md#file-operation-safeguards).

## Scope and sequence

The first milestone records the feature boundaries and safety decisions that all later milestones need. The owner requires file Copy and Paste in both directions between Dual Pane and Finder, while Cut as a move and drag and drop remain internal; see [P4-M1](milestone-01-overview.md#owner-decisions). Finer interaction choices are [assigned to M2–M8](milestone-01-overview.md#deferred-owner-decisions) for owner review before each implementation starts.

The intended feature set is:

- Narrow the current tab's Folder Items by a typed name filter without changing the folder on disk or turning filtering into recursive search.
- Copy, Cut, and Paste files using the macOS clipboard and the existing safe file-operation workflow. File Copy works between Dual Pane and Finder in both directions. The current Copy/Move to Other Browser commands stay available.
- Enter a folder path in the existing Path Edit Control and navigate through the ordinary tab history and error behavior.
- Click an ancestor component of the confirmed absolute path to navigate the active tab. Expand the control vertically to browse available folder options in columns, then collapse it without changing the active path.
- Drag selected Folder Items to supported targets in the workspace, using the same copy/move decisions, progress, cancellation, and data-safety rules as menu or keyboard commands.

Phase 4 supports file Copy between Dual Pane and Finder in both directions. Finder-sourced Paste into Dual Pane is a Copy through the existing safeguarded operation workflow, never a move of Finder's source. This does not export Cut as a Finder move or support drags to or from Finder. Unsupported or malformed clipboard content, and external drag payloads, must not start a Dual Pane operation.

## Exit criteria

- Each Browser Tab can show and clear a text-filtered view of its current Folder Items; sort, selection, cursor, count, refresh, and tab switching remain coherent. Selection-based commands capture only explicitly selected visible targets when invoked; a previously captured clipboard intent or running operation keeps its frozen targets.
- Copy in Dual Pane can be pasted into Finder while the clipboard is unchanged, including after Dual Pane quits or relaunches; Finder performs the resulting copy and the source remains intact. Files copied in Finder can be pasted into Dual Pane as a safe Copy with unchanged sources. Paste within Dual Pane initiates safe file operations with a frozen source and destination at command start. Conflicts, progress, cancellation, partial results, and refresh use the existing operation workflow and [data-safety invariants](../architecture.md#44-data-safety-invariants).
- The Path Edit Control accepts a folder path, navigates when a confirmed breadcrumb component is activated, and expands into a bounded-height column browser for available folders. It gives clear validation and navigation feedback without replacing the last successful Folder Items when navigation fails.
- Drag and drop uses explicit supported source and target types and a visible copy or move outcome; a rejected or cancelled drop changes no files.
- The new controls and commands work by pointer and keyboard where appropriate, have accessible names and feedback, and preserve the existing shortcut editor's conflict rules.
- All milestone checklists and the full repository gate pass. Native checks use a disposable profile and temporary file data.

## Other ideas considered

| Idea | Place in this plan |
|---|---|
| Better empty and no-match states, status counts, focus return, and shortcut discoverability | Included as acceptance details in the related feature milestones. |
| Broader large-folder responsiveness work | Kept as a later phase candidate; this phase still requires its new interactions to stay responsive. |
| Recursive search, archive browsing, or a built-in viewer | Outside this phase; archive browsing and the built-in viewer remain excluded by product behavior. |
| Remote-volume or packaging work | Outside this phase; current product scope excludes remote file systems, and packaging tooling is undecided. |

## Milestones

Milestones run in the order below. P4-M1 and P4-M2 are complete; no feature implementation milestone is active.

### P4-M1: Decide interaction and safety behavior

Status: Done
Goal: Record owner-approved feature boundaries and safety rules for filtering, clipboard transfer, path entry, and drag and drop, with finer interactions assigned to their feature milestones.
Plan: [overview](milestone-01-overview.md), [architecture](milestone-01-architecture.md)

### P4-M2: Model tab-local text filtering

Status: Done
Goal: Add Qt-free filter state and a safe visible-items projection without changing stored Folder Items or operation targets.
Plan: [overview](milestone-02-overview.md), [architecture](milestone-02-architecture.md)

### P4-M3: Deliver text filtering UI

Status: Active
Goal: Add an accessible filter control and feedback to each Browser, wired to the tab-local state.
Plan: [overview](milestone-03-overview.md), [architecture](milestone-03-architecture.md)

### P4-M4: Model clipboard transfer safely

Status: Done
Goal: Turn Copy, Cut, and Paste intents into validated, frozen requests for the existing file-operation workflow.
Plan: [overview](milestone-04-overview.md), [architecture](milestone-04-architecture.md)

### P4-M5: Deliver clipboard commands

Status: Done
Goal: Expose Copy, Cut, and Paste in the desktop interface, including file Copy and Paste between Dual Pane and Finder in both directions.
Plan: [overview](milestone-05-overview.md), [architecture](milestone-05-architecture.md)

### P4-M6: Navigate by entered path

Status: Planned
Goal: Make the Path Edit Control editable with safe validation, history integration, and clear failure feedback.
Plan: [overview](milestone-06-overview.md), [architecture](milestone-06-architecture.md)

### P4-M7: Drag and drop file items

Status: Planned
Goal: Add file drag and drop for approved sources and targets through the same safe operation workflow.
Plan: [overview](milestone-07-overview.md), [architecture](milestone-07-architecture.md)

### P4-M8: Advanced Path Edit Control

Status: Planned
Goal: Turn the confirmed path into clickable components and add an expandable, column-based folder navigator inside each Browser.
Plan: [overview](milestone-08-overview.md), [architecture](milestone-08-architecture.md)
