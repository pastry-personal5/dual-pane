//! Context Menus Settings invokables and properties.

use super::*;

impl ffi::WorkspaceBridge {
    pub(super) fn context_menu_action_count(&self) -> i32 {
        count(self.rust().shown.context_menu_rows().len())
    }
    pub(super) fn context_menu_action_label(&self, index: i32) -> QString {
        self.context_menu_row(index).map_or_else(QString::default, |row| QString::from(row.label.as_str()))
    }
    pub(super) fn context_menu_group_label(&self, index: i32) -> QString {
        self.context_menu_row(index).map_or_else(QString::default, |row| QString::from(row.group_label))
    }
    pub(super) fn context_menu_normal_visible(&self, index: i32) -> bool {
        self.context_menu_row(index).is_some_and(|row| row.normal)
    }
    pub(super) fn context_menu_comprehensive_visible(&self, index: i32) -> bool {
        self.context_menu_row(index).is_some_and(|row| row.comprehensive)
    }
    pub(super) fn context_menu_shortcut(&self, index: i32) -> QString {
        self.context_menu_row(index).and_then(|row| row.shortcut).map(key_sequence).map_or_else(QString::default, |shortcut| QString::from(shortcut.as_str()))
    }
    pub(super) fn context_menu_row(&self, index: i32) -> Option<&dual_pane_adapters::ContextMenuSettingsRowViewModel> {
        usize::try_from(index).ok().and_then(|index| self.rust().shown.context_menu_rows().get(index))
    }
    pub(super) fn change_context_menu_visibility(mut self: Pin<&mut Self>, index: i32, kind: i32, visible: bool) -> QString {
        let Some(action) = self.context_menu_row(index).map(|row| row.action) else { return QString::default() };
        let kind = context_menu_kind(kind);
        let rejection = with_session(|session| session.set_context_menu_visibility(kind, action, visible)).flatten();
        self.as_mut().session_changed();
        match rejection {
            Some(ContextMenuPreferenceRejection::LastVisibleAction(kind)) => QString::from(context_menu_rejection_text(kind)),
            Some(ContextMenuPreferenceRejection::IneligibleAction | ContextMenuPreferenceRejection::Malformed) => QString::from("Context Menu settings could not be changed."),
            None => QString::default(),
        }
    }
    pub(super) fn reset_context_menu(mut self: Pin<&mut Self>, kind: i32) {
        with_session(|session| session.reset_context_menu(context_menu_kind(kind)));
        self.as_mut().session_changed();
    }
    pub(super) fn reset_context_menus(mut self: Pin<&mut Self>) {
        with_session(WorkspaceSession::reset_context_menus);
        self.as_mut().session_changed();
    }
    pub(super) fn context_menu_rejection_text(&self) -> QString {
        self.rust().shown.last_context_menu_rejection().map(context_menu_rejection_text).map_or_else(QString::default, QString::from)
    }
    pub(super) fn set_context_menus_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().context_menus_revision != value {
            self.as_mut().rust_mut().get_mut().context_menus_revision = value;
            self.context_menus_revision_changed();
        }
    }
    pub(super) fn set_context_menu_rejection_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().context_menu_rejection_revision != value {
            self.as_mut().rust_mut().get_mut().context_menu_rejection_revision = value;
            self.context_menu_rejection_revision_changed();
        }
    }
}

pub(super) fn rejection_text(rejection: Option<ShortcutRejection>) -> QString {
    match rejection {
        None => QString::default(),
        Some(ShortcutRejection::Invalid) => QString::from("Use a supported key with Command, Control, or Option; function keys may stand alone. Return for Rename Item and Shift+Backspace or Shift+Forward Delete for Move to Trash are supported exceptions."),
        Some(ShortcutRejection::Reserved) => QString::from("This shortcut is reserved for Settings or macOS."),
        Some(ShortcutRejection::Duplicate) => QString::from("This action already has that shortcut."),
        Some(ShortcutRejection::Conflict(action)) => QString::from(format!("Already assigned to {}.", action_label(action)).as_str()),
    }
}

pub(super) fn context_menu_kind(kind: i32) -> ContextMenuKind {
    if kind == 1 { ContextMenuKind::Comprehensive } else { ContextMenuKind::Normal }
}
