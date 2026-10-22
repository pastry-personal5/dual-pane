//! The macOS file calls the standard library does not offer: exclusive
//! rename, cloning, copying with progress, and random bytes. Every `unsafe`
//! block outside the CXX-Qt bridges lives here; callers own the policy.

use std::ffi::{CString, c_char, c_int, c_void};
use std::fs::{self, File};
use std::io;
use std::os::darwin::fs::MetadataExt;
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt as UnixMetadataExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// `CLONE_NOFOLLOW` from `<sys/clonefile.h>`, which libc does not define.
const CLONE_NOFOLLOW: u32 = 0x0001;
/// `UF_HIDDEN` from `<sys/stat.h>`.
const UF_HIDDEN: u32 = 0x0000_8000;
/// Finder's `kIsInvisible` flag in the big-endian `FileInfo.fdFlags` field.
const FINDER_INVISIBLE: u16 = 0x4000;
const FINDER_INFO: &[u8] = b"com.apple.FinderInfo\0";

/// Whether the entry itself has either supported macOS invisibility marker:
/// `UF_HIDDEN` or Finder's `kIsInvisible`. Each read avoids following a
/// symbolic link, so a target cannot hide its link.
pub fn is_macos_invisible(path: &Path) -> io::Result<bool> {
    if fs::symlink_metadata(path)?.st_flags() & UF_HIDDEN != 0 {
        return Ok(true);
    }
    let path = c_path(path)?;
    let mut info = [0_u8; 32];
    // SAFETY: Both names are NUL-terminated, `info` is a valid writable
    // buffer, and `XATTR_NOFOLLOW` asks macOS to inspect the link itself.
    let length = unsafe { libc::getxattr(path.as_ptr(), FINDER_INFO.as_ptr().cast(), info.as_mut_ptr().cast(), info.len(), 0, libc::XATTR_NOFOLLOW) };
    if length < 0 {
        return finder_info_unreadable_result(io::Error::last_os_error());
    }
    let length = usize::try_from(length).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    finder_info_invisible(&info[..length])
}

/// Maps a failed `getxattr` into "no readable Finder flag" rather than an
/// error when the failure means there is none to read: no such attribute
/// (`ENOATTR`), no extended-attribute support (`ENOTSUP`/`EOPNOTSUPP`, as on
/// FAT32 and many network shares), or macOS refusing this process the read
/// (`EPERM`/`EACCES`, as for `~/.Trash`, volume-root service folders, and
/// other privacy-protected items that `lstat` still reports). Any other
/// failure stays an error, so a real I/O fault is not mistaken for a visible
/// item.
fn finder_info_unreadable_result(error: io::Error) -> io::Result<bool> {
    match error.raw_os_error() {
        Some(libc::ENOATTR | libc::ENOTSUP | libc::EOPNOTSUPP | libc::EPERM | libc::EACCES) => Ok(false),
        _ => Err(error),
    }
}

