# Planned repository architecture

Status: Active

This document turns the four-crate boundary in [architecture.md](architecture.md) into a repository layout and naming convention. It defines the eventual workspace target and the P1-M2 migration direction; it does not itself create the crates.

## Directory layout rules

- The repository root is a virtual Cargo workspace. It owns workspace-wide Cargo configuration, shared development tooling, top-level documentation, and repository metadata; it contains no Rust package or executable source.
- The current root package and `src/main.rs` are a temporary scaffold, not an exception to this target. P1-M2 will replace that package with the virtual workspace and move the executable responsibility to `crates/dual-pane-desktop/`; no root-level `src/` or `tests/` directory remains after that conversion.
- All Rust packages live directly below `crates/`. There is no shared `common`, `core`, `util`, or catch-all crate: code belongs in the innermost ring that owns its policy, or in the outer component that owns its technology.
- A crate owns its source, crate-local tests, and any implementation detail that must not cross its public boundary. Tests that exercise public behavior from outside the crate live in that crate's `tests/` directory.
- A test directory or fixture is created only with its first test. The tree below shows the eventual homes of representative test sources; it does not authorize empty directories or speculative test files.
- C++ source, headers, Qt bridge support, and `build.rs` are private implementation details of `dual-pane-desktop`. No other crate may contain Qt, CXX-Qt, Objective-C, or macOS-specific source.
- Application fixtures and test helpers remain local to the test suite that needs them unless more than one crate needs the same framework-neutral fixture. A shared test helper must still obey the dependency rule; do not create a production utility crate merely to share tests.
- New directories are created only when they give a real ownership or build boundary. In particular, do not pre-create empty module, adapter, driver, resource, or test directories.

## Target directory layout

This is the target tree once each crate and its first representative test have earned a reason to exist. Entries ending in `/` are directories. Omitted directories do not exist until their owning crate needs them.

```text
.
├── Cargo.toml                         # virtual workspace manifest
├── Cargo.lock
├── crates/
│   ├── dual-pane-domain/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │       └── lib.rs
│   │   └── tests/
│   │       └── operation_policy.rs       # public domain behavior
│   ├── dual-pane-application/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │       └── lib.rs
│   │   └── tests/
│   │       ├── stale_listing_results.rs  # token and state behavior
│   │       └── support/
│   │           └── mod.rs                # application-test fakes/builders
│   ├── dual-pane-adapters/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │       └── lib.rs
│   │   └── tests/
│   │       └── presenter_output.rs       # translation without policy
│   └── dual-pane-desktop/
│       ├── Cargo.toml
│       ├── build.rs                    # created with the CXX-Qt bridge
│       ├── src/
│       │   └── main.rs                 # composition root and executable entry point
│       ├── cpp/                        # created with C++ Qt shim source
│           ├── include/dual_pane_desktop/
│           └── src/
│       └── tests/
│           └── qt_window_smoke.rs       # only if desktop exposes a testable library API
├── docs/
├── AGENTS.md
├── README.md
└── LICENSE
```

`dual-pane-desktop` may add `src/lib.rs` only when a meaningful desktop component needs integration or smoke-test access without starting the executable. `main.rs` remains the sole composition root; it may call a narrowly scoped desktop library entry point but must not become a second wiring location.

P1-M2 is intentionally smaller than this target tree. Its plan creates the virtual workspace, `dual-pane-desktop`, and only the bridge or C++ files needed to show the empty Qt window. It does not create the three inner crates merely to make the tree look complete. The plan must also state whether the spike's evidence is a desktop unit test, a desktop library-backed smoke test, or manual launch evidence before it creates a test target.

## Crate rules

Cargo package and directory names are identical. Their Rust crate identifiers use Cargo's normal hyphen-to-underscore conversion.

| Package and directory | Rust crate identifier | Ring | Direct Dual Pane dependencies | Owns |
|---|---|---|---|---|
| `dual-pane-domain` | `dual_pane_domain` | Domain | None | Platform-neutral value types, invariants, policies, and stable errors. |
| `dual-pane-application` | `dual_pane_application` | Application | `dual-pane-domain` | Input boundary, workspace state, use cases, application outputs, and application-owned ports. |
| `dual-pane-adapters` | `dual_pane_adapters` | Interface adapters | `dual-pane-domain`, `dual-pane-application` | Controllers, presenters, gateway translations, and event ingress. |
| `dual-pane-desktop` | `dual_pane_desktop` | Frameworks, drivers, composition root | The three inner crates | Qt delivery, CXX-Qt bridge, macOS and settings drivers, worker runtime, and executable wiring. |

These rules apply to every crate:

- The dependency graph in the table is exhaustive. A crate may not depend on an outer-ring crate, create a cycle, or bypass a ring by importing another crate's private implementation module.
- `dual-pane-domain`, `dual-pane-application`, and `dual-pane-adapters` are libraries. `dual-pane-desktop` is the only binary package and the only crate allowed to start the Qt event loop.
- Each package declares the dependencies it uses. Versions and source locations shared by multiple packages belong in the workspace manifest; package manifests opt into them explicitly. Adding or changing a dependency still requires the approval required by [AGENTS.md](../AGENTS.md).
- Default visibility is private. A public item is a deliberate boundary: expose only the types, constructors, traits, and functions another crate needs, and keep inner state and technology details crate-private.
- Ports are declared in `dual-pane-application`, because the application owns the needs they express. Their concrete implementations live in `dual-pane-desktop`; adapters may translate values at either side but do not move port ownership outward.
- A module tree follows ownership, not the four ring names. For example, the application crate may add modules for inputs, outputs, ports, and workspace behavior when those concepts acquire code, but it must not mirror every table row or create one-file abstractions without a boundary to protect.
- `dual-pane-desktop` may depend on Qt, CXX-Qt, and macOS implementation dependencies. Those types must be translated before values cross into adapters or application code.

