# P1-M3 architecture: Inner crates for one-pane browsing

Status: Done

P1-M3 creates the domain, application, and adapter crates with the smallest public API that supports one pane listing a directory and changing directory. All three are plain Rust, tested without Qt, and not yet used by the desktop crate.

## Target boundary

```text
crates/
├── dual-pane-domain/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── location.rs          # Location, EntryName
│   │   ├── entry.rs             # Entry, EntryKind, listing sort order
│   │   ├── request_token.rs     # RequestToken
│   │   └── listing_error.rs     # ListingError, ListingErrorKind
│   └── tests/
│       ├── location_navigation.rs
│       └── listing_sort_order.rs
├── dual-pane-application/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── input.rs             # Input, Command, Event
│   │   ├── output.rs            # Output
│   │   ├── work_request.rs      # WorkRequest
│   │   └── workspace.rs         # Workspace, Transition, pane state
│   └── tests/
│       ├── directory_navigation.rs
│       ├── stale_listing_results.rs
│       └── support/
│           └── mod.rs           # location and entry builders
└── dual-pane-adapters/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs
    │   ├── input_controller.rs  # UiEvent → Command
    │   └── pane_presenter.rs    # Output → PaneViewModel, RowViewModel
    └── tests/
        ├── row_activation.rs
        └── listing_presentation.rs
```

Module files may be merged if one turns out to be trivially small; the list above is a ceiling, not a requirement. Test file names follow the observable behavior they cover. No `mod.rs` is used for source modules; `tests/support/mod.rs` is the documented exception for shared test builders.

Each manifest uses `edition.workspace = true` and `license.workspace = true` and declares only path dependencies on inner crates. External dependencies are declared once under `[workspace.dependencies]` and opted into with `.workspace = true`: `unicode-normalization = "=0.1.25"` is the domain crate's only normal dependency, used for the sort key; `proptest` is declared as `{ version = "=1.11.0", default-features = false, features = ["std"] }` and is a `[dev-dependencies]` entry of the domain and application crates.

## Domain

| Type | Rules |
|---|---|
| `EntryName` | A validated, exact entry name stored as the bytes the file system reports: non-empty, no `/`, no NUL, and not `.` or `..`. Construction returns `Result`. `as_bytes()` returns the exact bytes; `to_text_lossy()` returns a text rendering for display and sorting only. Equality and lookup always use the exact bytes, so two names that render alike remain distinct. |
| `Location` | An absolute, logical location stored as an ordered list of `EntryName` components; the empty list is the root. `root()`, `from_components(...)`, `components()`, `join(&EntryName) -> Location`, and `parent() -> Option<Location>` (`None` at the root). It performs no I/O and never resolves symbolic links, so the parent of a location entered through a link is the logical parent. Converting to and from a native path is the desktop gateway's job. |
| `EntryKind` | `Directory`, `File`, `Symlink { points_to_directory: bool }`, and `Other`. The gateway classifies the kind; the domain never inspects the file system. |
| `Entry` | `name: EntryName` and `kind: EntryKind`. No metadata columns in Phase 1. |
| Sort order | `listing_sort_key(&Entry) -> ListingSortKey`, an `Ord` key computed once per entry so a sort does not repeat text conversion on every comparison (for example `entries.sort_by_cached_key(listing_sort_key)`). Entries that can be entered (`Directory` or `Symlink { points_to_directory: true }`) come first. Names then compare in natural order on their lossy text after canonical composition (NFC), so names that differ only in Unicode composition sort together: the text is split into runs of ASCII digits and single other characters; digit runs compare by numeric value (without overflow, by length after leading zeros and then digit by digit), and other characters compare by their Unicode lowercase form in code-point order, not by a locale's collation. The exact bytes break every remaining tie, so the order is total and entries never merge. The desktop gateway computes keys and sorts on a worker; application and adapter code never sort. |
| `Entry::can_enter()` | True for `Directory` and `Symlink { points_to_directory: true }`. This is the policy that opening an entry navigates into it. |
| `RequestToken` | An opaque, copyable, comparable token created only through `RequestToken::first()` and `next()`. The application is the only issuer. |
| `ListingError` | `location: Location` and `kind: ListingErrorKind`, where the kind is `ItemMissing`, `NotADirectory`, `PermissionDenied`, `PrivacyRestricted`, or `Unknown`. Only the categories that change what the application or person can do are included; no native error object is stored. |