fn finder_info_invisible(info: &[u8]) -> io::Result<bool> {
    if info.len() < 10 {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok(u16::from_be_bytes([info[8], info[9]]) & FINDER_INVISIBLE != 0)
}

/// Whether `path` is a filesystem root. Resolve aliases before comparing the
/// directory with its parent, so an alias to a subfolder on another volume is
/// not mistaken for that volume's root.
pub fn is_volume_root(path: &Path) -> io::Result<bool> {
    let resolved = fs::canonicalize(path)?;
    let metadata = fs::metadata(&resolved)?;
    let parent = resolved.parent().unwrap_or(&resolved);
    let parent_metadata = fs::metadata(parent)?;
    Ok(metadata.dev() != parent_metadata.dev() || (metadata.dev() == parent_metadata.dev() && metadata.ino() == parent_metadata.ino()))
}

/// The name macOS stores for the final component of `path`, without following
/// a final link. Intermediate links are followed by the kernel. The lookup
/// ignores letter case and Unicode normalization on volumes that do, so the
/// returned spelling can differ from the typed one.
pub fn stored_name(path: &Path) -> io::Result<Vec<u8>> {
    let path = c_path(path)?;
    let mut request = libc::attrlist { bitmapcount: libc::ATTR_BIT_MAP_COUNT, reserved: 0, commonattr: libc::ATTR_CMN_NAME, volattr: 0, dirattr: 0, fileattr: 0, forkattr: 0 };
    // The buffer holds a length word, the name reference, and the name bytes
    // (at most `NAME_MAX` bytes of UTF-8 plus a terminator on macOS).
    let mut buffer = [0_u8; 4 + 8 + 1024];
    // SAFETY: `path` is NUL-terminated, `request` is a valid attribute list
    // that outlives the call, and `buffer` is writable for its full length.
    let status = unsafe { libc::getattrlist(path.as_ptr(), (&raw mut request).cast::<c_void>(), buffer.as_mut_ptr().cast::<c_void>(), buffer.len(), libc::FSOPT_NOFOLLOW) };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    let malformed = || io::Error::from(io::ErrorKind::InvalidData);
    let total = u32::from_ne_bytes(buffer[0..4].try_into().map_err(|_| malformed())?) as usize;
    let offset = i32::from_ne_bytes(buffer[4..8].try_into().map_err(|_| malformed())?);
    let length = u32::from_ne_bytes(buffer[8..12].try_into().map_err(|_| malformed())?) as usize;
    // The offset counts from the start of the name reference, at byte 4.
    let start = usize::try_from(i64::from(offset) + 4).map_err(|_| malformed())?;
    let end = start.checked_add(length).ok_or_else(malformed)?;
    if total > buffer.len() || end > total || length == 0 {
        return Err(malformed());
    }
    let mut name = &buffer[start..end];
    if let [rest @ .., 0] = name {
        name = rest;
    }
    Ok(name.to_vec())
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

/// Opens only the named regular file, never a symbolic-link target. The
/// returned descriptor stays owned by `File` while a preview reader uses it.
pub fn open_preview_file(path: &Path) -> io::Result<File> {
    let path = c_path(path)?;
    // SAFETY: `path` is a live NUL-terminated string. A successful `open`
    // returns one owned descriptor, transferred exactly once to `File`.
    let descriptor = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `descriptor` was just returned by `open`, and no other owner
    // exists; `File` closes it exactly once.
    let file = unsafe { File::from_raw_fd(descriptor) };
    if !file.metadata()?.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(file)
}

fn result(status: c_int) -> io::Result<()> {
    if status == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Renames `from` to `to` only if `to` does not exist; otherwise fails with
/// `EEXIST` and changes nothing. A volume without exclusive rename fails
/// with `ENOTSUP`, and callers never fall back to a plain rename.
pub fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (c_path(from)?, c_path(to)?);
    // SAFETY: Both arguments are valid NUL-terminated strings that live until
    // the call returns, and `RENAME_EXCL` is a valid flag for `renamex_np`.
    result(unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) })
}

/// Clones `from` to the new path `to` on the same volume, with its
/// attributes, extended attributes, and ACLs. A link is never followed.
pub fn clone_file(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (c_path(from)?, c_path(to)?);
    // SAFETY: Both arguments are valid NUL-terminated strings that live until
    // the call returns, and `CLONE_NOFOLLOW` is a documented flag.
    result(unsafe { libc::clonefile(from.as_ptr(), to.as_ptr(), CLONE_NOFOLLOW) })
}

/// The progress callback and whether it asked to stop.
struct CopyContext<'a> {
    progress: &'a mut dyn FnMut(u64) -> bool,
    stopped: bool,
}

/// Reports data progress to the `CopyContext` registered as the state's
/// context and stops the copy when it returns `false`. It never unwinds
/// into `copyfile`.
extern "C" fn copy_status(what: c_int, stage: c_int, state: libc::copyfile_state_t, _source: *const c_char, _destination: *const c_char, context: *mut c_void) -> c_int {
    if what != libc::COPYFILE_COPY_DATA {
        return libc::COPYFILE_CONTINUE;
    }
    if stage != libc::COPYFILE_PROGRESS {
        return if stage == libc::COPYFILE_ERR { libc::COPYFILE_QUIT } else { libc::COPYFILE_CONTINUE };
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mut copied: libc::off_t = 0;
        // SAFETY: `state` is the live state `copyfile` passed to this
        // callback, and `copied` is a valid `off_t` for
        // `COPYFILE_STATE_COPIED` to write.
        unsafe { libc::copyfile_state_get(state, libc::COPYFILE_STATE_COPIED as u32, (&raw mut copied).cast()) };
        // SAFETY: `context` is the `CopyContext` that `copy_file` registered
        // and keeps alive, unaliased, until `copyfile` returns.
        let context = unsafe { &mut *context.cast::<CopyContext<'_>>() };
        if (context.progress)(u64::try_from(copied).unwrap_or(0)) {
            libc::COPYFILE_CONTINUE
        } else {
            context.stopped = true;
            libc::COPYFILE_QUIT
        }
    }))
    .unwrap_or(libc::COPYFILE_QUIT)
}

