//! The Qt binder for workspace-wide presentation: the Sidebar Favorites,
//! Notices, effective shortcuts, and the active Browser. It also owns the one
//! session that both Browser models share.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use dual_pane_adapters::{FavoritesEvent, GroupMotion, WorkspaceViewModel, action_label, context_menu_rejection_text, favorite_rejection_text, reader_start_failure_status};
use dual_pane_application::{ActionId, Command, ContextMenuKind, ContextMenuPreferenceRejection, FileAssociationTypeView, Key, Shortcut, ShortcutRejection, ShortcutScope, WindowFrame, WindowLayout, WindowState, action_spec, default_shortcuts};
use dual_pane_domain::BrowserSide;

use crate::browser_session::{BrowserStartup, DRAIN_SLICE, DRAIN_TIME_BUDGET, SHUTDOWN_TIMEOUT, WorkspaceSession};
use crate::instance::{PendingRequest, Reply, Request, Server};
use crate::launch_target::LaunchTarget;
use crate::operation_journal::{Journal, journal_path};
use crate::operation_lane::{STEP_BUDGET, Services};
use crate::operation_step::NativeFileSystem;
use crate::runtime::Runtime;
use crate::settings_storage::SettingsWorker;

mod context_menus;
mod desktop_session;
mod favorites;
mod file_associations;
mod notices;
mod settings;
mod shortcuts;
#[cfg(test)]
mod tests;

