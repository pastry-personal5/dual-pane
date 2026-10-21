//! Context menu visibility settings and the Folder Items menu lifecycle.

use super::*;

impl<R: WorkRunner> WorkspaceSession<R> {
    pub fn set_context_menu_visibility(&mut self, kind: dual_pane_application::ContextMenuKind, action: dual_pane_application::ActionId, visible: bool) -> Option<dual_pane_application::ContextMenuPreferenceRejection> {
        self.handle_all(Command::SetContextMenuVisibility { kind, action, visible }.into()).iter().find_map(|output| match output {
            Output::ContextMenuPreferenceRejected { reason } => Some(*reason),
            _ => None,
        })
    }
    pub fn reset_context_menu(&mut self, kind: dual_pane_application::ContextMenuKind) {
        self.handle_all(Command::ResetContextMenu { kind }.into());
    }
    pub fn reset_context_menus(&mut self) {
        self.handle_all(Command::ResetContextMenus.into());
    }
    /// Obtains the application-owned, one-use context-menu projection for
    /// exactly the rows currently shown by the native view.
    pub fn request_folder_items_menu(&mut self, browser: BrowserSide, kind: ContextMenuKind, row: Option<usize>, shown: &BrowserViewModel) -> Option<FolderItemsMenuSnapshot> {
        let tab = shown.active_tab()?;
        let target = match row {
            Some(row) => FolderItemsMenuTarget::Row { row, name: shown.row_entry_name(row)? },
            None => FolderItemsMenuTarget::EmptySpace,
        };
        self.handle_all(Command::RequestFolderItemsMenu { browser, tab, visible_revision: shown.visible_revision(), kind, target }.into()).into_iter().find_map(|output| match output {
            Output::FolderItemsMenuReady { snapshot } => snapshot,
            _ => None,
        })
    }
    pub fn invoke_folder_items_menu(&mut self, token: dual_pane_application::FolderItemsMenuToken, action: dual_pane_application::ActionId) {
        self.submit(Command::InvokeFolderItemsMenu { token, action });
    }
    pub fn dismiss_folder_items_menu(&mut self, token: dual_pane_application::FolderItemsMenuToken) {
        self.submit(Command::DismissFolderItemsMenu { token });
    }
}
