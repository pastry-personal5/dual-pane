//! The Qt binder for workspace-wide presentation: the Sidebar Favorites,
//! Notices, effective shortcuts, and the active Browser. It also owns the one
//! session that both Browser models share.

use std::pin::Pin;
use std::sync::{Mutex, OnceLock};

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use dual_pane_adapters::{FavoritesEvent, GroupMotion, WorkspaceViewModel, favorite_rejection_text, reader_start_failure_status};
use dual_pane_application::{Key, Shortcut, ShortcutScope};
use dual_pane_domain::BrowserSide;

use crate::browser_session::{BrowserStartup, DRAIN_SLICE, DRAIN_TIME_BUDGET, SHUTDOWN_TIMEOUT, WorkspaceSession};
use crate::operation_journal::{Journal, journal_path};
use crate::operation_lane::{STEP_BUDGET, Services};
use crate::operation_step::NativeFileSystem;
use crate::runtime::Runtime;
use crate::settings_storage::SettingsWorker;

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
pub mod ffi {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    extern "Rust" {
        type BrowserStartup;
        /// Flushes settings after the Qt event loop exits; see [`super::shutdown_desktop`].
        fn shutdown_desktop();
    }
    unsafe extern "C++" {
        include!("dual_pane_desktop/desktop_window.hpp");
        fn run_desktop(startup: Box<BrowserStartup>) -> i32;
        fn schedule_gui_drain();
    }
    unsafe extern "RustQt" {
        #[qobject]
        /// 0 for the Left Browser, 1 for the Right Browser.
        #[qproperty(i32, active_browser, cxx_name = "activeBrowser", READ, NOTIFY)]
        #[qproperty(bool, favorites_ready, cxx_name = "favoritesReady", READ, NOTIFY)]
        #[qproperty(i32, favorites_revision, cxx_name = "favoritesRevision", READ, NOTIFY)]
        #[qproperty(i32, notices_revision, cxx_name = "noticesRevision", READ, NOTIFY)]
        #[qproperty(i32, notices_open_requests, cxx_name = "noticesOpenRequests", READ, NOTIFY)]
        #[qproperty(i32, bindings_revision, cxx_name = "bindingsRevision", READ, NOTIFY)]
        #[qproperty(QString, startup_failure, cxx_name = "startupFailure", READ, NOTIFY)]
        type WorkspaceBridge = super::WorkspaceBridgeRust;

        /// Emitted after any call that may have changed what a binder shows.
        #[qsignal]
        #[cxx_name = "sessionChanged"]
        fn session_changed(self: Pin<&mut WorkspaceBridge>);

        fn start(self: Pin<&mut WorkspaceBridge>, startup: Box<BrowserStartup>);
        fn drain(self: Pin<&mut WorkspaceBridge>) -> bool;
        fn refresh(self: Pin<&mut WorkspaceBridge>);
        #[cxx_name = "setRootVolumeName"]
        fn set_root_volume_name(self: Pin<&mut WorkspaceBridge>, name: &QString);

        #[cxx_name = "groupCount"]
        fn group_count(self: &WorkspaceBridge) -> i32;
        #[cxx_name = "groupId"]
        fn group_id(self: &WorkspaceBridge, group: i32) -> i64;
        #[cxx_name = "groupName"]
        fn group_name(self: &WorkspaceBridge, group: i32) -> QString;
        #[cxx_name = "itemCount"]
        fn item_count(self: &WorkspaceBridge, group: i32) -> i32;
        #[cxx_name = "itemId"]
        fn item_id(self: &WorkspaceBridge, group: i32, item: i32) -> i64;
        #[cxx_name = "itemAlias"]
        fn item_alias(self: &WorkspaceBridge, group: i32, item: i32) -> QString;
        #[cxx_name = "itemPath"]
        fn item_path(self: &WorkspaceBridge, group: i32, item: i32) -> QString;

        /// Each edit returns inline feedback, empty when the edit was accepted
        /// or changed nothing.
        #[cxx_name = "createGroup"]
        fn create_group(self: Pin<&mut WorkspaceBridge>, name: &QString) -> QString;
        #[cxx_name = "renameGroup"]
        fn rename_group(self: Pin<&mut WorkspaceBridge>, id: i64, name: &QString) -> QString;
        /// `motion`: 0 up, 1 down, 2 to top, 3 to bottom.
        #[cxx_name = "moveGroup"]
        fn move_group(self: Pin<&mut WorkspaceBridge>, id: i64, motion: i32);
        #[cxx_name = "deleteGroup"]
        fn delete_group(self: Pin<&mut WorkspaceBridge>, id: i64);
        #[cxx_name = "addItem"]
        fn add_item(self: Pin<&mut WorkspaceBridge>, group: i64) -> QString;
        #[cxx_name = "renameItem"]
        fn rename_item(self: Pin<&mut WorkspaceBridge>, id: i64, name: &QString) -> QString;
        #[cxx_name = "removeItem"]
        fn remove_item(self: Pin<&mut WorkspaceBridge>, id: i64);
        #[cxx_name = "dropItem"]
        fn drop_item(self: Pin<&mut WorkspaceBridge>, id: i64, group: i64, slot: i32) -> QString;
        #[cxx_name = "openItem"]
        fn open_item(self: Pin<&mut WorkspaceBridge>, id: i64);

        #[cxx_name = "noticeCount"]
        fn notice_count(self: &WorkspaceBridge) -> i32;
        #[cxx_name = "noticeText"]
        fn notice_text(self: &WorkspaceBridge, notice: i32) -> QString;
        #[cxx_name = "noticeOffersReset"]
        fn notice_offers_reset(self: &WorkspaceBridge, notice: i32) -> bool;
        /// Whether the Notice offers Try Again for the safety journal.
        #[cxx_name = "noticeOffersJournalRetry"]
        fn notice_offers_journal_retry(self: &WorkspaceBridge, notice: i32) -> bool;
        #[cxx_name = "retryJournal"]
        fn retry_journal(self: Pin<&mut WorkspaceBridge>);
        /// Resets settings after the person confirmed it.
        #[cxx_name = "resetSettings"]
        fn reset_settings(self: Pin<&mut WorkspaceBridge>);

        #[cxx_name = "bindingCount"]
        fn binding_count(self: &WorkspaceBridge) -> i32;
        #[cxx_name = "bindingAction"]
        fn binding_action(self: &WorkspaceBridge, binding: i32) -> QString;
        /// The binding as Qt portable key-sequence text, empty when unbound.
        #[cxx_name = "bindingSequence"]
        fn binding_sequence(self: &WorkspaceBridge, binding: i32) -> QString;
        /// Where the binding acts: 0 for a focused Folder Items List, 1 for
        /// the workspace window, 2 for every window, or -1 for no binding.
        #[cxx_name = "bindingScope"]
        fn binding_scope(self: &WorkspaceBridge, binding: i32) -> i32;
    }
}

