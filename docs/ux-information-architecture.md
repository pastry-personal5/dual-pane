# UX information architecture

Status: Active

The Standard Layout is the conceptual containment model for the dual-Browser window. It records structure only; visual and interaction rules are in [ux-gui.md](ux-gui.md), and it does not imply that every conceptual container has a separate Qt widget.

## Current Standard Layout (version 2.0.0)

- Standard Layout
  - Sidebar
    - Drives Group with Drive Items
    - Favorites Groups containing Favorite Groups, each with Favorite Items
    - flexible space
    - Main Toolbar with Settings
  - Sidebar Splitter
  - Left Browser
    - Navigation Pane
      - Browser Tabs Strip: label-only current-folder strip
      - Path Edit Control
    - Folder Pane
      - Folder Pane Toolbar Row #1: concise current folder name
      - Folder Pane Toolbar Row #2: reserved Folder Items summary
      - Folder Pane Toolbar Row #3: Up Button and sort controls
      - Folder Items List
      - Browser Status Bar
  - Browser Divider
  - Right Browser, with the same Navigation Pane and Folder Pane structure

The Sidebar Splitter divides the Sidebar from both Browsers. The Browser Divider divides the Left Browser from the Right Browser. Each Browser owns its own location, Folder Items, selection, and active state. Browser Tabs Strip binds only to the current folder name; it does not yet create, close, switch, or retain history. The Sidebar groups remain static and are not connected to persisted Favorites.

## Planned Phase 3 containment

- Workspace window
  - Sidebar
    - Favorites Groups
      - Favorite Group rows, each with Favorite Group Name, Favorite Group Menu Button, Add Favorite Item Button, and Favorite Items
      - New Group Button below the group list
    - Main Toolbar with Settings and Notices Button
  - Left Browser and Right Browser
    - Navigation Pane with Browser Tabs Strip, Browser Tabs, each tab's Close Tab Button, New Tab Button, and Path Edit Control
    - Folder Pane
      - Folder Pane Toolbar Rows #1 and #2
      - Folder Pane Toolbar Row #3 with Back Button, Forward Button, Up Button, and sort controls
      - Folder Items List, Folder Items Context Menu, and Missing Folder Overlay
      - Browser Status Bar
  - Operation Panel Strip below both Browsers, containing docked Operation Panels
- Floating Operation Panels, each with Operation Progress Indicator, Cancel Operation Button, and any Operation Decision Card for its job
- Notices auxiliary window with Notice entries and Notices Startup Checkbox
- Settings Window with General Settings and Keyboard Shortcuts Settings categories
- Transient Waiting Window while the workspace session loads
- Permanent Delete Confirmation Window

The Favorites hierarchy has one level: Favorites Groups contains Favorite Groups, and each group contains ordered Favorite Items. Groups do not nest. The Sidebar and Browser Tabs Strip above describe planned containment; the [GUI rules](ux-gui.md#planned-phase-3-gui-decisions) define their interactions.