use context_menus::*;
pub(crate) use desktop_session::with_session;
use desktop_session::*;
pub(crate) use shortcuts::key_sequence;

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
pub mod ffi {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;
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
        include!("dual_pane_desktop/clipboard_adapter.hpp");
        fn clipboard_generation() -> i64;
        fn clipboard_offer() -> i32;
        fn clipboard_publish(request: &QByteArray) -> QByteArray;
        fn clipboard_read() -> QByteArray;
        #[allow(dead_code)] // Called by the pasteboard adapter tests in `clipboard_native`.
        fn clipboard_isolation_begin();
        #[allow(dead_code)] // Called by the pasteboard adapter tests in `clipboard_native`.
        fn clipboard_isolation_end();
        #[allow(dead_code)] // Called by the pasteboard adapter tests in `clipboard_native`.
        fn clipboard_seam_write(kind: i32, payload: &QByteArray) -> bool;
        #[allow(dead_code)] // Called by the pasteboard adapter tests in `clipboard_native`.
        fn clipboard_seam_describe() -> QByteArray;
        #[allow(dead_code)] // Called by the pasteboard adapter tests in `clipboard_native`.
        fn clipboard_seam_private_payload() -> QByteArray;
        include!("dual_pane_desktop/preview_reader.hpp");
        fn decode_preview(fd: i32, target_width: i32, target_height: i32) -> QByteArray;
        include!("dual_pane_desktop/quick_look_preview.hpp");
        #[allow(dead_code)] // Called by the desktop-only Rust seam test below.
        fn quick_look_seam_tests() -> bool;
    }
    unsafe extern "RustQt" {
        #[qobject]
        /// 0 for the Left Browser, 1 for the Right Browser.
        #[qproperty(i32, active_browser, cxx_name = "activeBrowser", READ, NOTIFY)]
        #[qproperty(bool, favorites_ready, cxx_name = "favoritesReady", READ, NOTIFY)]
        #[qproperty(i32, favorites_revision, cxx_name = "favoritesRevision", READ, NOTIFY)]
        #[qproperty(i32, notices_revision, cxx_name = "noticesRevision", READ, NOTIFY)]
        #[qproperty(i32, notices_open_requests, cxx_name = "noticesOpenRequests", READ, NOTIFY)]
        #[qproperty(bool, hide_notices_at_startup, cxx_name = "hideNoticesAtStartup", READ, NOTIFY)]
        #[qproperty(bool, skip_quit_confirmation, cxx_name = "skipQuitConfirmation", READ, NOTIFY)]
        #[qproperty(bool, show_macos_items, cxx_name = "showMacosItems", READ, NOTIFY)]
        #[qproperty(bool, show_dotfiles, cxx_name = "showDotfiles", READ, NOTIFY)]
        #[qproperty(bool, show_macos_invisible, cxx_name = "showMacosInvisible", READ, NOTIFY)]
        #[qproperty(bool, notices_startup_ready, cxx_name = "noticesStartupReady", READ, NOTIFY)]
        #[qproperty(bool, settings_interaction_available, cxx_name = "settingsInteractionAvailable", READ, NOTIFY)]
        #[qproperty(i32, layout_revision, cxx_name = "layoutRevision", READ, NOTIFY)]
        #[qproperty(bool, saved_layout_available, cxx_name = "savedLayoutAvailable", READ, NOTIFY)]
        #[qproperty(i32, saved_layout_x, cxx_name = "savedLayoutX", READ, NOTIFY)]
        #[qproperty(i32, saved_layout_y, cxx_name = "savedLayoutY", READ, NOTIFY)]
        #[qproperty(i32, saved_layout_width, cxx_name = "savedLayoutWidth", READ, NOTIFY)]
        #[qproperty(i32, saved_layout_height, cxx_name = "savedLayoutHeight", READ, NOTIFY)]
        #[qproperty(i32, saved_layout_state, cxx_name = "savedLayoutState", READ, NOTIFY)]
        #[qproperty(i32, saved_sidebar_splitter, cxx_name = "savedSidebarSplitter", READ, NOTIFY)]
        #[qproperty(i32, saved_browser_splitter, cxx_name = "savedBrowserSplitter", READ, NOTIFY)]
        #[qproperty(i32, saved_left_preview_height, cxx_name = "savedLeftPreviewHeight", READ, NOTIFY)]
        #[qproperty(i32, saved_right_preview_height, cxx_name = "savedRightPreviewHeight", READ, NOTIFY)]
        #[qproperty(i32, bindings_revision, cxx_name = "bindingsRevision", READ, NOTIFY)]
        #[qproperty(i32, context_menus_revision, cxx_name = "contextMenusRevision", READ, NOTIFY)]
        #[qproperty(i32, context_menu_rejection_revision, cxx_name = "contextMenuRejectionRevision", READ, NOTIFY)]
        #[qproperty(i32, file_associations_revision, cxx_name = "fileAssociationsRevision", READ, NOTIFY)]
        #[qproperty(bool, file_associations_pending, cxx_name = "fileAssociationsPending", READ, NOTIFY)]
        #[qproperty(QString, file_associations_error, cxx_name = "fileAssociationsError", READ, NOTIFY)]
        #[qproperty(i32, file_association_open_requests, cxx_name = "fileAssociationOpenRequests", READ, NOTIFY)]
        #[qproperty(QString, startup_failure, cxx_name = "startupFailure", READ, NOTIFY)]
        type WorkspaceBridge = super::WorkspaceBridgeRust;

        /// Emitted after any call that may have changed what a binder shows.
        #[qsignal]
        #[cxx_name = "sessionChanged"]
        fn session_changed(self: Pin<&mut WorkspaceBridge>);

        fn start(self: Pin<&mut WorkspaceBridge>, startup: Box<BrowserStartup>);
        fn drain(self: Pin<&mut WorkspaceBridge>) -> bool;
        fn refresh(self: Pin<&mut WorkspaceBridge>);
        /// -2: none, -1: raise only, 0: focus Left, 1: focus Right.
        #[cxx_name = "takeActivationRequest"]
        fn take_activation_request(self: Pin<&mut WorkspaceBridge>) -> i32;
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
        #[cxx_name = "changeHideNoticesAtStartup"]
        fn change_hide_notices_at_startup(self: Pin<&mut WorkspaceBridge>, hide: bool);
        #[cxx_name = "changeSkipQuitConfirmation"]
        fn change_skip_quit_confirmation(self: Pin<&mut WorkspaceBridge>, skip: bool);
        #[cxx_name = "changeVisibility"]
        fn change_visibility(self: Pin<&mut WorkspaceBridge>, show_macos_items: bool, show_dotfiles: bool, show_macos_invisible: bool);
        #[cxx_name = "startupPeriodElapsed"]
        fn startup_period_elapsed(self: Pin<&mut WorkspaceBridge>);
        #[cxx_name = "settingsLoadTimedOut"]
        fn settings_load_timed_out(self: Pin<&mut WorkspaceBridge>);
        /// Pauses watcher-driven listing work while the application is not
        /// foreground-active; native subscriptions remain registered.
        #[cxx_name = "setApplicationActive"]
        fn set_application_active(self: Pin<&mut WorkspaceBridge>, active: bool);
        #[cxx_name = "observeClipboard"]
        fn observe_clipboard(self: Pin<&mut WorkspaceBridge>);
        /// Reports the workspace's current logical-point layout after a
        /// settled native layout change.
        #[cxx_name = "updateWindowLayout"]
        fn update_window_layout(self: Pin<&mut WorkspaceBridge>, origin: i64, width: i32, height: i32, state: i32, splitters: i64, previews: i64);
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
        #[cxx_name = "actionCount"]
        fn action_count(self: &WorkspaceBridge) -> i32;
        #[cxx_name = "actionId"]
        fn action_id(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "actionScope"]
        fn action_scope(self: &WorkspaceBridge, index: i32) -> i32;
        #[cxx_name = "actionCategory"]
        fn action_category(self: &WorkspaceBridge, index: i32) -> i32;
        #[cxx_name = "actionShortcutCount"]
        fn action_shortcut_count(self: &WorkspaceBridge, index: i32) -> i32;
        #[cxx_name = "actionShortcutLabel"]
        fn action_shortcut_label(self: &WorkspaceBridge, index: i32, position: i32) -> QString;
        #[cxx_name = "actionDefaultShortcutLabel"]
        fn action_default_shortcut_label(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "addActionShortcut"]
        fn add_action_shortcut(self: Pin<&mut WorkspaceBridge>, index: i32, key: i32, modifiers: i32) -> QString;
        #[cxx_name = "removeActionShortcut"]
        fn remove_action_shortcut(self: Pin<&mut WorkspaceBridge>, index: i32, position: i32) -> QString;
        #[cxx_name = "restoreActionShortcuts"]
        fn restore_action_shortcuts(self: Pin<&mut WorkspaceBridge>, index: i32) -> QString;

        #[cxx_name = "contextMenuActionCount"]
        fn context_menu_action_count(self: &WorkspaceBridge) -> i32;
        #[cxx_name = "contextMenuActionLabel"]
        fn context_menu_action_label(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "contextMenuGroupLabel"]
        fn context_menu_group_label(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "contextMenuNormalVisible"]
        fn context_menu_normal_visible(self: &WorkspaceBridge, index: i32) -> bool;
        #[cxx_name = "contextMenuComprehensiveVisible"]
        fn context_menu_comprehensive_visible(self: &WorkspaceBridge, index: i32) -> bool;
        /// The first effective shortcut as Qt portable key-sequence text.
        #[cxx_name = "contextMenuShortcut"]
        fn context_menu_shortcut(self: &WorkspaceBridge, index: i32) -> QString;
        /// `kind`: 0 Normal, 1 Comprehensive. Returns inline feedback.
        #[cxx_name = "changeContextMenuVisibility"]
        fn change_context_menu_visibility(self: Pin<&mut WorkspaceBridge>, index: i32, kind: i32, visible: bool) -> QString;
        /// `kind`: 0 Normal, 1 Comprehensive.
        #[cxx_name = "resetContextMenu"]
        fn reset_context_menu(self: Pin<&mut WorkspaceBridge>, kind: i32);
        #[cxx_name = "resetContextMenus"]
        fn reset_context_menus(self: Pin<&mut WorkspaceBridge>);
        #[cxx_name = "contextMenuRejectionText"]
        fn context_menu_rejection_text(self: &WorkspaceBridge) -> QString;

        #[cxx_name = "fileAssociationTypeCount"]
        fn file_association_type_count(self: &WorkspaceBridge) -> i32;
        #[cxx_name = "fileAssociationTypeIdentifier"]
        fn file_association_type_identifier(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "fileAssociationTypeKind"]
        fn file_association_type_kind(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "fileAssociationTypeExtension"]
        fn file_association_type_extension(self: &WorkspaceBridge, index: i32) -> QString;
        #[cxx_name = "fileAssociationTypePinned"]
        fn file_association_type_pinned(self: &WorkspaceBridge, index: i32) -> bool;
        #[cxx_name = "fileAssociationCandidateCount"]
        fn file_association_candidate_count(self: &WorkspaceBridge, type_index: i32) -> i32;
        #[cxx_name = "fileAssociationCandidateName"]
        fn file_association_candidate_name(self: &WorkspaceBridge, type_index: i32, app_index: i32) -> QString;
        #[cxx_name = "fileAssociationCandidateLocation"]
        fn file_association_candidate_location(self: &WorkspaceBridge, type_index: i32, app_index: i32) -> QString;
        #[cxx_name = "fileAssociationCandidateAvailable"]
        fn file_association_candidate_available(self: &WorkspaceBridge, type_index: i32, app_index: i32) -> bool;
        #[cxx_name = "fileAssociationCandidateNormal"]
        fn file_association_candidate_normal(self: &WorkspaceBridge, type_index: i32, app_index: i32) -> bool;
        #[cxx_name = "fileAssociationCandidateComprehensive"]
        fn file_association_candidate_comprehensive(self: &WorkspaceBridge, type_index: i32, app_index: i32) -> bool;
        #[cxx_name = "fileAssociationCandidateDefault"]
        fn file_association_candidate_default(self: &WorkspaceBridge, type_index: i32, app_index: i32) -> bool;
        #[cxx_name = "addFileAssociationSample"]
        fn add_file_association_sample(self: Pin<&mut WorkspaceBridge>, path: &QByteArray);
        #[cxx_name = "refreshFileAssociations"]
        fn refresh_file_associations(self: Pin<&mut WorkspaceBridge>);
        #[cxx_name = "changeFileAssociationSelection"]
        fn change_file_association_selection(self: Pin<&mut WorkspaceBridge>, type_index: i32, app_index: i32, kind: i32, selected: bool);
        #[cxx_name = "clearFileAssociation"]
        fn clear_file_association(self: Pin<&mut WorkspaceBridge>, type_index: i32);
        #[cxx_name = "removeFileAssociation"]
        fn remove_file_association(self: Pin<&mut WorkspaceBridge>, type_index: i32);
        #[cxx_name = "setDefaultApplication"]
        fn set_default_application(self: Pin<&mut WorkspaceBridge>, type_index: i32, app_index: i32);
        #[cxx_name = "takeFileAssociationType"]
        fn take_file_association_type(self: Pin<&mut WorkspaceBridge>) -> QString;
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
    hide_notices_at_startup: bool,
    skip_quit_confirmation: bool,
    show_macos_items: bool,
    show_dotfiles: bool,
    show_macos_invisible: bool,
    notices_startup_ready: bool,
    settings_interaction_available: bool,
    layout_revision: i32,
    saved_layout_available: bool,
    saved_layout_x: i32,
    saved_layout_y: i32,
    saved_layout_width: i32,
    saved_layout_height: i32,
    saved_layout_state: i32,
    saved_sidebar_splitter: i32,
    saved_browser_splitter: i32,
    saved_left_preview_height: i32,
    saved_right_preview_height: i32,
    bindings_revision: i32,
    context_menus_revision: i32,
    context_menu_rejection_revision: i32,
    file_associations_revision: i32,
    file_associations_pending: bool,
    file_associations_error: QString,
    file_association_open_requests: i32,
    file_association_types: Vec<FileAssociationTypeView>,
    pending_file_association_types: VecDeque<Option<String>>,
    startup_failure: QString,
    shown: WorkspaceViewModel,
}

