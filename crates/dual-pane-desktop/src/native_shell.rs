//! Qt's macOS shell services, called with exact path bytes: moving an item
//! to the Trash, recognizing a package, and opening an item with its default
//! application.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use dual_pane_application::{ApplicationCandidate, ApplicationIdentity, ContentTypeDescription, DefaultApplicationRequest, FileAssociationLookup, FileAssociationRecord, FileAssociationRequest, FileIdentity, OpenWithRequest, ResolvedFileAssociation};

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
mod ffi {
    unsafe extern "C++" {
        include!("dual_pane_desktop/native_shell.hpp");
        fn move_to_trash(path: &[u8], trashed: &mut Vec<u8>) -> bool;
        fn is_bundle(path: &[u8]) -> bool;
        fn open_with_default_application(path: &[u8]) -> bool;
        fn reveal_in_finder(path: &[u8]) -> bool;
        fn file_associations(request: &[u8], response: &mut Vec<u8>);
        fn open_with_application(request: &[u8], response: &mut Vec<u8>);
        fn set_default_application(request: &[u8], response: &mut Vec<u8>);
    }
}

/// Moves `path` to its volume's Trash with `QFile::moveToTrash` and returns
/// where it went. Qt reports only success or failure.
pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    let mut trashed = Vec::new();
    if ffi::move_to_trash(path.as_os_str().as_bytes(), &mut trashed) { Ok(PathBuf::from(OsString::from_vec(trashed))) } else { Err(io::Error::other("the item could not be moved to the Trash")) }
}

/// Whether the folder at `path` is a macOS package, such as an application.
/// `QFileInfo` is reentrant, so a listing worker may ask.
pub fn is_bundle(path: &Path) -> bool {
    ffi::is_bundle(path.as_os_str().as_bytes())
}

/// Asks macOS to open `path` with its default application. Call it on the
/// GUI thread; it reports only whether macOS accepted the request.
pub fn open_with_default_application(path: &Path) -> bool {
    ffi::open_with_default_application(path.as_os_str().as_bytes())
}

/// Asks the Finder to reveal `path`, selecting it in its enclosing folder.
/// Call it on the GUI thread; it reports only whether macOS accepted the
/// request.
pub fn reveal_in_finder(path: &Path) -> bool {
    ffi::reveal_in_finder(path.as_os_str().as_bytes())
}

pub fn resolve_file_associations(request: &FileAssociationRequest) -> FileAssociationLookup {
    let mut encoded = Vec::new();
    push_u32(&mut encoded, request.files.len());
    for file in &request.files {
        push_bytes(&mut encoded, crate::native_location::path_from_location(file).as_os_str().as_bytes());
    }
    push_u32(&mut encoded, request.content_types.len());
    for content_type in &request.content_types {
        push_bytes(&mut encoded, content_type.as_bytes());
    }
    encoded.push(match request.purpose {
        dual_pane_application::FileAssociationRequestPurpose::Observe => 0,
        dual_pane_application::FileAssociationRequestPurpose::FocusSettings => 1,
        dual_pane_application::FileAssociationRequestPurpose::PinSample => 2,
        dual_pane_application::FileAssociationRequestPurpose::Refresh => 3,
    });
    let mut response = Vec::new();
    ffi::file_associations(&encoded, &mut response);
    decode_associations(&response).unwrap_or_else(FileAssociationLookup::Rejected)
}

pub fn open_with_application(request: &OpenWithRequest) -> Result<(), String> {
    let mut encoded = Vec::new();
    push_bytes(&mut encoded, request.content_type.as_bytes());
    push_identity(&mut encoded, &request.application);
    push_u32(&mut encoded, request.files.len());
    for file in &request.files {
        push_bytes(&mut encoded, crate::native_location::path_from_location(&file.source).as_os_str().as_bytes());
        push_bytes(&mut encoded, crate::native_location::path_from_location(&file.target).as_os_str().as_bytes());
        push_u64(&mut encoded, file.source_identity.device);
        push_u64(&mut encoded, file.source_identity.inode);
        push_u64(&mut encoded, file.target_identity.device);
        push_u64(&mut encoded, file.target_identity.inode);
    }
    native_effect(&encoded, ffi::open_with_application)
}

