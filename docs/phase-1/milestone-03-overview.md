# P1-M3 overview: Inner crates for one-pane browsing

Status: Done

Add the three Qt-free inner crates (`dual-pane-domain`, `dual-pane-application`, and `dual-pane-adapters`) with only the behavior Phase 1 needs: one pane that lists a directory and changes directory. The desktop crate is not wired to them in this milestone; every behavior is proven by Qt-free Cargo tests.

## Goal

Implement and test the domain values, the `Workspace::handle` reducer, and the controller and presenter for one pane that requests a directory listing, shows the sorted folders and files, enters a listed folder, goes to the parent directory, rejects stale results, and reports a directory that cannot be listed.

## Scope

**In:**

- Create `crates/dual-pane-domain`, `crates/dual-pane-application`, and `crates/dual-pane-adapters` as library packages and add them to the workspace, with the dependency directions fixed by [planned-repository-architecture.md](../planned-repository-architecture.md#crate-rules).
- Domain: locations, entry names, entry kinds, entries, request tokens, the listing sort order, and the listing error categories. See the [architecture plan](milestone-03-architecture.md#domain).
- Application: one pane's state, its inputs (navigate, open an entry, go to parent, listing loaded, listing failed), its outputs, and its directory-read and cancel work requests.
- Adapters: an input controller that maps action-named UI events to application commands, and a presenter that turns application outputs into plain-Rust pane and row view-models, formatting rows on demand.
- Example-based unit and integration tests for each crate, plus `proptest` property tests for the domain sort order and location navigation and for the application's token and listing invariants, placed as [planned-repository-architecture.md](../planned-repository-architecture.md#test-source-layout-and-boundary-rules) requires.
- The documentation updates listed in the completion checklist.

**Out:**

- Any change to `dual-pane-desktop`: no dependency on the inner crates, no Qt item model, no worker runtime, and no file-system gateway. The next Phase 1 milestone wires the desktop crate, reads real directories on a worker, and shows the listing. That milestone, not this one, satisfies the Phase 1 exit criteria for on-screen listing and non-blocking loading.
- Two panes, the active pane, tabs, tab history, selection, and cursor. These arrive in Phase 2. This milestone is a subset of the [architecture's](../architecture.md#3-boundary-ownership) target workspace, shaped so that adding them is additive.
- Insert, remove, and update row deltas. Navigation replaces the whole listing; deltas arrive with same-location refresh and watching in Phase 2.
- Hidden-file filtering, other sort orders, metadata columns (size, dates), opening files, file operations, persistence, and watching.
- Any concrete key binding. UI events name actions, never keys.
- Any external dependency other than the approved `unicode-normalization` domain dependency and `proptest` dev-dependency.

## Prerequisites and decisions

- The user approved `proptest = "=1.11.0"` (MIT OR Apache-2.0) as a dev-dependency of the inner crates on 2026-09-30. It is declared once in the workspace manifest with `default-features = false, features = ["std"]`, which leaves out the `fork` feature and its `rusty-fork` and `tempfile` dependencies. The user also approved `unicode-normalization = "=0.1.25"` (MIT OR Apache-2.0) as a normal dependency of the domain crate on 2026-09-30, for the sort key only. No other dependency is added.
- The user decided these behaviors on 2026-09-30, recorded in the [phase changelog](changelog.md):
  1. **Failed navigation:** the pane stays on its previous directory and listing and reports the error.
  2. **Sort order:** folders, including symbolic links to folders, come first. Names then sort in natural order: runs of digits compare by numeric value (`file2` before `file10`), and other text compares case-insensitively after Unicode canonical composition, so names that differ only in how an accent is encoded sort together. The exact name breaks remaining ties.
  3. **Entry names:** an entry name is the exact byte sequence the file system reports. It is never changed; display and sorting use a text rendering that may show `�` for invalid UTF-8, but identity, lookup, and navigation always use the exact bytes.
  4. **Parent after a symbolic link:** locations are logical. Going to the parent of `/a/link` returns to `/a`, not to the link target's parent.

## Completion checklist

- [x] The behaviors and the `proptest` dependency under "Prerequisites and decisions" are decided by the user and recorded in the [phase changelog](changelog.md). Evidence: 2026-09-30 changelog entry.
- [x] The workspace members are `dual-pane-desktop`, `dual-pane-domain`, `dual-pane-application`, and `dual-pane-adapters`. `dual-pane-desktop` is unchanged. Evidence: root manifest lists the four members; `git diff --stat` shows no change under `crates/dual-pane-desktop`.
- [x] Dependency directions match the crate rules: domain has no Dual Pane dependency; application depends only on domain; adapters depend only on application and domain; the only external normal dependency is `unicode-normalization` in the domain crate, and the only external dev-dependency is `proptest` with default features disabled. No crate pulls in `cxx`, `cxx-qt`, or `cxx-qt-lib`. Evidence: 2026-09-30 `cargo tree -p <crate> -e normal,dev --depth 1`: domain has `unicode-normalization` and the `proptest` dev-dependency; application has `dual-pane-domain` and the `proptest` dev-dependency; adapters have `dual-pane-application` and `dual-pane-domain`.
- [x] Every crate that `proptest` or `unicode-normalization` adds to `Cargo.lock` has a license compatible with Apache-2.0. Evidence: 2026-09-30 license list of the packages this change adds to `Cargo.lock` — `proptest` 1.11.0, `bitflags`, `getrandom`, `num-traits`, `ppv-lite86`, `rand`, `rand_chacha`, `rand_core`, `rand_xorshift`, `regex-syntax`, and `unarray` are MIT OR Apache-2.0; `autocfg` is Apache-2.0 OR MIT; `zerocopy` and `zerocopy-derive` are BSD-2-Clause OR Apache-2.0 OR MIT; `wasip2` and `wit-bindgen` are Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT; `r-efi` is MIT OR Apache-2.0 OR LGPL-2.1-or-later, used under MIT or Apache-2.0. `unicode-normalization` 0.1.25 is MIT OR Apache-2.0 and its dependency `tinyvec` is Zlib OR Apache-2.0 OR MIT. `cfg-if` and `libc` were already locked.
- [x] Domain tests prove location joining and parent behavior (including the root and logical symbolic-link parents), byte-exact entry-name validation (including names that are not valid UTF-8), and the natural listing sort order, including composed and decomposed accents sorting together. Property tests show that the sort order is a total order and that `join` followed by `parent` returns the original location. Evidence: `cargo test -q -p dual-pane-domain` passed 18 tests on 2026-09-30.
- [x] Application tests prove that navigating issues a directory-read request with a new token; a matching result replaces the listing; a failed read keeps the previous listing and reports a typed error with its location; a superseded or unknown token changes nothing; a failure is reported at the location its token was issued for; a newer navigation cancels the pending read; navigating again to the location being loaded keeps the pending read; opening a folder or a symbolic link to a folder navigates into it; opening any other entry kind, or a name not in the current listing, changes nothing; going to the parent of the root changes nothing; and `handle` never sorts a result. A property test over generated input sequences shows that only the latest navigation's token commits a listing, at most one navigation is pending, and a failed or stale result never changes the committed listing. Evidence: `cargo test -q -p dual-pane-application` passed 18 tests on 2026-09-30.
- [x] Adapter tests prove that row activation and go-to-parent events map to the right commands (and that an out-of-range row maps to none), and that the presenter produces location text, loading state, error wording, and on-demand row view-models without duplicating application policy. Evidence: `cargo test -q -p dual-pane-adapters` passed 9 tests on 2026-09-30.
- [x] The three crates build and test without Qt. Evidence: `env -u QMAKE` with `PATH=/usr/bin:/bin:~/.cargo/bin` (no `qmake` reachable) ran all three crates' tests successfully on 2026-09-30.
- [x] No source-code name uses a phase word, and no source code or script uses `MVP` wording, as required by [AGENTS.md](../../AGENTS.md#code-conventions). Evidence: `rg -n -i 'phase|mvp' crates scripts Makefile`, which also searches untracked files, printed nothing on 2026-09-30.
- [x] No UI event, command, or test names a concrete key. Evidence: `UiEvent` has only `ActivateRow { row }` and `GoToParent`.
- [x] `make check` and the equivalent direct commands (`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make lint-cpp`, `cargo test`) pass from the workspace root. Evidence: on 2026-09-30, `make check`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make lint-cpp`, and `cargo test` each exited 0.
- [x] Documentation is current: the "Project" line in [AGENTS.md](../../AGENTS.md), [README.md](../../README.md) where it describes the workspace, the "Change sequence" section of [planned-repository-architecture.md](../planned-repository-architecture.md#change-sequence), and this milestone's status in [phase.md](phase.md). Evidence: this change updates all four.
