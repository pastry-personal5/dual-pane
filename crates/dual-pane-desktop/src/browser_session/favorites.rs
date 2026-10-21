//! Favorites edits and the Favorites root label.

use super::*;

impl<R: WorkRunner> WorkspaceSession<R> {
    /// Handles `input` and every terminal event its work returns at once, and
    /// reports a Favorites rejection among the results.
    pub(super) fn submit_and_report(&mut self, input: Input) -> Option<(FavoriteEdit, FavoriteRejection)> {
        self.handle_all(input).iter().rev().find_map(|output| match output {
            Output::FavoriteEditRejected { edit, reason } => Some((*edit, *reason)),
            _ => None,
        })
    }
    /// Handles a Sidebar gesture for the active Browser and reports whether
    /// the application rejected it, so an inline editor can stay open.
    pub fn submit_favorites(&mut self, event: FavoritesEvent) -> Option<(FavoriteEdit, FavoriteRejection)> {
        let browser = self.workspace.active_browser();
        let command = self.controller.favorites_command(event, self.sidebar.view(), browser, self.view(browser))?;
        self.submit_and_report(command.into())
    }
    /// Names the root volume for tab labels and root Favorite aliases.
    pub fn set_root_label(&mut self, label: &str) {
        self.left.set_root_label(label);
        self.right.set_root_label(label);
    }
}
