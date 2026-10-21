//! General Settings invokables and properties.

use super::*;

impl ffi::WorkspaceBridge {
    pub(super) fn reset_settings(mut self: Pin<&mut Self>) {
        with_session(|session| session.submit(dual_pane_application::Command::ResetSettings));
        self.as_mut().session_changed();
    }
    pub(super) fn change_hide_notices_at_startup(mut self: Pin<&mut Self>, hide: bool) {
        with_session(|session| session.submit(dual_pane_application::Command::SetHideNoticesAtStartup { hide }));
        self.as_mut().session_changed();
    }
    pub(super) fn change_skip_quit_confirmation(mut self: Pin<&mut Self>, skip: bool) {
        with_session(|session| session.submit(dual_pane_application::Command::SetSkipQuitConfirmation { skip }));
        self.as_mut().session_changed();
    }
    pub(super) fn change_visibility(mut self: Pin<&mut Self>, show_macos_items: bool, show_dotfiles: bool, show_macos_invisible: bool) {
        with_session(|session| session.submit(dual_pane_application::Command::SetVisibilityPolicy { visibility: dual_pane_application::VisibilityPolicy { show_macos_items, show_dotfiles, show_macos_invisible } }));
        self.as_mut().session_changed();
    }
    pub(super) fn set_hide_notices_at_startup(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().hide_notices_at_startup != value {
            self.as_mut().rust_mut().get_mut().hide_notices_at_startup = value;
            self.hide_notices_at_startup_changed();
        }
    }
    pub(super) fn set_skip_quit_confirmation(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().skip_quit_confirmation != value {
            self.as_mut().rust_mut().get_mut().skip_quit_confirmation = value;
            self.skip_quit_confirmation_changed();
        }
    }
    pub(super) fn set_show_macos_items(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().show_macos_items != value {
            self.as_mut().rust_mut().get_mut().show_macos_items = value;
            self.show_macos_items_changed();
        }
    }
    pub(super) fn set_show_dotfiles(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().show_dotfiles != value {
            self.as_mut().rust_mut().get_mut().show_dotfiles = value;
            self.show_dotfiles_changed();
        }
    }
    pub(super) fn set_show_macos_invisible(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().show_macos_invisible != value {
            self.as_mut().rust_mut().get_mut().show_macos_invisible = value;
            self.show_macos_invisible_changed();
        }
    }
    pub(super) fn set_settings_interaction_available(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().settings_interaction_available != value {
            self.as_mut().rust_mut().get_mut().settings_interaction_available = value;
            self.settings_interaction_available_changed();
        }
    }
}