pub fn change_default_application(request: &DefaultApplicationRequest) -> Result<(), String> {
    let mut encoded = Vec::new();
    push_bytes(&mut encoded, request.content_type.as_bytes());
    push_identity(&mut encoded, &request.application);
    native_effect(&encoded, ffi::set_default_application)
}

fn native_effect(encoded: &[u8], call: fn(&[u8], &mut Vec<u8>)) -> Result<(), String> {
    let mut response = Vec::new();
    call(encoded, &mut response);
    if response.is_empty() { Ok(()) } else { Err(String::from_utf8(response).unwrap_or_else(|_| "macOS returned an unreadable error".to_owned())) }
}

fn push_u32(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&u32::try_from(value).unwrap_or(u32::MAX).to_be_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_bytes(target: &mut Vec<u8>, value: &[u8]) {
    push_u32(target, value.len());
    target.extend_from_slice(value);
}

fn push_identity(target: &mut Vec<u8>, identity: &ApplicationIdentity) {
    push_bytes(target, identity.bundle_identifier.as_bytes());
    push_bytes(target, identity.bundle_url.as_bytes());
}

struct Decoder<'a> {
    rest: &'a [u8],
}

impl<'a> Decoder<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let (&value, rest) = self.rest.split_first().ok_or_else(|| "macOS returned incomplete association data".to_owned())?;
        self.rest = rest;
        Ok(value)
    }
    fn bytes(&mut self) -> Result<&'a [u8], String> {
        let length_bytes = self.rest.get(..4).ok_or_else(|| "macOS returned incomplete association data".to_owned())?;
        let length = u32::from_be_bytes(length_bytes.try_into().map_err(|_| "macOS returned invalid association data".to_owned())?) as usize;
        self.rest = &self.rest[4..];
        let value = self.rest.get(..length).ok_or_else(|| "macOS returned incomplete association data".to_owned())?;
        self.rest = &self.rest[length..];
        Ok(value)
    }
    fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| "macOS returned invalid text for a file association".to_owned())
    }
    fn count(&mut self) -> Result<usize, String> {
        let bytes = self.rest.get(..4).ok_or_else(|| "macOS returned incomplete association data".to_owned())?;
        self.rest = &self.rest[4..];
        Ok(u32::from_be_bytes(bytes.try_into().map_err(|_| "macOS returned invalid association data".to_owned())?) as usize)
    }
    fn u64(&mut self) -> Result<u64, String> {
        let bytes = self.rest.get(..8).ok_or_else(|| "macOS returned incomplete association data".to_owned())?;
        self.rest = &self.rest[8..];
        Ok(u64::from_be_bytes(bytes.try_into().map_err(|_| "macOS returned invalid association data".to_owned())?))
    }
    fn identity(&mut self) -> Result<ApplicationIdentity, String> {
        Ok(ApplicationIdentity { bundle_identifier: self.string()?, bundle_url: self.string()? })
    }
}

