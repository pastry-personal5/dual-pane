use super::*;
use dual_pane_application::{DragId, DropAction, DropTarget};

impl ffi::FolderItemsListModel {
    pub(super) fn row_is_selected(&self, row: i32) -> bool {
        usize::try_from(row).is_ok_and(|row| self.rust().shown.selected_rows().contains(&row))
    }

    pub(super) fn begin_file_drag(self: Pin<&mut Self>, tab: u64, visible_revision: u64) -> u64 {
        let browser = self.rust().browser;
        let result = with_session(|session| session.begin_drag(browser, TabId::new(tab), visible_revision)).flatten();
        let Some(drag) = result else { return 0 };
        drag.get()
    }

    fn drop_target(&self, target_kind: i32, target_tab: u64, row: i32) -> Option<DropTarget> {
        let browser = self.rust().browser;
        let tab = TabId::new(target_tab);
        match target_kind {
            0 => Some(DropTarget::BrowserFolder { browser, tab }),
            1 => {
                let row = usize::try_from(row).ok()?;
                let name = self.rust().shown.row_entry_name(row)?;
                Some(DropTarget::FolderRow { browser, tab, visible_revision: self.rust().shown.visible_revision(), name })
            }
            2 => Some(DropTarget::BrowserTab { browser, tab }),
            _ => None,
        }
    }

    pub(super) fn preview_file_drop(self: Pin<&mut Self>, drag: u64, source_browser: i32, target_kind: i32, target_tab: u64, row: i32, copy: bool) -> i32 {
        let Some(target) = self.drop_target(target_kind, target_tab, row) else { return -1 };
        let source_browser = if source_browser == 0 {
            BrowserSide::Left
        } else if source_browser == 1 {
            BrowserSide::Right
        } else {
            return -1;
        };
        let action = if copy { DropAction::Copy } else { DropAction::Move };
        let result = with_session(|session| session.preview_drop(DragId::new(drag), source_browser, target, action));
        result.and_then(Result::ok).map_or(-1, |preview| qt_int(preview.count))
    }

    pub(super) fn accept_file_drop(mut self: Pin<&mut Self>, drag: u64, source_browser: i32, target_kind: i32, target_tab: u64, row: i32, copy: bool) -> bool {
        let Some(target) = self.drop_target(target_kind, target_tab, row) else { return false };
        let source_browser = if source_browser == 0 {
            BrowserSide::Left
        } else if source_browser == 1 {
            BrowserSide::Right
        } else {
            return false;
        };
        let action = if copy { DropAction::Copy } else { DropAction::Move };
        let accepted = with_session(|session| session.accept_drop(DragId::new(drag), source_browser, target, action)).unwrap_or(false);
        self.as_mut().session_changed();
        accepted
    }

    pub(super) fn cancel_file_drag(self: Pin<&mut Self>, drag: u64) {
        with_session(|session| session.cancel_drag(DragId::new(drag)));
    }
}
