//! The macOS file calls the standard library does not offer: exclusive
//! rename, cloning, copying with progress, and random bytes. Every `unsafe`
//! block outside the CXX-Qt bridges lives here; callers own the policy.

use std::ffi::{CString, c_char, c_int, c_void};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// `CLONE_NOFOLLOW` from `<sys/clonefile.h>`, which libc does not define.
const CLONE_NOFOLLOW: u32 = 0x0001;

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
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
