# UX information architecture

Status: Active

The Standard Layout is the conceptual containment model for the dual-Browser window. It records structure only; visual and interaction rules are in [ux-gui.md](ux-gui.md), and it does not imply that every conceptual container has a separate Qt widget.

## Standard Layout

- Standard Layout
  - Sidebar
    - Drives Group with Drive Items
    - Favorites Groups containing Favorite Groups, each with Favorite Items
  - Sidebar Splitter
  - Left Browser
    - Navigation Pane
      - Browser Tabs (deferred)
      - Path Edit Control
    - Folder Pane
      - Folder Pane Toolbar Row #1: concise current folder name
      - Folder Pane Toolbar Row #2: Up Button
      - Folder Items List
      - Browser Status Bar
  - Browser Divider
  - Right Browser, with the same Navigation Pane and Folder Pane structure

The Sidebar Splitter divides the Sidebar from both Browsers. The Browser Divider divides the Left Browser from the Right Browser. Each Browser owns its own location, Folder Items, selection, and active state. Browser Tabs are part of the information architecture but remain deferred; the current Sidebar presents only static labels for its groups.