impl ffi::WorkspaceBridge {
    #[expect(clippy::boxed_local, reason = "CXX passes an opaque Rust value from C++ only in a Box")]
    fn start(mut self: Pin<&mut Self>, startup: Box<BrowserStartup>) {
        let BrowserStartup { home, settings_path, source_factory, location_probe, launch, target, receiver } = *startup;
        match Runtime::start(source_factory, location_probe, Box::new(ffi::schedule_gui_drain)) {
            Ok(mut runtime) => {
                // Without the lane, operations fail closed with a Notice.
                let journal = Journal::new(settings_path.as_deref().map(journal_path), launch);
                runtime.attach_operations(Services { fs: std::sync::Arc::new(NativeFileSystem), journal, budget: STEP_BUDGET }).ok();
                let settings = settings_path.and_then(|path| SettingsWorker::start_with_wake(path, Box::new(ffi::schedule_gui_drain)).ok());
                let mut coordinator = WorkspaceSession::with_settings(runtime, home, settings, DRAIN_SLICE, DRAIN_TIME_BUDGET);
                if let Some(LaunchTarget { folder, select }) = target {
                    coordinator.submit(Command::OpenLaunchTarget { folder, select });
                }
                *SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(coordinator);
                if let Some(receiver) = receiver {
                    let enqueue = Arc::new(|pending: PendingRequest| {
                        if INSTANCE_STOPPING.load(Ordering::Acquire) {
                            let _ = pending.reply.send(Reply::ShuttingDown);
                            return;
                        }
                        let mut queue = INSTANCE_REQUESTS.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        if queue.len() >= INSTANCE_QUEUE_CAPACITY {
                            drop(queue);
                            let _ = pending.reply.send(Reply::Refused);
                            return;
                        }
                        queue.push_back(pending);
                        drop(queue);
                        ffi::schedule_gui_drain();
                    });
                    match receiver.start(enqueue) {
                        Ok(server) => *INSTANCE_SERVER.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(server),
                        Err(error) => eprintln!("Dual Pane instance receiver could not start: {error}"),
                    }
                }
                self.as_mut().session_changed();
            }
            Err(_) => self.set_startup_failure(QString::from(reader_start_failure_status())),
        }
    }