/// The one session both Browser models and the workspace binder share. Only
/// the GUI thread uses it, so the mutex is a holder rather than
/// synchronization; its order comes from the GUI thread's event loop. Each
/// caller releases it before notifying Qt, which may call back into a binder.
static SESSION: OnceLock<Mutex<Option<WorkspaceSession>>> = OnceLock::new();

/// Runs `action` on the session, if it has started.
pub(crate) fn with_session<T>(action: impl FnOnce(&mut WorkspaceSession) -> T) -> Option<T> {
    SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_mut().map(action)
}

/// Ends the session after the Qt event loop has exited: unsaved settings get
/// a bounded flush, and outstanding reads are cancelled.
fn shutdown_desktop() {
    let session = SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    if let Some(session) = session {
        session.shutdown(SHUTDOWN_TIMEOUT);
    }
}

/// Converts a revision counter to a Qt `int` that changes whenever it does.
pub(crate) fn revision(value: u64) -> i32 {
    value as i32
}

#[derive(Default)]
pub struct WorkspaceBridgeRust {
    active_browser: i32,
    favorites_ready: bool,
    favorites_revision: i32,
    notices_revision: i32,
    notices_open_requests: i32,
    bindings_revision: i32,
    startup_failure: QString,
    shown: WorkspaceViewModel,
}