fn decode_associations(bytes: &[u8]) -> Result<FileAssociationLookup, String> {
    let mut decoder = Decoder { rest: bytes };
    if decoder.u8()? == 0 {
        return Ok(FileAssociationLookup::Rejected(decoder.string()?));
    }
    let mut records = Vec::new();
    for _ in 0..decoder.count()? {
        records.push(decode_record(&mut decoder)?);
    }
    let mut files = Vec::new();
    for _ in 0..decoder.count()? {
        let source = PathBuf::from(OsString::from_vec(decoder.bytes()?.to_vec()));
        let target = PathBuf::from(OsString::from_vec(decoder.bytes()?.to_vec()));
        let source_identity = FileIdentity { device: decoder.u64()?, inode: decoder.u64()? };
        let target_identity = FileIdentity { device: decoder.u64()?, inode: decoder.u64()? };
        let record = decode_record(&mut decoder)?;
        let source = crate::native_location::location_from_path(&source).ok_or_else(|| "macOS returned an invalid source path".to_owned())?;
        let target = crate::native_location::location_from_path(&target).ok_or_else(|| "macOS returned an invalid target path".to_owned())?;
        files.push(ResolvedFileAssociation { source, target, source_identity, target_identity, record });
    }
    if !decoder.rest.is_empty() {
        return Err("macOS returned trailing association data".to_owned());
    }
    Ok(FileAssociationLookup::Resolved { files, records })
}

fn decode_record(decoder: &mut Decoder<'_>) -> Result<FileAssociationRecord, String> {
    let identifier = decoder.string()?;
    let localized_kind = decoder.string()?;
    let extension = decoder.string()?;
    let mut candidates = Vec::new();
    for _ in 0..decoder.count()? {
        candidates.push(ApplicationCandidate { identity: decoder.identity()?, display_name: decoder.string()?, location_hint: decoder.string()? });
    }
    let default_application = if decoder.u8()? == 1 { Some(decoder.identity()?) } else { None };
    let record = FileAssociationRecord { content_type: ContentTypeDescription { identifier, localized_kind, preferred_extension: (!extension.is_empty()).then_some(extension) }, candidates, default_application };
    if !record.content_type.valid() || record.candidates.iter().any(|candidate| !candidate.identity.valid()) {
        return Err("macOS returned an invalid file association".to_owned());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_named_like_an_application_with_a_bundle_layout_is_a_package() {
        let directory = tempfile::tempdir().unwrap();
        let plain = directory.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        assert!(!is_bundle(&plain));
        let application = directory.path().join("Tool.app");
        std::fs::create_dir_all(application.join("Contents/MacOS")).unwrap();
        std::fs::write(application.join("Contents/Info.plist"), br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#).unwrap();
        let worker = std::thread::spawn(move || is_bundle(&application));
        assert!(worker.join().unwrap(), "a worker thread recognizes the package");
    }

    #[test]
    fn file_association_discovery_uses_a_temporary_regular_file() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("sample.txt");
        std::fs::write(&file, b"temporary file association fixture\n").unwrap();
        let location = crate::native_location::location_from_path(&file).unwrap();
        let request = FileAssociationRequest { id: dual_pane_application::FileAssociationRequestId(1), files: vec![location.clone()], content_types: Vec::new(), purpose: dual_pane_application::FileAssociationRequestPurpose::PinSample };
        let result = std::thread::spawn(move || resolve_file_associations(&request)).join().unwrap();
        let (files, records) = match result {
            FileAssociationLookup::Resolved { files, records } => (files, records),
            FileAssociationLookup::Rejected(reason) => panic!("temporary text file association was rejected: {reason}"),
        };
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].source, location);
        assert_eq!(files[0].source_identity, files[0].target_identity);
        assert!(files[0].record.content_type.valid());
        assert!(records.iter().any(|record| record.content_type.identifier == files[0].record.content_type.identifier));
    }

    /// Settles whether `QFile::moveToTrash` works off the GUI thread. It moves
    /// a temporary file into the real Trash of its volume, outside any
    /// temporary folder, so it runs only on request.
    #[test]
    #[ignore = "moves a file into the user's real Trash"]
    fn moving_to_the_trash_works_on_a_worker_thread() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("dual-pane-trash-check.txt");
        std::fs::write(&file, b"check").unwrap();
        let moved = std::thread::spawn(move || move_to_trash(&file).map(|trashed| (file, trashed))).join().unwrap();
        let (file, trashed) = moved.unwrap();
        assert!(!file.exists());
        assert!(trashed.exists(), "the item is in the Trash at {trashed:?}");
    }
}