    fn drain(mut self: Pin<&mut Self>) -> bool {
        let instance_more = drain_instance_requests();
        let more = with_session(WorkspaceSession::drain).unwrap_or(false) || instance_more;
        self.as_mut().session_changed();
        more
    }

    fn take_activation_request(self: Pin<&mut Self>) -> i32 {
        with_session(WorkspaceSession::take_activation_request).flatten().map_or(-2, |browser| match browser {
            Some(BrowserSide::Left) => 0,
            Some(BrowserSide::Right) => 1,
            None => -1,
        })
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some((view, layout, association_types, association_state, association_requests)) = with_session(|session| {
            let mut requests = Vec::new();
            while let Some(request) = session.take_association_settings_request() {
                requests.push(request);
            }
            (session.workspace_view().clone(), session.take_layout_request(), session.file_association_types(), session.file_association_state(), requests)
        }) else {
            return;
        };
        // The session lock is released, so Qt may call back into this binder
        // while the notifications below are delivered.
        let favorites = revision(view.favorites_revision());
        let notices = revision(view.notices_revision());
        let opened = revision(view.open_requests());
        let bindings = revision(view.bindings_revision());
        let context_menus = revision(view.context_menus_revision());
        let context_menu_rejection = revision(view.context_menu_rejection_revision());
        let association_revision = revision(association_state.0);
        let active = i32::from(view.active_browser() == BrowserSide::Right);
        let ready = view.favorites_ready();
        let hide_notices = view.hide_notices_at_startup();
        let skip_quit_confirmation = view.skip_quit_confirmation();
        let visibility = view.visibility();
        let notices_ready = view.notices_startup_ready();
        let settings_available = view.settings_interaction_available();
        self.as_mut().rust_mut().get_mut().shown = view;
        self.as_mut().set_active_browser(active);
        self.as_mut().set_favorites_ready(ready);
        self.as_mut().set_favorites_revision(favorites);
        self.as_mut().set_bindings_revision(bindings);
        self.as_mut().set_context_menus_revision(context_menus);
        self.as_mut().set_context_menu_rejection_revision(context_menu_rejection);
        self.as_mut().rust_mut().get_mut().file_association_types = association_types;
        self.as_mut().set_file_associations_revision(association_revision);
        self.as_mut().set_file_associations_pending(association_state.1);
        self.as_mut().set_file_associations_error(QString::from(association_state.2.as_deref().unwrap_or_default()));
        if !association_requests.is_empty() {
            let bridge = self.as_mut().rust_mut().get_mut();
            bridge.pending_file_association_types.extend(association_requests);
            bridge.file_association_open_requests = bridge.file_association_open_requests.wrapping_add(1);
            self.as_mut().file_association_open_requests_changed();
        }
        self.as_mut().set_notices_revision(notices);
        self.as_mut().set_notices_open_requests(opened);
        self.as_mut().set_hide_notices_at_startup(hide_notices);
        self.as_mut().set_skip_quit_confirmation(skip_quit_confirmation);
        self.as_mut().set_show_macos_items(visibility.show_macos_items);
        self.as_mut().set_show_dotfiles(visibility.show_dotfiles);
        self.as_mut().set_show_macos_invisible(visibility.show_macos_invisible);
        self.as_mut().set_notices_startup_ready(notices_ready);
        self.as_mut().set_settings_interaction_available(settings_available);
        if let Some(layout) = layout {
            let values = layout.map_or((false, 0, 0, 0, 0, 0, 0, 0, 0, 0), |layout| {
                let state = match layout.state {
                    WindowState::Normal => 0,
                    WindowState::Zoomed => 1,
                    WindowState::FullScreen => 2,
                };
                (true, layout.frame.x, layout.frame.y, layout.frame.width, layout.frame.height, state, layout.sidebar_splitter, layout.browser_splitter, layout.left_preview_height, layout.right_preview_height)
            });
            let bridge = self.as_mut().rust_mut().get_mut();
            bridge.saved_layout_available = values.0;
            bridge.saved_layout_x = values.1;
            bridge.saved_layout_y = values.2;
            bridge.saved_layout_width = values.3;
            bridge.saved_layout_height = values.4;
            bridge.saved_layout_state = values.5;
            bridge.saved_sidebar_splitter = values.6;
            bridge.saved_browser_splitter = values.7;
            bridge.saved_left_preview_height = values.8;
            bridge.saved_right_preview_height = values.9;
            self.as_mut().bump_layout_revision();
        }
    }

