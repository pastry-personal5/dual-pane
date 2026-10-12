# P3-M16 architecture: GUI polish and Preview Pane

Status: Planned

P3-M16 keeps state ownership and presentation boundaries intact. Qt owns Preview Pane geometry, widget lifetime, scaling and painting; the application owns request identity and stale-result acceptance; the desktop runtime performs file reads and image decoding on a bounded worker lane.

## Desktop composition and visual states

Replace the `FolderPane` status `QLabel` with a private `PreviewPane` below the Folder Items List. A vertical `QSplitter` contains the existing Folder Items portion and `PreviewPane`; use the existing narrow splitter treatment, a dedicated accessible name, and no layout on the splitter itself. Qt's `setSizes` establishes the one-line initial preview; the child remains user-collapsible so the control can expose the complete Folder Items area. At collapsed or one-line height, `PreviewPane` contains only the status label. Its status projection reuses the previous precedence exactly: temporary Quick Look failure, loading/error/monitoring/command status, then concise summary, otherwise blank.

Once the preview has enough height to show content, show a horizontal child splitter with Item Content Preview and Item Details Preview. Establish a 2:1 initial ratio and retain that ratio through Browser resizing, constraining the two panes to useful minimum widths before the existing narrow-Browser overlay applies. The image widget preserves aspect ratio, centers the image, and never scales above the decoded image's native dimensions. The details child uses selectable, accessible text; its fields include Type, Modified, Size, and, for an image, Dimensions. Do not create a Qt layout on either splitter.

Extend the stylesheet with explicit Favorite Group Menu and Add Favorite Item foreground/background state selectors. The colors must cover normal, hover, pressed, disabled, and an inactive Sidebar. Button hover keeps priority over the `FavoriteGroupRow` hover, then normal selected, inline-edit, disabled, and error priorities continue to apply as P3-M12 defines them.

## Preview request boundary

Add an application-owned preview request/result pair to the existing input, output, and work-request boundary. A request copies Browser side, active tab ID, current listing revision, selection revision, selected `EntryName`, exact byte native path, requested image target size, and a monotonically increasing preview token. The Browser starts one only when its pane is expanded and its current active tab has exactly one selected `EntryKind::File`. Selection itself still belongs wholly to the existing Folder Items workflow.

The desktop runtime receives preview work independently of directory reads and file operations. It has bounded admission and cooperative cancellation, coalesces pending replacement requests by Browser, and reserves one terminal delivery per admitted job. A worker reads the selected local file with `QImageReader` (or an equivalent existing Qt image facility), checks the image format and dimensions before decoding, applies an allocation and decoded-pixel bound, uses the reader's orientation transformation, and decodes at a size no larger than the current content target. It obtains file byte size from the selected file's own metadata. It returns a typed supported preview, unsupported selection, unavailable file, decode failure, or cancelled result. A worker never touches `QWidget`, a list model, workspace state, or a shared image widget.

The application accepts a result only if the Browser still shows the captured tab, listing revision, selection revision, and exactly the captured single `EntryName`, the pane is still expanded, and the token is current. Any selection change, listing replacement, navigation, tab activation/close, collapse, Browser teardown, or newer target size invalidates the token before cancellation is requested. The presenter maps accepted state to plain preview content/details view data; the CXX-Qt bridge carries copied image data or a desktop-owned immutable image payload only after the match. The UI clears image and dimensions immediately on invalidation, which prevents cross-tab, cross-Browser, and stale-result display.

The domain `Entry` remains a listing record. It does not acquire image dimensions or decode fields, because those data are demanded only for an expanded single-item preview and are neither directory-sorting data nor enduring workspace state.

## Layout persistence

Extend the existing `WindowLayout`, session snapshot, SQLite session encoding, bridge properties, and `updateWindowLayout` call with `left_preview_splitter` and `right_preview_splitter`. Both values are non-negative logical-pixel positions. Backward-compatible session decoding supplies the collapsed one-line default. Validation rejects invalid values with the rest of a malformed layout; it never guesses positions from opaque Qt state. On restoration, first show and lay out the window, then apply Sidebar, Browser Divider, and per-Browser Preview splitter sizes on the next queued GUI turn. Debounce all splitter movement through the existing layout-report timer so changing either Preview Pane generates ordinary coalesced session saves.

## Delivery order and verification

1. Update UX containment/rules and P3-M16 plan decisions, then add layout fields with migration and Qt-free validation tests.
2. Refactor the Browser composition around `PreviewPane`, preserve collapsed status projection, and add dedicated splitter state restoration/reporting.
3. Add Favorite button state styling and desktop object/property seams.
4. Add application preview identity, reducer transitions, and presenter view data with focused stale-result tests.
5. Add the bounded desktop image reader and CXX-Qt bridge, using temporary files for supported image, corrupted data, disappearance, and byte-exact-path checks.
6. Wire the expanded view, resize replacement requests, clearing behavior, and details text; run native macOS checks after the full automated gate.

No new third-party dependency or build tool is required. Qt's documented `QSplitter` child sizing/collapse behavior supplies the layout mechanism, and `QImageReader` supplies image format, size, transformation, and decoding support through the existing dynamically linked Qt installation.