## Application

The application exposes one reducer and plain value types:

```rust
pub struct Workspace { /* private */ }

impl Workspace {
    pub fn new() -> Self;
    pub fn handle(&mut self, input: Input) -> Transition;
}

pub struct Transition {
    pub outputs: Vec<Output>,
    pub work: Vec<WorkRequest>,
}
```

`Workspace` holds one pane. The pane holds the committed location and its sorted entries (`Option<(Location, Arc<[Entry]>)>`, empty before the first successful listing) and an optional pending navigation (`RequestToken`, target `Location`). The single pane is private state, so adding a second pane and tabs in Phase 2 adds pane and tab identifiers to inputs and outputs without changing the meaning of existing variants. Read-only accessors `location()`, `entries()`, and `loading_location()` expose the pane state for the composition root and tests; they return references and never change state. `Arc<[Entry]>` lets outputs share a listing with the presenter without copying rows on the GUI thread.

| Input | Behavior |
|---|---|
| `Command::Navigate(Location)` | If the location is already being loaded, the transition is empty and the pending read continues. Otherwise it issues a new token, records the pending navigation, emits `Output::LoadingStarted { location }` and `WorkRequest::ReadDirectory { token, location }`. If a navigation was already pending, it also emits `WorkRequest::Cancel { token }` for the older token. Used by the composition root for the initial location. |
| `Command::OpenEntry(EntryName)` | Looks up the name in the committed listing. If it exists and `can_enter()`, behaves as `Navigate(current.join(name))`. Otherwise the transition is empty. Opening a file is outside this milestone. |
| `Command::GoToParent` | If the committed location has a parent, behaves as `Navigate(parent)`. At the root, or before the first listing, the transition is empty. |
| `Event::ListingLoaded { token, entries: Arc<[Entry]> }` | If `token` matches the pending token, commits the pending navigation's location with these entries, clears the pending navigation, and emits `Output::ListingReplaced { location, entries }`. The entries are already sorted by the gateway; `handle` does not sort or diff them. Any other result is ignored with an empty transition. |
| `Event::ListingFailed { token, kind: ListingErrorKind }` | If `token` matches the pending token, clears the pending navigation, keeps the committed location and listing, and emits `Output::ListingFailed { error }` whose location is the pending navigation's location. Any other token is ignored. |

Result events carry no location: the token alone identifies the request and therefore its location. A gateway that reports a location in another spelling (another Unicode form, case, or a resolved link) therefore cannot strand a pending navigation or attribute an error to the wrong folder.