## File and module naming rules

- Rust package directories and Cargo package names use lowercase kebab-case: `dual-pane-application`. Rust source files, modules, functions, variables, and test names use `snake_case`. Rust types and traits use `PascalCase`; constants use `SCREAMING_SNAKE_CASE`.
- `lib.rs`, `main.rs`, and `build.rs` are the only special Rust filenames. Otherwise, a module named `directory_listing` starts as `src/directory_listing.rs`. If it grows children, retain that entry file and place children below `src/directory_listing/` (for example, `src/directory_listing/request.rs`); do not use `mod.rs` for new modules.
- Test source names describe the behavior or boundary under test, not an implementation ticket or generic category: for example, `tests/stale_listing_results.rs` rather than `tests/test1.rs` or `tests/misc.rs`. Test fixtures use the same `snake_case` convention.
- C++ source and header filenames use `snake_case` with `.cpp` and `.hpp` extensions. Headers live under `cpp/include/dual_pane_desktop/`; implementation files live under `cpp/src/`. C++ names that bridge Qt may follow Qt's required naming and macro conventions, but native types never appear in an inner Rust crate's public API.
- Documentation filenames use lowercase kebab-case, except the established uppercase repository files `README.md`, `AGENTS.md`, `LICENSE`, and Cargo's conventional filenames. Phase and milestone filenames continue to follow [development-process.md](development-process.md).
- File names state the owned concept, not a technology that happens to implement it. Use `directory_listing.rs` or `settings_gateway.rs` where that is the owned role; reserve names such as `qt_*`, `macos_*`, or `cxx_*` for desktop-only files that truly bind that technology.

## Test source layout and boundary rules

Test placement is part of the dependency design. A test may name the crate it tests and that crate's allowed dependencies, but it may not use a test-only import to bypass a production boundary.

| Test scope | Source location | May use | Must not use |
|---|---|---|---|
| Module-local behavior | `src/<module>.rs` in a `#[cfg(test)]` module | Private implementation details of that module | Another crate's private API or a real external resource |
| Crate public behavior | `crates/<package>/tests/<behavior>.rs` | The package's public API and dependencies permitted to that package | Private modules or an outer Dual Pane crate |
| Test-only builders and fakes | `crates/<package>/tests/support/mod.rs` (or a nested module below it) | Only the same allowed dependency direction as the owning test | A top-level file directly under `tests/`, which Cargo would treat as another integration-test target |
| Qt, bridge, and macOS behavior | `dual-pane-desktop` unit or integration tests | Desktop public APIs and native facilities required by the test | A direct mutation of application state or a separate C++ test runner |

Small unit tests stay beside the implementation when they clarify local invariants. Tests of a crate's public contract, especially cross-module behavior such as stale-listing rejection, belong in `tests/` so they cannot rely on private implementation details. Test filenames name the observable behavior under test; the example names in the target tree are conventions, not required future files.

`tests/support/` is deliberately nested. Cargo automatically treats each Rust file directly below `tests/` as an integration-test crate, so shared helpers must be nested and imported by their owning tests. Do not make a workspace-wide test-support crate preemptively. If a genuinely shared, framework-neutral fixture is later needed, its dependency direction and ownership must be designed in the milestone that introduces it rather than letting test convenience create a fifth production boundary.

Fixtures are source-controlled, minimal, and immutable inputs such as malformed settings bytes or listing metadata samples. Tests that create, alter, copy, move, or delete file-system entries construct their hierarchy in a temporary directory and clean it up with the test; they never use a developer's home directory, `/`, or another real user path. Desktop tests remain Cargo-driven: `cpp/` contains bridge implementation, not an independently built C++ test program, CMake project, or alternate test framework.

Test-only dependencies are dependencies for architecture purposes. A crate's `[dev-dependencies]` must preserve the same inward-only rule as its normal dependencies, and adding or changing one requires the dependency approval described in [AGENTS.md](../AGENTS.md). Domain, application, and adapter tests must remain runnable without Qt or macOS facilities; desktop-owned tests are the only tests permitted to require them.

## Change sequence

When P1-M2 is planned and implemented, it will convert the current root package into the virtual workspace, retire the root `src/main.rs`, and create only the workspace manifest and desktop crate needed for the Qt window. The three inner library crates are added when a later planned milestone first needs their boundary; their package names and dependency directions are fixed by this document and [architecture.md](architecture.md). Each such milestone adds the tests and fixtures that prove its new boundary rather than deferring test placement to a later cleanup.

The conversion must preserve the architecture's verification rule: inner-crate tests run without Qt or macOS facilities, while Qt and native integration tests remain desktop-owned. The P1-M2 checklist must name the exact test or launch evidence for the stack spike and must not mark the milestone complete until the repository-wide gate in [AGENTS.md](../AGENTS.md) passes.
