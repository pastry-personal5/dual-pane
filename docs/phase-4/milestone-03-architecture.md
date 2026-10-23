# P4-M3 architecture

Status: Done

The Qt Folder Items Filter Row edits application-owned tab state through a typed controller event and renders the resulting view model. The [overview checklist](milestone-03-overview.md#completion-checklist) defines acceptance.

## Affected surfaces

| Surface | M3 responsibility |
|---|---|
| `crates/dual-pane-application/src/workspace.rs` and `tests/filtering.rs` | Replace substring containment with prefix matching and retain M2 safety, ordering, and reconciliation behavior. |
| `crates/dual-pane-application/src/settings.rs` | Append `FocusFolderItemsFilter`, its Window scope, and its `Command+F` default without reordering stable IDs. |
| `crates/dual-pane-adapters/src/input_controller.rs` and `browser_presenter.rs` | Translate typed filter edits, expose coherent filter state, and preserve observation-time visible targets. |
| `crates/dual-pane-desktop/src/folder_items_list_model.rs` | Expose filter properties and signals and carry observation-time tab, revision, and byte-exact row identity across CXX-Qt. |
| `crates/dual-pane-desktop/src/browser_session.rs` | Submit the observed payload unchanged and reject filter edits whose observed tab is no longer shown; do not restamp visible gestures from the latest view. |
| `crates/dual-pane-desktop/src/settings_storage.rs` and `workspace_bridge.rs` | Preserve older profiles, persist and present the new action, and cover catalogue compatibility. |
| `crates/dual-pane-desktop/cpp/src/desktop_window.cpp` | Construct and synchronize the row, route native text and close events, integrate shortcuts and accessibility, and suppress programmatic edit feedback. |

M3 adds no dependency or build-tool change.

## Qt-free and presentation contract

Change the Qt-free matcher in `crates/dual-pane-application` from substring containment to prefix matching after applying M2's existing Unicode transformation to both query and displayed name. Retain byte-exact Item identity, literal query whitespace, bounded worker admission, result validation, projection ordering, and reconciliation. Update `crates/dual-pane-application/tests/filtering.rs` so a match occurring only in the middle of a name is rejected while the M2 normalization, failure, race, and visible-action scenarios continue to pass.

Expose the active tab's filter text, filter status, total count, visible-row revision, and explicit zero-result state through `BrowserViewModel` and `FolderItemsListModel`. A successful zero-result projection is `Ready`, has a nonempty query and zero visible rows, and is not a projection failure. The presenter continues to provide the visible/total summary and the retryable projection-failure status; the native layer must not infer these states only from an empty row count.

Add a typed filter-edit handoff that carries `BrowserSide`, the tab observed by the Folder Items Filter Row, and the complete edited text into `Command::SetFilterText`. The adapter or session validates that the observed tab is still the Browser's shown tab before submitting it. Closing uses the same handoff with empty text. Do not add a `String` payload to the existing copyable `UiEvent`; use a dedicated filter-edit value or method.

## Observed gesture boundary

Extend the `FolderItemsListModel` CXX-Qt surface so the native event handler captures the shown tab and visible-row revision when it receives a row or selection-dependent gesture. A row gesture also captures the exact Rust `EntryName` from the shown model; never round-trip that identity through `QString`. Carry this observation into `Input::ObservedCommand` instead of consulting `BrowserSession::view()` later and stamping whatever revision is current then. The application remains the final validator of browser, tab, revision, row, and exact name.

This capture applies to pointer selection and activation, keyboard movement and activation, Select All, scrolling, inline Rename, Quick Look targeting, context-menu commands, and selection-based file commands. Tests must queue an old observation, publish a different projection or tab, then prove the old observation cannot act even when the new view has the same row number and displayed name.

## Desktop composition and signal ordering

Place the temporary Folder Items Filter Row immediately below the Folder Navigation and Sort Toolbar. It contains text rendered as `Starts with: {query}` and a far-right Close Filter Button. `crates/dual-pane-desktop/cpp/src/desktop_window.cpp` owns widget construction and accessibility properties; the CXX-Qt bridge and `browser_session.rs` carry text changes and per-tab view updates. The application never holds a Qt widget or clipboard object.

Apply model and row updates on the GUI thread. While projection work runs, show the blank list and updating cue without clearing application-owned visit state; block Item and selection-dependent actions. A failed or cancelled projection remains blank and blocked and exposes the presenter's retryable filter-failure status to sighted and assistive-technology users. For a successful zero-result projection, leave the Folder Items List visually empty: do not insert a synthetic row or overlay. Keep the filter row visible, publish the zero-result count, and announce the result through accessibility.

When Folder Items List has keyboard focus, route an unmodified printable text event other than Space into the active tab's filter and focus the Folder Items Filter Row for subsequent text input. Consume the initiating text exactly once; do not synthesize a second key press. Append subsequent printable input to the query, so `a` followed by `p` produces `Starts with: ap`. IME preedit is displayed in the focused text editor and does not update the query; committed text updates it once. Leave Space, movement, activation, modified shortcuts, and inline editor input with their existing owners.

Activating the Close Filter Button submits an empty query for the tab shown by that row, restores the complete current projection, and hides the row. Use signal blocking or an equivalent explicit rendering guard when applying `filter_text`, changing tabs, showing or hiding the row, or applying an asynchronous result. Only user-originated edits enter the filter-edit handoff. When a coherent filter view is rendered, retain that view's tab identity beside the native text field; text edits, Close, and Escape submit that retained identity rather than asking the model which tab is current when the callback runs. The application rejects it if that tab is no longer shown. Track an explicitly opened empty row separately: `Command+F` and a user-cleared field may keep it visible until focus loss, while a programmatic clear (for example a confirmed different-folder navigation) hides it. Reset this empty-row intent on every tab switch. A tab with a nonempty query shows its row when activated without briefly rendering the previous tab's text or rows.

## Shortcut catalogue and focus command

Append `FocusFolderItemsFilter` to `ActionId::ALL`; do not insert it before an existing stable action. Give it visible name **Focus Folder Items Filter**, Window scope, and the `Command+F` factory default. Wire the action through `settings.rs`, settings snapshot/storage compatibility, `workspace_bridge.rs`, the Keyboard Shortcuts Settings navigation group, `ShortcutBinder`, and the active-Browser handler in `desktop_window.cpp`. The handler shows and focuses that Browser's Folder Items Filter Row even when the query is empty. An empty row hides on focus loss or tab switch and preserves the new focus; a nonempty query remains tab-local and restores its row.

Loading an older profile adds the missing action under the existing compatibility rules without disturbing customized or deliberately unbound actions and without taking `Command+F` from an accepted existing binding. Reset restores the factory default subject to the normal conflict rules. Tests cover catalogue append order, parsing and persistence, old-profile completion, default restoration, conflict rejection, settings presentation, handler routing to the active Browser, and rebinding.

While the filter text editor has focus, its text input and ordinary editing shortcuts take precedence over workspace actions. Integrate it with `ShortcutBinder` using an explicit text-editing classification rather than treating it as an inline Rename editor accidentally. Keep the fixed full-screen shortcut, Space Quick Look ownership, and Settings shortcut capture behavior unchanged.

## Verification and sequencing

Implement in this order: prefix matcher and Qt-free tests; presenter/model filter properties and typed edit handoff; observation-time gesture payloads and stale-event tests; native row and signal guards; shortcut catalogue and persistence; accessibility; then automated desktop acceptance. This keeps every native path on an already-tested application contract. Physical macOS, real-IME, and VoiceOver validation is deferred without a target date in the [overview](milestone-03-overview.md#deferred-native-validation).

Desktop unit tests cover signal routing, programmatic-update suppression, rendered-tab identity, explicit-empty-row lifetime, observation-time revisions and names, status differentiation, action-catalogue compatibility, and focus/shortcut precedence. The physical-platform checks are recorded separately as deferred validation rather than a remaining milestone condition.