impl ffi::WorkspaceBridge {
    #[expect(clippy::boxed_local, reason = "CXX passes an opaque Rust value from C++ only in a Box")]
    fn start(mut self: Pin<&mut Self>, startup: Box<BrowserStartup>) {
        let BrowserStartup { home, settings_path, source_factory, location_probe, launch } = *startup;
        match Runtime::start(source_factory, location_probe, Box::new(ffi::schedule_gui_drain)) {
            Ok(mut runtime) => {
                // Without the lane, operations fail closed with a Notice.
                let journal = Journal::new(settings_path.as_deref().map(journal_path), launch);
                runtime.attach_operations(Services { fs: std::sync::Arc::new(NativeFileSystem), journal, budget: STEP_BUDGET }).ok();
                let settings = settings_path.and_then(|path| SettingsWorker::start_with_wake(path, Box::new(ffi::schedule_gui_drain)).ok());
                let coordinator = WorkspaceSession::with_settings(runtime, home, settings, DRAIN_SLICE, DRAIN_TIME_BUDGET);
                *SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(coordinator);
                self.as_mut().session_changed();
            }
            Err(_) => self.set_startup_failure(QString::from(reader_start_failure_status())),
        }
    }

    fn drain(mut self: Pin<&mut Self>) -> bool {
        let more = with_session(WorkspaceSession::drain).unwrap_or(false);
        self.as_mut().session_changed();
        more
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some(view) = with_session(|session| session.workspace_view().clone()) else { return };
        // The session lock is released, so Qt may call back into this binder
        // while the notifications below are delivered.
        let favorites = revision(view.favorites_revision());
        let notices = revision(view.notices_revision());
        let opened = revision(view.open_requests());
        let bindings = revision(view.bindings_revision());
        let active = i32::from(view.active_browser() == BrowserSide::Right);
        let ready = view.favorites_ready();
        self.as_mut().rust_mut().get_mut().shown = view;
        self.as_mut().set_active_browser(active);
        self.as_mut().set_favorites_ready(ready);
        self.as_mut().set_favorites_revision(favorites);
        self.as_mut().set_bindings_revision(bindings);
        self.as_mut().set_notices_revision(notices);
        self.as_mut().set_notices_open_requests(opened);
    }

    fn set_root_volume_name(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        with_session(|session| session.set_root_label(&name));
        self.as_mut().session_changed();
    }

    fn group_count(&self) -> i32 {
        count(self.rust().shown.groups().len())
    }
    fn group_id(&self, group: i32) -> i64 {
        self.group(group).map_or(-1, |group| group.id)
    }
    fn group_name(&self, group: i32) -> QString {
        self.group(group).map_or_else(QString::default, |group| QString::from(group.name.as_str()))
    }
    fn item_count(&self, group: i32) -> i32 {
        self.group(group).map_or(0, |group| count(group.items.len()))
    }
    fn item_id(&self, group: i32, item: i32) -> i64 {
        self.item(group, item).map_or(-1, |item| item.id)
    }
    fn item_alias(&self, group: i32, item: i32) -> QString {
        self.item(group, item).map_or_else(QString::default, |item| QString::from(item.alias.as_str()))
    }
    fn item_path(&self, group: i32, item: i32) -> QString {
        self.item(group, item).map_or_else(QString::default, |item| QString::from(item.path.as_str()))
    }
    fn group(&self, group: i32) -> Option<&dual_pane_adapters::FavoriteGroupViewModel> {
        usize::try_from(group).ok().and_then(|group| self.rust().shown.groups().get(group))
    }
    fn item(&self, group: i32, item: i32) -> Option<&dual_pane_adapters::FavoriteItemViewModel> {
        let item = usize::try_from(item).ok()?;
        self.group(group)?.items.get(item)
    }

