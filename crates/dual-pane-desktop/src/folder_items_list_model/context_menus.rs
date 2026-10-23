//! The Folder Items Context Menu session.

use super::*;

pub(super) fn context_menu_action(menu: Option<&FolderItemsMenuSnapshot>, index: i32) -> Option<&FolderItemsMenuAction> {
    let index = usize::try_from(index).ok()?;
    menu?.sections.iter().flat_map(|section| section.actions.iter()).nth(index)
}

impl ffi::FolderItemsListModel {
    pub(super) fn request_context_menu(mut self: Pin<&mut Self>, kind: i32, row: i32) -> bool {
        let kind = if kind == 1 { ContextMenuKind::Comprehensive } else { ContextMenuKind::Normal };
        let browser = self.rust().browser;
        let shown = self.rust().shown.clone();
        let menu = with_session(|session| session.request_folder_items_menu(browser, kind, usize::try_from(row).ok(), &shown)).flatten();
        self.as_mut().rust_mut().get_mut().context_menu = menu;
        self.as_mut().session_changed();
        self.rust().context_menu.is_some()
    }
    pub(super) fn context_menu_action_count(&self) -> i32 {
        self.rust().context_menu.as_ref().map(|menu| menu.sections.iter().map(|section| section.actions.len()).sum::<usize>()).and_then(|count| i32::try_from(count).ok()).unwrap_or(0)
    }
    pub(super) fn context_menu_action_id(&self, index: i32) -> i32 {
        context_menu_action(self.rust().context_menu.as_ref(), index).map_or(-1, |action| match action {
            FolderItemsMenuAction::Static { action, .. } => *action as i32,
            FolderItemsMenuAction::OpenWith(_) => -2,
        })
    }
    pub(super) fn context_menu_action_label(&self, index: i32) -> QString {
        context_menu_action(self.rust().context_menu.as_ref(), index).map_or_else(QString::default, |action| match action {
            FolderItemsMenuAction::Static { action, .. } => QString::from(action_label(*action).as_str()),
            FolderItemsMenuAction::OpenWith(action) => QString::from(action.label.as_str()),
        })
    }
    pub(super) fn context_menu_action_shortcut(&self, index: i32) -> QString {
        context_menu_action(self.rust().context_menu.as_ref(), index)
            .and_then(|action| match action {
                FolderItemsMenuAction::Static { shortcut, .. } => *shortcut,
                FolderItemsMenuAction::OpenWith(_) => None,
            })
            .map(key_sequence)
            .map_or_else(QString::default, |sequence| QString::from(sequence.as_str()))
    }
    pub(super) fn context_menu_action_starts_section(&self, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else { return false };
        self.rust().context_menu.as_ref().is_some_and(|menu| {
            menu.sections
                .iter()
                .scan(0, |offset, section| {
                    let first = *offset;
                    *offset += section.actions.len();
                    Some(first)
                })
                .any(|first| first == index && index != 0)
        })
    }
    pub(super) fn context_menu_kind(&self) -> i32 {
        self.rust().context_menu.as_ref().map_or(0, |menu| matches!(menu.kind, ContextMenuKind::Comprehensive) as i32)
    }
    pub(super) fn invoke_context_menu_action(mut self: Pin<&mut Self>, index: i32) {
        let menu = self.rust().context_menu.as_ref().map(|menu| (menu.token, context_menu_action(Some(menu), index).cloned()));
        self.as_mut().rust_mut().get_mut().context_menu = None;
        if let Some((token, action)) = menu {
            with_session(|session| match action {
                Some(FolderItemsMenuAction::Static { action, .. }) => session.invoke_folder_items_menu(token, action),
                Some(FolderItemsMenuAction::OpenWith(action)) => session.invoke_open_with(token, action.token),
                None => session.dismiss_folder_items_menu(token),
            });
        }
        self.as_mut().session_changed();
    }
    pub(super) fn dismiss_context_menu(mut self: Pin<&mut Self>) {
        let token = self.rust().context_menu.as_ref().map(|menu| menu.token);
        self.as_mut().rust_mut().get_mut().context_menu = None;
        if let Some(token) = token {
            with_session(|session| session.dismiss_folder_items_menu(token));
        }
        self.as_mut().session_changed();
    }
}
