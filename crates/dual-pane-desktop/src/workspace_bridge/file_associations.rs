//! File Associations Settings projection and edits.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use super::*;

impl ffi::WorkspaceBridge {
    pub(super) fn file_association_type_count(&self) -> i32 {
        count(self.rust().file_association_types.len())
    }
    pub(super) fn file_association_type_identifier(&self, index: i32) -> QString {
        self.association_type(index).map_or_else(QString::default, |row| QString::from(row.content_type.identifier.as_str()))
    }
    pub(super) fn file_association_type_kind(&self, index: i32) -> QString {
        self.association_type(index).map_or_else(QString::default, |row| QString::from(row.content_type.localized_kind.as_str()))
    }
    pub(super) fn file_association_type_extension(&self, index: i32) -> QString {
        self.association_type(index).and_then(|row| row.content_type.preferred_extension.as_deref()).map_or_else(QString::default, QString::from)
    }
    pub(super) fn file_association_type_pinned(&self, index: i32) -> bool {
        self.association_type(index).is_some_and(|row| row.pinned)
    }
    pub(super) fn file_association_candidate_count(&self, type_index: i32) -> i32 {
        self.association_type(type_index).map_or(0, |row| count(row.candidates.len()))
    }
    pub(super) fn file_association_candidate_name(&self, type_index: i32, app_index: i32) -> QString {
        self.association_candidate(type_index, app_index).map_or_else(QString::default, |row| QString::from(row.display_name.as_str()))
    }
    pub(super) fn file_association_candidate_location(&self, type_index: i32, app_index: i32) -> QString {
        self.association_candidate(type_index, app_index).map_or_else(QString::default, |row| QString::from(row.location_hint.as_str()))
    }
    pub(super) fn file_association_candidate_bundle_identifier(&self, type_index: i32, app_index: i32) -> QString {
        self.association_candidate(type_index, app_index).map_or_else(QString::default, |row| QString::from(row.identity.bundle_identifier.as_str()))
    }
    pub(super) fn file_association_candidate_bundle_url(&self, type_index: i32, app_index: i32) -> QString {
        self.association_candidate(type_index, app_index).map_or_else(QString::default, |row| QString::from(row.identity.bundle_url.as_str()))
    }
    pub(super) fn file_association_candidate_available(&self, type_index: i32, app_index: i32) -> bool {
        self.association_candidate(type_index, app_index).is_some_and(|row| row.available)
    }
    pub(super) fn file_association_candidate_normal(&self, type_index: i32, app_index: i32) -> bool {
        self.association_candidate(type_index, app_index).is_some_and(|row| row.normal)
    }
    pub(super) fn file_association_candidate_comprehensive(&self, type_index: i32, app_index: i32) -> bool {
        self.association_candidate(type_index, app_index).is_some_and(|row| row.comprehensive)
    }
    pub(super) fn file_association_candidate_default(&self, type_index: i32, app_index: i32) -> bool {
        self.association_candidate(type_index, app_index).is_some_and(|row| row.is_default)
    }
    pub(super) fn add_file_association_sample(mut self: Pin<&mut Self>, path: &cxx_qt_lib::QByteArray) {
        let path = PathBuf::from(OsString::from_vec(path.as_ref().to_vec()));
        if let Some(location) = crate::native_location::location_from_path(&path) {
            with_session(|session| session.add_file_association_sample(location));
            self.as_mut().session_changed();
        }
    }
    pub(super) fn refresh_file_associations(mut self: Pin<&mut Self>) {
        with_session(WorkspaceSession::refresh_file_associations);
        self.as_mut().session_changed();
    }
    pub(super) fn change_file_association_selection(mut self: Pin<&mut Self>, type_index: i32, app_index: i32, kind: i32, selected: bool) {
        let Some((content_type, application)) = self.association_choice(type_index, app_index) else { return };
        with_session(|session| session.set_file_association_selected(content_type, context_menu_kind(kind), application, selected));
        self.as_mut().session_changed();
    }
    pub(super) fn clear_file_association(mut self: Pin<&mut Self>, type_index: i32) {
        let Some(content_type) = self.association_type(type_index).map(|row| row.content_type.identifier.clone()) else { return };
        with_session(|session| session.clear_file_association(content_type));
        self.as_mut().session_changed();
    }
    pub(super) fn remove_file_association(mut self: Pin<&mut Self>, type_index: i32) {
        let Some(content_type) = self.association_type(type_index).map(|row| row.content_type.identifier.clone()) else { return };
        with_session(|session| session.remove_file_association(content_type));
        self.as_mut().session_changed();
    }
    pub(super) fn set_default_application(mut self: Pin<&mut Self>, type_index: i32, app_index: i32) {
        let Some((content_type, application)) = self.association_choice(type_index, app_index) else { return };
        with_session(|session| session.set_default_application(dual_pane_application::DefaultApplicationRequest { content_type, application }));
        self.as_mut().session_changed();
    }
    pub(super) fn take_file_association_type(mut self: Pin<&mut Self>) -> QString {
        self.as_mut().rust_mut().get_mut().pending_file_association_types.pop_front().flatten().map_or_else(QString::default, |content_type| QString::from(content_type.as_str()))
    }
    fn association_type(&self, index: i32) -> Option<&FileAssociationTypeView> {
        usize::try_from(index).ok().and_then(|index| self.rust().file_association_types.get(index))
    }
    fn association_candidate(&self, type_index: i32, app_index: i32) -> Option<&dual_pane_application::FileAssociationCandidateView> {
        let row = self.association_type(type_index)?;
        usize::try_from(app_index).ok().and_then(|index| row.candidates.get(index))
    }
    fn association_choice(&self, type_index: i32, app_index: i32) -> Option<(String, dual_pane_application::ApplicationIdentity)> {
        Some((self.association_type(type_index)?.content_type.identifier.clone(), self.association_candidate(type_index, app_index)?.identity.clone()))
    }
    pub(super) fn set_file_associations_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().file_associations_revision != value {
            self.as_mut().rust_mut().get_mut().file_associations_revision = value;
            self.file_associations_revision_changed();
        }
    }
    pub(super) fn set_file_associations_pending(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().file_associations_pending != value {
            self.as_mut().rust_mut().get_mut().file_associations_pending = value;
            self.file_associations_pending_changed();
        }
    }
    pub(super) fn set_file_associations_error(mut self: Pin<&mut Self>, value: QString) {
        if self.rust().file_associations_error != value {
            self.as_mut().rust_mut().get_mut().file_associations_error = value;
            self.file_associations_error_changed();
        }
    }
}