    fn create_group(self: Pin<&mut Self>, name: &QString) -> QString {
        self.favorites(FavoritesEvent::CreateGroup { name: name.to_string() })
    }
    fn rename_group(self: Pin<&mut Self>, id: i64, name: &QString) -> QString {
        self.favorites(FavoritesEvent::RenameGroup { id, name: name.to_string() })
    }
    fn move_group(self: Pin<&mut Self>, id: i64, motion: i32) {
        let motion = match motion {
            0 => GroupMotion::Up,
            1 => GroupMotion::Down,
            2 => GroupMotion::Top,
            _ => GroupMotion::Bottom,
        };
        self.favorites(FavoritesEvent::MoveGroup { id, motion });
    }
    fn delete_group(self: Pin<&mut Self>, id: i64) {
        self.favorites(FavoritesEvent::DeleteGroup { id });
    }
    fn add_item(self: Pin<&mut Self>, group: i64) -> QString {
        self.favorites(FavoritesEvent::AddItem { group_id: group })
    }
    fn rename_item(self: Pin<&mut Self>, id: i64, name: &QString) -> QString {
        self.favorites(FavoritesEvent::RenameItem { id, name: name.to_string() })
    }
    fn remove_item(self: Pin<&mut Self>, id: i64) {
        self.favorites(FavoritesEvent::RemoveItem { id });
    }
    fn drop_item(self: Pin<&mut Self>, id: i64, group: i64, slot: i32) -> QString {
        let Ok(slot) = usize::try_from(slot) else { return QString::default() };
        self.favorites(FavoritesEvent::DropItem { id, group_id: group, slot })
    }
    fn open_item(self: Pin<&mut Self>, id: i64) {
        self.favorites(FavoritesEvent::OpenItem { id });
    }
    /// Submits a Sidebar gesture and returns its inline feedback.
    fn favorites(mut self: Pin<&mut Self>, event: FavoritesEvent) -> QString {
        let rejection = with_session(|session| session.submit_favorites(event)).flatten();
        self.as_mut().session_changed();
        rejection.and_then(|(edit, reason)| favorite_rejection_text(edit, reason)).map_or_else(QString::default, QString::from)
    }

    fn notice_count(&self) -> i32 {
        count(self.rust().shown.notices().len())
    }
    fn notice_text(&self, notice: i32) -> QString {
        self.notice(notice).map_or_else(QString::default, |notice| QString::from(notice.text.as_str()))
    }
    fn notice_offers_reset(&self, notice: i32) -> bool {
        self.notice(notice).is_some_and(|notice| notice.offers_reset)
    }
    fn notice_offers_journal_retry(&self, notice: i32) -> bool {
        self.notice(notice).is_some_and(|notice| notice.offers_journal_retry)
    }
    fn retry_journal(mut self: Pin<&mut Self>) {
        with_session(|session| session.submit(dual_pane_application::Command::RetryJournal));
        self.as_mut().session_changed();
    }
    fn notice(&self, notice: i32) -> Option<&dual_pane_adapters::NoticeViewModel> {
        usize::try_from(notice).ok().and_then(|notice| self.rust().shown.notices().get(notice))
    }
    fn reset_settings(mut self: Pin<&mut Self>) {
        with_session(|session| session.submit(dual_pane_application::Command::ResetSettings));
        self.as_mut().session_changed();
    }

    fn binding_count(&self) -> i32 {
        count(self.rust().shown.bindings().len())
    }
    fn binding_action(&self, binding: i32) -> QString {
        usize::try_from(binding).ok().and_then(|binding| self.rust().shown.bindings().get(binding)).map_or_else(QString::default, |binding| QString::from(binding.action.as_str()))
    }
    fn binding_sequence(&self, binding: i32) -> QString {
        usize::try_from(binding).ok().and_then(|binding| self.rust().shown.bindings().get(binding)).and_then(|binding| binding.shortcut).map_or_else(QString::default, |shortcut| QString::from(key_sequence(shortcut).as_str()))
    }
    fn binding_scope(&self, binding: i32) -> i32 {
        usize::try_from(binding).ok().and_then(|binding| self.rust().shown.bindings().get(binding)).map_or(-1, |binding| scope_code(binding.action.scope()))
    }