/// Frees a `copyfile` state when dropped.
struct CopyState(libc::copyfile_state_t);

impl Drop for CopyState {
    fn drop(&mut self) {
        // SAFETY: The state came from `copyfile_state_alloc` and is freed
        // exactly once, after `copyfile` has returned.
        unsafe { libc::copyfile_state_free(self.0) };
    }
}

/// Copies `from` to the new path `to` with its data, permissions,
/// timestamps, extended attributes, resource fork, and ACLs, never following
/// a link and never replacing an existing `to`. `progress` receives the data
/// bytes copied so far and returns `false` to stop. Returns `Ok(false)` when
/// it stopped the copy; the caller removes whatever `to` holds then.
pub fn copy_file(from: &Path, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<bool> {
    let (from, to) = (c_path(from)?, c_path(to)?);
    // SAFETY: `copyfile_state_alloc` takes no arguments; a null result is
    // checked before use.
    let state = CopyState(unsafe { libc::copyfile_state_alloc() });
    if state.0.is_null() {
        return Err(io::Error::from(io::ErrorKind::OutOfMemory));
    }
    let mut context = CopyContext { progress, stopped: false };
    let callback: libc::copyfile_callback_t = Some(copy_status);
    // SAFETY: The state is live. `COPYFILE_STATE_STATUS_CB` takes the
    // function pointer itself and `COPYFILE_STATE_STATUS_CTX` the context
    // pointer itself; `context` outlives the `copyfile` call below.
    unsafe {
        libc::copyfile_state_set(state.0, libc::COPYFILE_STATE_STATUS_CB as u32, callback.map_or(std::ptr::null(), |callback| callback as *const c_void));
        libc::copyfile_state_set(state.0, libc::COPYFILE_STATE_STATUS_CTX as u32, (&raw mut context).cast());
    }
    let flags = libc::COPYFILE_METADATA | libc::COPYFILE_DATA | libc::COPYFILE_NOFOLLOW | libc::COPYFILE_EXCL;
    // SAFETY: Both paths are valid NUL-terminated strings, and the state is
    // live with a callback and context that outlive the call.
    let status = unsafe { libc::copyfile(from.as_ptr(), to.as_ptr(), state.0, flags) };
    if context.stopped {
        return Ok(false);
    }
    result(status).map(|()| true)
}

/// Copies the permissions, timestamps, extended attributes, and ACLs of
/// `from` onto the existing `to`, without following links. It is used for a
/// folder after its contents are copied.
pub fn copy_metadata(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (c_path(from)?, c_path(to)?);
    // SAFETY: Both paths are valid NUL-terminated strings; a null state is
    // allowed and the flags only copy metadata.
    result(unsafe { libc::copyfile(from.as_ptr(), to.as_ptr(), std::ptr::null_mut(), libc::COPYFILE_METADATA | libc::COPYFILE_NOFOLLOW) })
}

/// 16 bytes from the system's secure random source.
pub fn random_bytes() -> io::Result<[u8; 16]> {
    let mut bytes = [0_u8; 16];
    // SAFETY: The buffer is valid for writes of its full length, which is
    // within `getentropy`'s 256-byte limit.
    result(unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) })?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use super::*;

    #[test]
    fn exclusive_rename_refuses_an_existing_destination_and_changes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let (from, to) = (directory.path().join("from"), directory.path().join("to"));
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"old").unwrap();
        assert_eq!(rename_exclusive(&from, &to).unwrap_err().raw_os_error(), Some(libc::EEXIST));
        assert_eq!((fs::read(&from).unwrap(), fs::read(&to).unwrap()), (b"new".to_vec(), b"old".to_vec()));
        fs::remove_file(&to).unwrap();
        rename_exclusive(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read(&to).unwrap(), b"new");
    }

    #[test]
    fn invisibility_reads_finder_info_without_following_links() {
        let directory = tempfile::tempdir().unwrap();
        let hidden = directory.path().join("hidden");
        fs::write(&hidden, b"content").unwrap();
        let path = c_path(&hidden).unwrap();
        let mut finder_info = [0_u8; 32];
        finder_info[8] = 0x40;
        // SAFETY: Both names are NUL-terminated and `finder_info` stays live
        // as the 32-byte Finder information payload for this call.
        assert_eq!(unsafe { libc::setxattr(path.as_ptr(), FINDER_INFO.as_ptr().cast(), finder_info.as_ptr().cast(), finder_info.len(), 0, 0) }, 0);
        assert!(is_macos_invisible(&hidden).unwrap());
        let link = directory.path().join("link");
        symlink(&hidden, &link).unwrap();
        assert!(!is_macos_invisible(&link).unwrap());
    }

    #[test]
    fn missing_unsupported_or_protected_finder_information_is_not_an_error() {
        for code in [libc::ENOATTR, libc::ENOTSUP, libc::EOPNOTSUPP, libc::EPERM, libc::EACCES] {
            assert!(!finder_info_unreadable_result(io::Error::from_raw_os_error(code)).unwrap());
        }
        for code in [libc::EIO, libc::ENOMEM, libc::EINVAL] {
            assert!(finder_info_unreadable_result(io::Error::from_raw_os_error(code)).is_err());
        }
    }

    #[test]
    fn finder_information_uses_big_endian_invisible_bit() {
        let mut info = [0_u8; 32];
        assert!(!finder_info_invisible(&info).unwrap());
        info[8] = 0x40;
        assert!(finder_info_invisible(&info).unwrap());
        info[8] = 0;
        info[9] = 0x40;
        assert!(!finder_info_invisible(&info).unwrap());
        assert!(finder_info_invisible(&info[..9]).is_err());
    }

    #[test]
    fn alias_to_an_ordinary_folder_is_not_a_volume_root() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let alias = directory.path().join("alias");
        symlink(&nested, &alias).unwrap();
        assert!(!is_volume_root(&nested).unwrap());
        assert!(!is_volume_root(&alias).unwrap());
    }

    #[test]
    fn clone_and_copy_keep_data_and_permissions_and_copy_links_as_links() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::write(&source, b"content").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        let cloned = directory.path().join("cloned");
        match clone_file(&source, &cloned) {
            Ok(()) => assert_eq!(fs::read(&cloned).unwrap(), b"content"),
            // The temporary directory's volume may not clone.
            Err(error) => assert_eq!(error.raw_os_error(), Some(libc::ENOTSUP)),
        }
        let copied = directory.path().join("copied");
        assert!(copy_file(&source, &copied, &mut |_| true).unwrap());
        assert_eq!(fs::read(&copied).unwrap(), b"content");
        assert_eq!(fs::metadata(&copied).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(copy_file(&source, &copied, &mut |_| true).unwrap_err().raw_os_error(), Some(libc::EEXIST), "an existing destination is never replaced");

        let link = directory.path().join("link");
        symlink("source", &link).unwrap();
        let link_copy = directory.path().join("link-copy");
        assert!(copy_file(&link, &link_copy, &mut |_| true).unwrap());
        assert!(fs::symlink_metadata(&link_copy).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&link_copy).unwrap(), Path::new("source"));
    }

    #[test]
    fn the_progress_callback_reports_bytes_and_can_stop_a_copy() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("large");
        fs::write(&source, vec![7_u8; 8 * 1024 * 1024]).unwrap();
        let mut reported = Vec::new();
        assert!(
            copy_file(&source, &directory.path().join("whole"), &mut |bytes| {
                reported.push(bytes);
                true
            })
            .unwrap()
        );
        assert!(!reported.is_empty(), "data progress is reported");
        assert!(reported.windows(2).all(|pair| pair[0] <= pair[1]));

        let stopped = directory.path().join("stopped");
        let mut calls = 0;
        assert!(
            !copy_file(&source, &stopped, &mut |_| {
                calls += 1;
                false
            })
            .unwrap()
        );
        assert_eq!(calls, 1, "the copy stops at the first report");
        fs::remove_file(&stopped).ok();
        assert!(!stopped.exists());
    }

    #[test]
    fn folder_metadata_copies_onto_an_existing_folder() {
        let directory = tempfile::tempdir().unwrap();
        let (from, to) = (directory.path().join("from"), directory.path().join("to"));
        fs::create_dir(&from).unwrap();
        fs::create_dir(&to).unwrap();
        fs::set_permissions(&from, fs::Permissions::from_mode(0o750)).unwrap();
        copy_metadata(&from, &to).unwrap();
        assert_eq!(fs::metadata(&to).unwrap().permissions().mode() & 0o777, 0o750);
        assert_ne!(fs::metadata(&to).unwrap().ino(), fs::metadata(&from).unwrap().ino());
    }

    #[test]
    fn random_bytes_differ_between_calls() {
        assert_ne!(random_bytes().unwrap(), random_bytes().unwrap());
    }
}