    fn set_root_volume_name(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        with_session(|session| session.set_root_label(&name));
        self.as_mut().session_changed();
    }
    fn observe_clipboard(mut self: Pin<&mut Self>) {
        let (generation, offer) = crate::clipboard_native::generation_and_offer();
        with_session(|session| session.observe_clipboard(generation, offer));
        self.as_mut().session_changed();
    }

    fn startup_period_elapsed(mut self: Pin<&mut Self>) {
        with_session(|session| session.submit(dual_pane_application::Event::StartupPeriodElapsed));
        self.as_mut().session_changed();
    }
    fn settings_load_timed_out(mut self: Pin<&mut Self>) {
        with_session(|session| session.submit(dual_pane_application::Event::SettingsLoadTimedOut));
        self.as_mut().session_changed();
    }
    fn set_application_active(mut self: Pin<&mut Self>, active: bool) {
        with_session(|session| session.submit(dual_pane_application::Event::ApplicationActivityChanged { active }));
        self.as_mut().session_changed();
    }
    fn update_window_layout(mut self: Pin<&mut Self>, origin: i64, width: i32, height: i32, state: i32, splitters: i64, previews: i64) {
        let x = (origin >> 32) as i32;
        let y = origin as i32;
        let sidebar_splitter = (splitters >> 32) as i32;
        let browser_splitter = splitters as i32;
        let left_preview_height = (previews >> 32) as i32;
        let right_preview_height = previews as i32;
        let state = match state {
            1 => WindowState::Zoomed,
            2 => WindowState::FullScreen,
            _ => WindowState::Normal,
        };
        let layout = WindowLayout { frame: WindowFrame { x, y, width, height }, state, sidebar_splitter, browser_splitter, left_preview_height, right_preview_height };
        with_session(|session| session.submit(dual_pane_application::Command::UpdateWindowLayout { layout }));
        self.as_mut().session_changed();
    }

    fn set_active_browser(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().active_browser != value {
            self.as_mut().rust_mut().get_mut().active_browser = value;
            self.active_browser_changed();
        }
    }
    fn bump_layout_revision(mut self: Pin<&mut Self>) {
        let next = self.rust().layout_revision.wrapping_add(1);
        self.as_mut().rust_mut().get_mut().layout_revision = next;
        self.layout_revision_changed();
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