    fn set_active_browser(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().active_browser != value {
            self.as_mut().rust_mut().get_mut().active_browser = value;
            self.active_browser_changed();
        }
    }
    fn set_favorites_ready(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().favorites_ready != value {
            self.as_mut().rust_mut().get_mut().favorites_ready = value;
            self.favorites_ready_changed();
        }
    }
    fn set_favorites_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().favorites_revision != value {
            self.as_mut().rust_mut().get_mut().favorites_revision = value;
            self.favorites_revision_changed();
        }
    }
    fn set_notices_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().notices_revision != value {
            self.as_mut().rust_mut().get_mut().notices_revision = value;
            self.notices_revision_changed();
        }
    }
    fn set_notices_open_requests(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().notices_open_requests != value {
            self.as_mut().rust_mut().get_mut().notices_open_requests = value;
            self.notices_open_requests_changed();
        }
    }
    fn set_bindings_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().bindings_revision != value {
            self.as_mut().rust_mut().get_mut().bindings_revision = value;
            self.bindings_revision_changed();
        }
    }
    fn set_startup_failure(mut self: Pin<&mut Self>, value: QString) {
        if self.rust().startup_failure != value {
            self.as_mut().rust_mut().get_mut().startup_failure = value;
            self.startup_failure_changed();
        }
    }
}

fn count(length: usize) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

/// The Qt code for `scope`, matching the `scope_*` constants in
/// `desktop_window.cpp`.
fn scope_code(scope: ShortcutScope) -> i32 {
    match scope {
        ShortcutScope::FolderItemsList => 0,
        ShortcutScope::Window => 1,
        ShortcutScope::Application => 2,
    }
}

/// Qt portable key-sequence text for `shortcut`. Qt names the Command key
/// `Ctrl` and the Control key `Meta` on macOS.
pub(crate) fn key_sequence(shortcut: Shortcut) -> String {
    let mut text = String::new();
    for (held, name) in [(shortcut.control, "Meta+"), (shortcut.option, "Alt+"), (shortcut.shift, "Shift+"), (shortcut.command, "Ctrl+")] {
        if held {
            text.push_str(name);
        }
    }
    match shortcut.key {
        Key::Character(key) => text.push(key),
        Key::Up => text.push_str("Up"),
        Key::Down => text.push_str("Down"),
        Key::Left => text.push_str("Left"),
        Key::Right => text.push_str("Right"),
        Key::Return => text.push_str("Return"),
        Key::Delete => text.push_str("Backspace"),
        Key::Function(number) => text.push_str(&format!("F{number}")),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_application::{ActionId, default_shortcut};

    #[test]
    fn delivered_shortcuts_map_to_qt_key_sequences() {
        let sequence = |action| default_shortcut(action).map(key_sequence);
        assert_eq!(sequence(ActionId::NewTab).as_deref(), Some("Ctrl+T"));
        assert_eq!(sequence(ActionId::CloseTab).as_deref(), Some("Ctrl+W"));
        assert_eq!(sequence(ActionId::NavigateBack).as_deref(), Some("Alt+Left"));
        assert_eq!(sequence(ActionId::NavigateForward).as_deref(), Some("Alt+Right"));
        assert_eq!(sequence(ActionId::RefreshFolder).as_deref(), Some("Ctrl+R"));
        assert_eq!(sequence(ActionId::FocusOtherBrowser).as_deref(), Some("Alt+F"));
        assert_eq!(sequence(ActionId::NavigateParent).as_deref(), Some("Ctrl+Up"));
        assert_eq!(sequence(ActionId::QuitApplication).as_deref(), Some("Ctrl+Q"));
        assert_eq!(sequence(ActionId::NewFolder).as_deref(), Some("Shift+Ctrl+N"));
        assert_eq!(sequence(ActionId::CloseWindow), None);
        assert_eq!(key_sequence(Shortcut::new(true, false, true, true, Key::Delete)), "Meta+Alt+Ctrl+Backspace");
        assert_eq!(key_sequence(Shortcut::new(false, false, false, false, Key::Function(2))), "F2");
    }

    #[test]
    fn scope_codes_match_the_qt_constants() {
        assert_eq!(scope_code(ActionId::MoveToTrash.scope()), 0, "scope_folder_items_list");
        assert_eq!(scope_code(ActionId::NewFolder.scope()), 1, "scope_window");
        assert_eq!(scope_code(ActionId::QuitApplication.scope()), 2, "scope_application");
    }
}
