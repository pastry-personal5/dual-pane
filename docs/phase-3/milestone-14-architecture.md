# P3-M14 architecture: Basic Settings and Keyboard Shortcuts

Status: Planned

P3-M14 extends the existing Qt-free settings model through the established application, adapter, worker, and bridge path. The exact new values and shortcut interaction are deliberately supplied by the required Owner Decision table, not chosen by the desktop implementation.

## Ownership and flow

For an owner-approved basic setting, add a typed value with its default and validation to the application settings state and snapshot, a command that proposes its change, and a projection that supplies its effective value to presentation. Accepted commands change the effective state immediately and schedule the existing settings-worker save; loading, reset, migration, and failed-save behavior follow the recorded decision and current storage rules. The SQLite worker remains the sole durable authority.

For bindings, add a typed application command that proposes an `ActionBinding` change and returns enough projection state for the Settings Window to render the current effective value and the owner-defined rejection feedback. Reuse stable `ActionId`, `Shortcut`, scope, validation, and persistence semantics wherever the recorded design permits. The Qt page converts captured input only into the application’s normalized proposal and displays the response; it does not decide collisions, reserved keys, or persistence.

On acceptance, the bridge announces the binding revision already used by the desktop shortcut binder, so the matching `QShortcut` is rebound immediately. Capture owns its key event until it commits or rejects, preventing the workspace shortcut from firing. Restore Default submits the action’s compiled default through the same path. General’s existing startup-notices control keeps its current command, and Reset invokes the existing confirmation/reset workflow before dismissing Settings.

## Storage and verification

Any new persisted value receives an explicit schema migration, temporary-directory round-trip coverage, and reset/default tests. A rejected proposal sends no save. A save failure leaves the effective state available for the current launch but changes settings-interaction availability to false, closes Settings, and retains the existing Notice/Reset recovery route.

Implement in this order: first complete the Owner Decision table; then add Qt-free values, commands, validation, migrations, and tests; project them through adapters and the bridge; finally implement the General and shortcut pages plus live rebinding/capture seams. Native checks and the repository gate complete the milestone.
