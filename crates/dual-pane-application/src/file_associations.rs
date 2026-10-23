use std::collections::HashSet;

use dual_pane_domain::Location;

use crate::ContextMenuKind;

pub const FILE_ASSOCIATION_LIMIT: usize = 256;
pub const FILE_ASSOCIATION_APP_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplicationIdentity {
    pub bundle_identifier: String,
    pub bundle_url: String,
}

impl ApplicationIdentity {
    pub fn valid(&self) -> bool {
        !self.bundle_identifier.is_empty() && self.bundle_identifier.len() <= 1024 && !self.bundle_identifier.chars().any(char::is_control) && self.bundle_url.starts_with("file://") && self.bundle_url.len() <= 16_384 && !self.bundle_url.chars().any(char::is_control)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationCandidate {
    pub identity: ApplicationIdentity,
    pub display_name: String,
    pub location_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentTypeDescription {
    pub identifier: String,
    pub localized_kind: String,
    pub preferred_extension: Option<String>,
}

impl ContentTypeDescription {
    pub fn valid(&self) -> bool {
        valid_type_identifier(&self.identifier) && !self.localized_kind.is_empty() && self.localized_kind.len() <= 4096 && self.preferred_extension.as_ref().is_none_or(|extension| !extension.is_empty() && extension.len() <= 255 && !extension.chars().any(char::is_control))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssociationRecord {
    pub content_type: ContentTypeDescription,
    pub candidates: Vec<ApplicationCandidate>,
    pub default_application: Option<ApplicationIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFileAssociation {
    pub source: Location,
    pub target: Location,
    pub source_identity: FileIdentity,
    pub target_identity: FileIdentity,
    pub record: FileAssociationRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAssociationLookup {
    Resolved { files: Vec<ResolvedFileAssociation>, records: Vec<FileAssociationRecord> },
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssociationPreference {
    pub content_type: String,
    pub pinned: bool,
    pub normal: Vec<ApplicationIdentity>,
    pub comprehensive: Vec<ApplicationIdentity>,
}

impl FileAssociationPreference {
    pub fn selected(&self, kind: ContextMenuKind, application: &ApplicationIdentity) -> bool {
        match kind {
            ContextMenuKind::Normal => &self.normal,
            ContextMenuKind::Comprehensive => &self.comprehensive,
        }
        .contains(application)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileAssociationPreferences {
    entries: Vec<FileAssociationPreference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAssociationPreferenceRejection {
    Malformed,
    UnknownType,
}

impl FileAssociationPreferences {
    pub fn from_entries(entries: Vec<FileAssociationPreference>) -> Result<Self, FileAssociationPreferenceRejection> {
        if entries.len() > FILE_ASSOCIATION_LIMIT {
            return Err(FileAssociationPreferenceRejection::Malformed);
        }
        let mut types = HashSet::new();
        for entry in &entries {
            // Entries exist only for types a person explicitly retained:
            // adding a sample, changing a choice, or clearing choices all
            // pins the type. An unpinned durable row would resurrect an
            // otherwise transient observed type after its folder closes.
            if !entry.pinned || !valid_type_identifier(&entry.content_type) || !types.insert(entry.content_type.as_str()) || entry.normal.len() > FILE_ASSOCIATION_APP_LIMIT || entry.comprehensive.len() > FILE_ASSOCIATION_APP_LIMIT || !valid_identities(&entry.normal) || !valid_identities(&entry.comprehensive) {
                return Err(FileAssociationPreferenceRejection::Malformed);
            }
        }
        let mut entries = entries;
        entries.sort_by(|left, right| left.content_type.cmp(&right.content_type));
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[FileAssociationPreference] {
        &self.entries
    }

    pub fn get(&self, content_type: &str) -> Option<&FileAssociationPreference> {
        self.entries.iter().find(|entry| entry.content_type == content_type)
    }

    pub fn pin(&mut self, content_type: &str) -> Result<bool, FileAssociationPreferenceRejection> {
        if !valid_type_identifier(content_type) {
            return Err(FileAssociationPreferenceRejection::Malformed);
        }
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.content_type == content_type) {
            let changed = !entry.pinned;
            entry.pinned = true;
            return Ok(changed);
        }
        if self.entries.len() >= FILE_ASSOCIATION_LIMIT {
            return Err(FileAssociationPreferenceRejection::Malformed);
        }
        self.entries.push(FileAssociationPreference { content_type: content_type.to_owned(), pinned: true, normal: Vec::new(), comprehensive: Vec::new() });
        self.entries.sort_by(|left, right| left.content_type.cmp(&right.content_type));
        Ok(true)
    }

    pub fn set_selected(&mut self, content_type: &str, kind: ContextMenuKind, application: ApplicationIdentity, selected: bool) -> Result<bool, FileAssociationPreferenceRejection> {
        if !application.valid() {
            return Err(FileAssociationPreferenceRejection::Malformed);
        }
        self.pin(content_type)?;
        let entry = self.entries.iter_mut().find(|entry| entry.content_type == content_type).ok_or(FileAssociationPreferenceRejection::UnknownType)?;
        let choices = match kind {
            ContextMenuKind::Normal => &mut entry.normal,
            ContextMenuKind::Comprehensive => &mut entry.comprehensive,
        };
        if selected {
            if choices.contains(&application) {
                return Ok(false);
            }
            if choices.len() >= FILE_ASSOCIATION_APP_LIMIT {
                return Err(FileAssociationPreferenceRejection::Malformed);
            }
            choices.push(application);
        } else if let Some(index) = choices.iter().position(|candidate| candidate == &application) {
            choices.remove(index);
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    pub fn clear(&mut self, content_type: &str) -> Result<bool, FileAssociationPreferenceRejection> {
        let entry = self.entries.iter_mut().find(|entry| entry.content_type == content_type).ok_or(FileAssociationPreferenceRejection::UnknownType)?;
        let changed = !entry.normal.is_empty() || !entry.comprehensive.is_empty() || !entry.pinned;
        entry.normal.clear();
        entry.comprehensive.clear();
        entry.pinned = true;
        Ok(changed)
    }

    /// Updates a saved application URL only when macOS reports one current
    /// candidate for its bundle identifier. Multiple copies remain explicit
    /// unavailable choices so a refresh cannot silently choose one.
    pub fn remap_unambiguous_candidates(&mut self, content_type: &str, candidates: &[ApplicationIdentity]) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.content_type == content_type) else { return false };
        let mut changed = false;
        for choices in [&mut entry.normal, &mut entry.comprehensive] {
            let mut remapped = Vec::with_capacity(choices.len());
            for saved in choices.iter() {
                let replacement = candidates.iter().filter(|candidate| candidate.bundle_identifier == saved.bundle_identifier).take(2).collect::<Vec<_>>();
                let identity = match replacement.as_slice() {
                    [candidate] if *candidate != saved => {
                        changed = true;
                        (*candidate).clone()
                    }
                    _ => saved.clone(),
                };
                if !remapped.contains(&identity) {
                    remapped.push(identity);
                } else {
                    changed = true;
                }
            }
            *choices = remapped;
        }
        changed
    }

    pub fn remove(&mut self, content_type: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.content_type != content_type);
        before != self.entries.len()
    }
}

fn valid_identities(identities: &[ApplicationIdentity]) -> bool {
    let mut seen = HashSet::new();
    identities.iter().all(|identity| identity.valid() && seen.insert(identity))
}

pub fn valid_type_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.len() <= 1024 && identifier.contains('.') && identifier.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenWithToken(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWithMenuAction {
    pub token: OpenWithToken,
    pub application: ApplicationIdentity,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileAssociationRequestId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAssociationRequestPurpose {
    Observe,
    FocusSettings,
    PinSample,
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssociationRequest {
    pub id: FileAssociationRequestId,
    pub files: Vec<Location>,
    pub content_types: Vec<String>,
    pub purpose: FileAssociationRequestPurpose,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWithRequest {
    pub files: Vec<OpenWithFile>,
    pub content_type: String,
    pub application: ApplicationIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWithFile {
    pub source: Location,
    pub target: Location,
    pub source_identity: FileIdentity,
    pub target_identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultApplicationRequest {
    pub content_type: String,
    pub application: ApplicationIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssociationCandidateView {
    pub identity: ApplicationIdentity,
    pub display_name: String,
    pub location_hint: String,
    pub available: bool,
    pub normal: bool,
    pub comprehensive: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAssociationTypeView {
    pub content_type: ContentTypeDescription,
    pub pinned: bool,
    pub candidates: Vec<FileAssociationCandidateView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> ApplicationIdentity {
        ApplicationIdentity { bundle_identifier: format!("com.example.{name}"), bundle_url: format!("file:///Applications/{name}.app") }
    }

    #[test]
    fn choices_start_empty_and_remain_independent() {
        let mut preferences = FileAssociationPreferences::default();
        preferences.pin("public.plain-text").unwrap();
        assert!(preferences.get("public.plain-text").unwrap().normal.is_empty());
        preferences.set_selected("public.plain-text", ContextMenuKind::Normal, app("Editor"), true).unwrap();
        let entry = preferences.get("public.plain-text").unwrap();
        assert_eq!(entry.normal, vec![app("Editor")]);
        assert!(entry.comprehensive.is_empty());
        preferences.clear("public.plain-text").unwrap();
        assert!(preferences.get("public.plain-text").unwrap().pinned);
        assert!(preferences.get("public.plain-text").unwrap().normal.is_empty());
    }

    #[test]
    fn malformed_and_duplicate_durable_values_are_rejected() {
        let entry = FileAssociationPreference { content_type: "not a type".into(), pinned: true, normal: vec![], comprehensive: vec![] };
        assert_eq!(FileAssociationPreferences::from_entries(vec![entry]), Err(FileAssociationPreferenceRejection::Malformed));
        let duplicate = FileAssociationPreference { content_type: "public.text".into(), pinned: true, normal: vec![app("Editor"), app("Editor")], comprehensive: vec![] };
        assert_eq!(FileAssociationPreferences::from_entries(vec![duplicate]), Err(FileAssociationPreferenceRejection::Malformed));
        let transient = FileAssociationPreference { content_type: "public.text".into(), pinned: false, normal: vec![], comprehensive: vec![] };
        assert_eq!(FileAssociationPreferences::from_entries(vec![transient]), Err(FileAssociationPreferenceRejection::Malformed));
    }

    #[test]
    fn a_moved_application_remaps_only_when_its_bundle_identifier_is_unambiguous() {
        let mut preferences = FileAssociationPreferences::default();
        let old = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Old/Editor.app".into() };
        let moved = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Applications/Editor.app".into() };
        preferences.set_selected("public.plain-text", ContextMenuKind::Normal, old.clone(), true).unwrap();
        assert!(preferences.remap_unambiguous_candidates("public.plain-text", std::slice::from_ref(&moved)));
        assert!(preferences.get("public.plain-text").unwrap().selected(ContextMenuKind::Normal, &moved));
        assert!(!preferences.get("public.plain-text").unwrap().selected(ContextMenuKind::Normal, &old));

        preferences.set_selected("public.plain-text", ContextMenuKind::Normal, old.clone(), true).unwrap();
        let second_copy = ApplicationIdentity { bundle_identifier: "com.example.Editor".into(), bundle_url: "file:///Elsewhere/Editor.app".into() };
        assert!(!preferences.remap_unambiguous_candidates("public.plain-text", &[moved, second_copy]));
        assert!(preferences.get("public.plain-text").unwrap().selected(ContextMenuKind::Normal, &old));
    }
}
