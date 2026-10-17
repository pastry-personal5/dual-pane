# UX information architecture

Status: Active

This document describes the current component hierarchy of Dual Pane. It records containment; visual treatment and interaction rules are in [ux-gui.md](ux-gui.md), and not every conceptual container has a separate Qt widget.

## Workspace Window

- Frameless main application window; the Application Title Row moves the window
- Fixed `Control+Command+F` workspace shortcut toggles full screen
- Workspace Window
  - Sidebar
    - Draggable Application Title Row with **Dual Pane** heading
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
      - Path Edit Control
    - Folder Pane
      - Folder Pane Toolbar Row #1
      - Folder Pane Toolbar Row #2
      - Folder Pane Toolbar Row #3 with Back Button, Forward Button, Up Button, and sort controls
      - Temporary Folder Items Filter Row with filter text and a far-right Close Filter Button
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
    - General Settings
      - Startup Notices card
        - Don’t show notices at startup
      - Item Visibility card
        - Show macOS metadata and service items
        - Show dotfiles
        - Show items marked invisible by macOS
        - Visibility rules help
      - Reset Settings card
        - Reset Settings Button
    - Keyboard Shortcuts Settings
      - Scrollable list of left-aligned category cards sized to the table columns, ending just beyond Add with a small right inset: Navigation, Tabs and Windows, File Actions, Sorting, and Other Actions
        - Category heading
        - Repeated Shortcut Table Header: Action, Shortcuts, Original Shortcuts, Scope, Add
        - Action rows with:
          - Action name and Shortcut Information Button
          - Current shortcut combinations, each on its own line with a Remove Shortcut Button
          - Original factory shortcut combinations and Reset to Default Shortcuts Button
          - Shortcut scope
          - Add Shortcut Button in its own column
          - Inline capture or rejection feedback when needed
  - Anchored Settings Information Overlay
- Waiting Window while the workspace session loads
- Permanent Delete Confirmation Window
- Quit Confirmation Window

The Favorites hierarchy has one level. Settings and Notices are separate windows opened from the Main Toolbar. See [ux-gui.md](ux-gui.md) for presentation, focus, capture, dismissal, and shortcut behavior.
