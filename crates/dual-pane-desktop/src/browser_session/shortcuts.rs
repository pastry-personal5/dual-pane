//! Shortcut editing for the Settings window.

use super::*;

impl<R: WorkRunner> WorkspaceSession<R> {
    pub fn submit_shortcuts(&mut self, action: dual_pane_application::ActionId, shortcuts: Vec<dual_pane_application::Shortcut>) -> Option<dual_pane_application::ShortcutRejection> {
        self.handle_all(Command::SetActionShortcuts { action, shortcuts }.into()).iter().find_map(|output| match output {
            Output::ShortcutEditRejected { reason, .. } => Some(*reason),
            _ => None,
        })
    }
    pub fn restore_shortcuts(&mut self, action: dual_pane_application::ActionId) -> Option<dual_pane_application::ShortcutRejection> {
        self.handle_all(Command::RestoreActionShortcuts { action }.into()).iter().find_map(|output| match output {
            Output::ShortcutEditRejected { reason, .. } => Some(*reason),
            _ => None,
        })
    }
    pub fn shortcuts(&self, action: dual_pane_application::ActionId) -> Vec<dual_pane_application::Shortcut> {
        self.workspace.shortcuts(action).to_vec()
    }
}
