# UX information architecture

Status: Active

This document describes the current component hierarchy of Dual Pane and explicitly named accepted additions that remain planned. It records containment; visual treatment and interaction rules are in [ux-gui.md](ux-gui.md), and not every conceptual container has a separate Qt widget.

## Workspace Window

- Frameless main application window; the Application Title Row moves the window
- Fixed `Control+Command+F` workspace shortcut toggles full screen
- Workspace Window
  - Sidebar
    - Draggable Application Title Row with **Dual Pane** heading and hover-revealed Application Close Button
    - Drives Group with Drive Items
    - Favorites Groups
      - Favorite Group rows with Favorite Group Name, Favorite Group Menu Button, Add Favorite Item Button, and Favorite Items
      - New Group Button
    - Main Toolbar
      - Settings Button
      - Notices Button
  - Sidebar Splitter
  - Left Browser
    - Navigation Pane
      - Browser Tabs Strip
        - Browser Tabs, each with a Close Tab Button
        - New Tab Button
    - Folder Pane
      - Folder Pane Toolbar Row #1, containing the Path Edit Control
        - Confirmed-path Breadcrumbs
        - Expand/Collapse Button
        - Edit Button, which enters the P4-M6 typed-path mode
        - Typed path editor when editing
      - Ancestor Folder Columns when expanded (visible child folders along path, ending at current folder's parent; Expand disabled at root)
      - Folder Summary Row with item count, selected count, and size
      - Folder Navigation and Sort Toolbar with Back Button, Forward Button, Up Button, and sort controls
      - Temporary Folder Items Filter Row with “Starts with:” text input, result status, and a far-right Close Filter Button
      - Folder Items List and Folder Items Context Menu
      - Missing Folder Overlay
    - Preview Pane Splitter
    - Preview Pane
      - Item Content Preview
      - Item Details Preview
    - Browser Status Bar
  - Browser Divider
  - Right Browser with the same Navigation Pane, Folder Pane, Preview Pane, and Browser Status Bar structure as the Left Browser
  - Operation Panel Strip containing docked Operation Panels
- Floating Operation Panels, each with Operation Progress Indicator, Cancel Operation Button, and any Operation Decision Card for its job

Each Browser owns its tabs, location, Folder Items, selection, cursor, and preview state. The Sidebar contains one level of Favorite Groups; groups do not nest, and each group contains ordered Favorite Items.

## Auxiliary Windows and Dialogs

- Notices Window with Notice entries and the Notices Startup Checkbox
- Settings Window
  - Settings Title Row with Close Button at the far left, followed by the Settings heading
  - Settings Category List
    - General (General Settings page)
      - Startup Notices card
        - Don’t show notices at startup
      - Item Visibility card
        - Show macOS metadata and service items
        - Show dotfiles
        - Show items marked invisible by macOS
        - Visibility rules help
      - Reset Settings card
        - Reset Settings Button
    - Keyboard Shortcuts (Keyboard Shortcuts Settings page)
      - Scrollable list of full-width category tables: Navigation, Tabs and Windows, File Actions, Sorting, and Other Actions
        - Category name as plain text above the table card, aligned with the Action column
        - Repeated Shortcut Table Header: Action, Shortcuts, Original Shortcuts, Scope, Add, and an unlabeled information column
        - Action rows with:
          - Action name
          - Current shortcut combinations, each on its own line with a Remove Shortcut Button
          - Original factory shortcut combinations and Reset to Default Shortcuts Button
          - Shortcut scope
          - Add Shortcut Button in its own column
          - Settings Information Button at the far right, after the unused width
          - Inline capture or rejection feedback when needed
    - Context Menus (Context Menus Settings page)
      - Scrollable Folder Items menu action matrix of full-width tables grouped, in order, as Rename and Create, Open, Clipboard, Other Browser, and Delete
        - Category name as plain text above the table card, aligned with the Action column
        - Repeated header: Action, Normal, Comprehensive, Shortcut, and an unlabeled information column
        - Action rows with independent Normal and Comprehensive visibility checkboxes, a read-only first effective shortcut, and a Settings Information Button at the far right
        - Inline feedback when a change would leave either menu with no configured action
      - Reset Normal Button
      - Reset Comprehensive Button
      - Reset Both Button
      - A separate confirmation for each reset, with Cancel as the default and Escape action
    - File Associations (File Associations Settings page; planned P4-M13)
      - Type list from open-folder files, pinned sample-file types, and types with saved choices
      - Add File Type Button and Refresh Applications Button
      - Selected type's kind, extension when available, content-type identifier, and current macOS-user default app
      - Ranked application rows with independent Normal and Comprehensive checkboxes and unavailable-app feedback
      - Clear Open With Choices Button, Remove Type Button, and Set as Default Button with scope-specific confirmations
  - Anchored Settings Information Overlay
- Waiting Window while the workspace session loads
- Permanent Delete Confirmation Window
- Quit Confirmation Window

The M8 additions above are implemented; their detailed behavior follows the [P4-M8 owner decisions](phase-4/milestone-08-overview.md#owner-decisions--2026-10-23). The Favorites hierarchy has one level. Settings and Notices are separate windows opened from the Main Toolbar. See [ux-gui.md](ux-gui.md) for presentation, focus, capture, dismissal, and shortcut behavior.