Navigation replaces the whole listing. `Output::ListingReplaced` is the Phase 1 form of the [architecture's](../architecture.md#32-application--use-cases-and-application-state) listing output; insert, remove, and update deltas are added with same-location refresh in Phase 2. The directory-read request then gains the previous listing snapshot the architecture describes.

## Adapters

| Type | Responsibility |
|---|---|
| `UiEvent` | Action-named, key-free events: `ActivateRow { row: usize }` and `GoToParent`. The desktop driver later maps a double-click, a menu item, a toolbar button, or an eventual key binding to these. |
| `InputController` | `command(event, &PaneViewModel) -> Option<Command>`. `ActivateRow` maps the row index to the displayed entry's exact `EntryName`, never to its display text, and produces `Command::OpenEntry`; an out-of-range row produces `None`. It does not check `can_enter()`; the application decides. `GoToParent` maps to `Command::GoToParent`. |
| `PanePresenter` | Applies each `Output` to a `PaneViewModel`: location text (components joined with `/`, with `/` for the root), a loading flag, an optional error message, and the current listing. It owns error wording per `ListingErrorKind` and location. |
| `PaneViewModel` | Exposes `row_count()` and `row(index) -> Option<RowViewModel>`. Rows are formatted when requested, so the presenter's work per output is constant rather than proportional to the directory size. |
| `RowViewModel` | Display name (the entry's lossy text) and a row kind (`Folder`, `File`, `FolderLink`, `Link`, `Other`) for the later Qt icon choice. |

The presenter keeps only rebuildable rendering state. It never decides whether a row can be entered or whether a result is stale.

## Tests

- **Domain:** `location_navigation.rs` covers `join`, `parent`, the root, and logical parents through a link-named component. `listing_sort_order.rs` covers folders first, links to folders among folders, case-insensitive natural order, composed and decomposed accents sorting together, and the exact-name tie-break. `EntryName` validation, including names that are not valid UTF-8 and names that render alike, is covered by a unit test in `location.rs`. `listing_sort_order.rs` also has `proptest` properties that the order is reflexive, antisymmetric, and transitive; `location_navigation.rs` has a property that `join` then `parent` returns the original location.
- **Application:** `directory_navigation.rs` covers the initial navigation, successful listing, entering folders and links to folders, ignored non-enterable and unknown names, go-to-parent including the root, and failed listings keeping the previous state. `stale_listing_results.rs` covers superseded and unknown tokens for both result events, the cancel request for a superseded navigation, a repeated navigation to the location being loaded, and a failure reported at the location its token was issued for. A test feeds entries in a deliberately unsorted order and asserts they are output unchanged, proving that `handle` does not sort. A `proptest` property in `stale_listing_results.rs` generates sequences of navigations and loaded, failed, and stale results and checks that only the latest pending token commits, at most one navigation is pending, and failed or stale results never change the committed listing.
- **Adapters:** `row_activation.rs` covers row mapping, out-of-range rows, and go-to-parent. `listing_presentation.rs` covers location text, loading state, error wording for each kind, and on-demand rows.

No test creates, alters, or deletes file-system entries, so no temporary-directory dependency is needed. With its `std` feature, `proptest` reads, and after a failure writes, regression files in a `proptest-regressions/` directory beside the test sources.

## Implementation sequence

1. Add `proptest` and `unicode-normalization` to the workspace dependencies and record the license of every crate they add to `Cargo.lock`.
2. Add `dual-pane-domain` to the workspace with its types and tests; iterate with `cargo test -q -p dual-pane-domain`.
3. Add `dual-pane-application` with the reducer and tests; iterate with `cargo test -q -p dual-pane-application`.
4. Add `dual-pane-adapters` with the controller, presenter, and tests.
5. Collect the `cargo tree` and Qt-free test evidence, run the naming-rule grep, then run `make check` and the equivalent direct commands once.
6. Update AGENTS.md, README.md, `planned-repository-architecture.md`, and `phase.md`, and check the completed checklist items.

## Constraints and rejection criteria

- An inner crate that depends on Qt, CXX-Qt, a platform crate, or any external crate other than `unicode-normalization` in the domain crate and the `proptest` dev-dependency is rejected.
- `handle` must not perform I/O, spawn threads, call a callback, sort, or diff. It returns outputs and work requests only.
- The controller must not validate commands with file-manager policy, and the presenter must not hold behaviorally significant state.
- Using lossy name text for identity, lookup, or navigation, a `Location` built from a native path inside an inner crate, or a native error type in an inner crate is rejected.
- No change to `dual-pane-desktop`, and no speculative panes, tabs, selection, cursor, delta engine, filter, or file-operation types.
- `unwrap()` and `expect()` are allowed only in tests, as [AGENTS.md](../../AGENTS.md#code-conventions) requires.
