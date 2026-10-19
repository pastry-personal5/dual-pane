//! Per-profile single-instance rendezvous and its bounded binary protocol.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use dual_pane_domain::EntryName;

use crate::launch_target::LaunchTarget;
use crate::native_location::{location_from_path, path_from_location};

const MAGIC: &[u8; 4] = b"DPL1";
const KIND_RAISE: u8 = 0;
const KIND_OPEN: u8 = 1;
const MAX_FRAME: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_DELAY: Duration = Duration::from_millis(25);
const ACCEPT_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Raise,
    Open(LaunchTarget),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Accepted,
    Refused,
    ShuttingDown,
}

impl Reply {
    fn byte(self) -> u8 {
        match self {
            Self::Accepted => 0,
            Self::Refused => 1,
            Self::ShuttingDown => 2,
        }
    }
    fn from_byte(byte: u8) -> io::Result<Self> {
        match byte {
            0 => Ok(Self::Accepted),
            1 => Ok(Self::Refused),
            2 => Ok(Self::ShuttingDown),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "unknown instance reply")),
        }
    }
}

pub struct PendingRequest {
    pub request: Request,
    pub reply: SyncSender<Reply>,
}

pub enum LaunchAction {
    Forwarded,
    Receiver(Receiver),
}

#[derive(Clone)]
pub struct ProfileIdentity(Arc<[u8]>);

impl ProfileIdentity {
    pub fn resolve(profile: Option<&Path>) -> io::Result<Self> {
        let bytes = match profile {
            Some(profile) => {
                let canonical = fs::canonicalize(profile)?;
                let mut bytes = Vec::with_capacity(canonical.as_os_str().as_bytes().len() + 1);
                bytes.push(1);
                bytes.extend_from_slice(canonical.as_os_str().as_bytes());
                bytes
            }
            None => vec![0],
        };
        Ok(Self(bytes.into()))
    }
}

struct Endpoint {
    socket: PathBuf,
    lock: PathBuf,
}

fn fnv(seed: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(seed, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

fn endpoint(identity: &ProfileIdentity) -> io::Result<Endpoint> {
    let directory = std::env::temp_dir();
    let metadata = fs::symlink_metadata(&directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the temporary directory is not owner-private; use --new-instance"));
    }
    let first = fnv(0xcbf29ce484222325, &identity.0);
    let second = fnv(0x84222325cbf29ce4, &identity.0);
    let stem = format!("dp1-{first:016x}{second:016x}");
    let socket = directory.join(format!("{stem}.s"));
    if socket.as_os_str().as_bytes().len() >= 104 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the instance socket path is too long; use --new-instance"));
    }
    Ok(Endpoint { socket, lock: directory.join(format!("{stem}.l")) })
}

fn lock_file(path: &Path) -> io::Result<File> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(io::Error::other("the instance lock is not a regular file"));
    }
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("the instance lock is not a regular file"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(path),
        Ok(_) => Err(io::Error::other("the instance socket path is not a socket")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn push_bytes(frame: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    let length = u32::try_from(bytes.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "instance field is too large"))?;
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(bytes);
    Ok(())
}

fn encode(identity: &ProfileIdentity, request: &Request) -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    frame.extend_from_slice(MAGIC);
    frame.push(match request {
        Request::Raise => KIND_RAISE,
        Request::Open(_) => KIND_OPEN,
    });
    push_bytes(&mut frame, &identity.0)?;
    if let Request::Open(target) = request {
        push_bytes(&mut frame, path_from_location(&target.folder).as_os_str().as_bytes())?;
        match &target.select {
            Some(name) => {
                frame.push(1);
                push_bytes(&mut frame, name.as_bytes())?;
            }
            None => frame.push(0),
        }
    }
    if frame.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "instance request is too large"));
    }
    Ok(frame)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Decoder<'a> {
    fn take(&mut self, length: usize) -> io::Result<&'a [u8]> {
        let end = self.at.checked_add(length).filter(|end| *end <= self.bytes.len()).ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated instance request"))?;
        let value = &self.bytes[self.at..end];
        self.at = end;
        Ok(value)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn field(&mut self) -> io::Result<&'a [u8]> {
        let length = u32::from_be_bytes(self.take(4)?.try_into().expect("four bytes"));
        self.take(length as usize)
    }
}

fn decode(identity: &ProfileIdentity, frame: &[u8]) -> io::Result<Request> {
    if frame.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "oversized instance request"));
    }
    let mut decoder = Decoder { bytes: frame, at: 0 };
    if decoder.take(MAGIC.len())? != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "wrong instance protocol version"));
    }
    let kind = decoder.byte()?;
    if decoder.field()? != &*identity.0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "instance profile mismatch"));
    }
    let request = match kind {
        KIND_RAISE => Request::Raise,
        KIND_OPEN => {
            let folder = PathBuf::from(std::ffi::OsString::from_vec(decoder.field()?.to_vec()));
            if !folder.is_absolute() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "instance path is not absolute"));
            }
            let folder = location_from_path(&folder).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unsupported instance path"))?;
            let select = match decoder.byte()? {
                0 => None,
                1 => Some(EntryName::new(decoder.field()?.to_vec()).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid selected name"))?),
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid selected-name marker")),
            };
            Request::Open(LaunchTarget { folder, select })
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown instance request")),
    };
    if decoder.at != frame.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "trailing instance request bytes"));
    }
    Ok(request)
}

fn exchange(mut stream: UnixStream, frame: &[u8]) -> io::Result<Reply> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(frame)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut reply = [0_u8; 1];
    stream.read_exact(&mut reply)?;
    Reply::from_byte(reply[0])
}

fn forward(path: &Path, frame: &[u8]) -> io::Result<Reply> {
    exchange(UnixStream::connect(path)?, frame)
}

fn connection_absent(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused)
}

pub fn forward_or_claim(identity: ProfileIdentity, request: Request) -> Result<LaunchAction, String> {
    let endpoint = endpoint(&identity).map_err(|error| error.to_string())?;
    let frame = encode(&identity, &request).map_err(|error| error.to_string())?;
    match forward(&endpoint.socket, &frame) {
        Ok(Reply::Accepted) => return Ok(LaunchAction::Forwarded),
        Ok(Reply::Refused) => return Err("the running instance refused the request".into()),
        Ok(Reply::ShuttingDown) => return Err("the running instance is quitting; try again".into()),
        Err(error) if !connection_absent(&error) => {
            return retry_forward(&endpoint.socket, &frame, true).map(|()| LaunchAction::Forwarded).map_err(|retry| format!("the running instance did not acknowledge the request: {retry}"));
        }
        Err(_) => {}
    }

    let lock = lock_file(&endpoint.lock).map_err(|error| error.to_string())?;
    match lock.try_lock() {
        Ok(()) => {
            remove_stale_socket(&endpoint.socket).map_err(|error| error.to_string())?;
            let listener = UnixListener::bind(&endpoint.socket).map_err(|error| error.to_string())?;
            fs::set_permissions(&endpoint.socket, fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;
            listener.set_nonblocking(true).map_err(|error| error.to_string())?;
            Ok(LaunchAction::Receiver(Receiver { identity, listener, lock, socket: endpoint.socket }))
        }
        Err(TryLockError::WouldBlock) => retry_forward(&endpoint.socket, &frame, false).map(|()| LaunchAction::Forwarded).map_err(|error| format!("another launcher won the startup race but did not become ready: {error}")),
        Err(TryLockError::Error(error)) => Err(error.to_string()),
    }
}

fn retry_forward(path: &Path, frame: &[u8], mut attempted_exchange: bool) -> Result<(), String> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    let mut last = None;
    while Instant::now() < deadline {
        match forward(path, frame) {
            Ok(Reply::Accepted) => return Ok(()),
            Ok(Reply::Refused) => return Err("request refused".into()),
            Ok(Reply::ShuttingDown) => return Err("instance is shutting down".into()),
            Err(error) => {
                if !connection_absent(&error) {
                    if attempted_exchange {
                        return Err(error.to_string());
                    }
                    attempted_exchange = true;
                }
                last = Some(error);
            }
        }
        thread::sleep(RETRY_DELAY);
    }
    Err(last.map_or_else(|| "timed out".into(), |error| error.to_string()))
}

pub struct Receiver {
    identity: ProfileIdentity,
    listener: UnixListener,
    lock: File,
    socket: PathBuf,
}

impl Receiver {
    pub fn start(self, enqueue: Arc<dyn Fn(PendingRequest) + Send + Sync>) -> io::Result<Server> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let identity = self.identity.clone();
        let listener = self.listener;
        let thread = thread::Builder::new().name("dual-pane-instance".into()).spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => serve_connection(stream, &identity, &enqueue, &thread_stop),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
                    Err(_) if thread_stop.load(Ordering::Acquire) => break,
                    Err(error) => {
                        eprintln!("Dual Pane instance receiver failed: {error}");
                        thread::sleep(ACCEPT_POLL);
                    }
                }
            }
        })?;
        Ok(Server { stop, thread: Some(thread), lock: self.lock, socket: self.socket })
    }
}

fn serve_connection(mut stream: UnixStream, identity: &ProfileIdentity, enqueue: &Arc<dyn Fn(PendingRequest) + Send + Sync>, stopping: &AtomicBool) {
    // A short read timeout makes shutdown prompt even when a peer sent a
    // partial request and kept its connection open. The whole-request
    // deadline still bounds an ordinary stalled client.
    let _ = stream.set_read_timeout(Some(ACCEPT_POLL));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let mut frame = Vec::new();
    let deadline = Instant::now() + IO_TIMEOUT;
    let decoded = loop {
        if stopping.load(Ordering::Acquire) {
            break Err(io::Error::new(io::ErrorKind::Interrupted, "instance receiver is shutting down"));
        }
        if Instant::now() >= deadline {
            break Err(io::Error::new(io::ErrorKind::TimedOut, "instance request deadline expired"));
        }
        let mut bytes = [0_u8; 4096];
        match stream.read(&mut bytes) {
            Ok(0) => break decode(identity, &frame),
            Ok(count) => {
                if frame.len().saturating_add(count) > MAX_FRAME {
                    break Err(io::Error::new(io::ErrorKind::InvalidData, "oversized instance request"));
                }
                frame.extend_from_slice(&bytes[..count]);
            }
            Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted) => {}
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) && Instant::now() < deadline => {}
            Err(error) => break Err(error),
        }
    };
    let reply = match decoded {
        Ok(request) => {
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            enqueue(PendingRequest { request, reply: sender });
            receiver.recv_timeout(IO_TIMEOUT).unwrap_or(Reply::ShuttingDown)
        }
        Err(_) => Reply::Refused,
    };
    let _ = stream.write_all(&[reply.byte()]);
}

pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    lock: File,
    socket: PathBuf,
}

impl Server {
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        // Remove the published endpoint while the rendezvous lock is still
        // held. A subsequent launcher then cannot connect to an instance
        // that has committed to quitting.
        if let Err(error) = fs::remove_file(&self.socket)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!("Dual Pane instance socket cleanup failed: {error}");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = fs::remove_file(&self.socket);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    fn identity(directory: &Path) -> ProfileIdentity {
        ProfileIdentity::resolve(Some(directory)).unwrap()
    }

    #[test]
    fn protocol_round_trips_non_utf8_and_rejects_bad_frames_and_profiles() {
        let profile = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let request = Request::Open(LaunchTarget { folder: location_from_path(folder.path()).unwrap(), select: Some(EntryName::new(vec![b'n', 0xff]).unwrap()) });
        let frame = encode(&identity(profile.path()), &request).unwrap();
        assert_eq!(decode(&identity(profile.path()), &frame).unwrap(), request);
        assert_eq!(decode(&identity(other.path()), &frame).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert!(decode(&identity(profile.path()), &frame[..frame.len() - 1]).is_err());
        let mut trailing = frame.clone();
        trailing.push(0);
        assert!(decode(&identity(profile.path()), &trailing).is_err());
        let mut wrong = frame;
        wrong[3] = b'2';
        assert!(decode(&identity(profile.path()), &wrong).is_err());
        let mut unknown_kind = encode(&identity(profile.path()), &Request::Raise).unwrap();
        unknown_kind[4] = 99;
        assert!(decode(&identity(profile.path()), &unknown_kind).is_err());
        assert!(decode(&identity(profile.path()), &vec![0; MAX_FRAME + 1]).is_err());
    }

    #[test]
    fn endpoint_is_short_and_profile_specific() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let one = endpoint(&identity(first.path())).unwrap();
        let two = endpoint(&identity(second.path())).unwrap();
        assert_ne!(one.socket, two.socket);
        assert!(one.socket.as_os_str().as_bytes().len() < 104);
    }

    #[test]
    fn stale_socket_is_replaced_and_removed_on_shutdown() {
        let profile = tempfile::tempdir().unwrap();
        let identity = identity(profile.path());
        let endpoint = endpoint(&identity).unwrap();
        let stale = UnixListener::bind(&endpoint.socket).unwrap();
        drop(stale);
        let LaunchAction::Receiver(receiver) = forward_or_claim(identity, Request::Raise).unwrap() else { panic!("receiver") };
        let server = receiver
            .start(Arc::new(|pending| {
                let _ = pending.reply.send(Reply::Accepted);
            }))
            .unwrap();
        assert!(endpoint.socket.exists());
        server.shutdown();
        assert!(!endpoint.socket.exists());
    }

    #[test]
    fn a_running_receiver_accepts_a_forwarded_request() {
        let profile = tempfile::tempdir().unwrap();
        let identity = identity(profile.path());
        let LaunchAction::Receiver(receiver) = forward_or_claim(identity.clone(), Request::Raise).unwrap() else { panic!("receiver") };
        let (seen_sender, seen_receiver) = std::sync::mpsc::sync_channel(1);
        let server = receiver
            .start(Arc::new(move |pending| {
                seen_sender.send(pending.request).unwrap();
                pending.reply.send(Reply::Accepted).unwrap();
            }))
            .unwrap();
        assert!(matches!(forward_or_claim(identity, Request::Raise).unwrap(), LaunchAction::Forwarded));
        assert_eq!(seen_receiver.recv_timeout(IO_TIMEOUT).unwrap(), Request::Raise);
        server.shutdown();
    }

    #[test]
    fn a_startup_loser_waits_for_the_receiver_to_begin_accepting() {
        let profile = tempfile::tempdir().unwrap();
        let identity = identity(profile.path());
        let LaunchAction::Receiver(receiver) = forward_or_claim(identity.clone(), Request::Raise).unwrap() else { panic!("receiver") };
        let loser = thread::spawn(move || forward_or_claim(identity, Request::Raise));
        thread::sleep(Duration::from_millis(50));
        let server = receiver
            .start(Arc::new(|pending| {
                let _ = pending.reply.send(Reply::Accepted);
            }))
            .unwrap();
        assert!(matches!(loser.join().unwrap().unwrap(), LaunchAction::Forwarded));
        server.shutdown();
    }

    #[test]
    fn a_stalled_client_is_bounded_and_refused() {
        let profile = tempfile::tempdir().unwrap();
        let identity = identity(profile.path());
        let (mut client, server) = UnixStream::pair().unwrap();
        let started = Instant::now();
        let worker = thread::spawn(move || {
            let enqueue: Arc<dyn Fn(PendingRequest) + Send + Sync> = Arc::new(|_| panic!("a partial frame must not be enqueued"));
            serve_connection(server, &identity, &enqueue, &AtomicBool::new(false));
        });
        client.set_read_timeout(Some(IO_TIMEOUT + Duration::from_secs(1))).unwrap();
        client.write_all(b"DPL").unwrap();
        let mut reply = [0_u8; 1];
        client.read_exact(&mut reply).unwrap();
        worker.join().unwrap();
        assert_eq!(Reply::from_byte(reply[0]).unwrap(), Reply::Refused);
        assert!(started.elapsed() < IO_TIMEOUT + Duration::from_secs(1));
    }

    #[test]
    fn a_client_dripping_bytes_cannot_extend_the_whole_request_deadline() {
        let profile = tempfile::tempdir().unwrap();
        let identity = identity(profile.path());
        let (mut client, server) = UnixStream::pair().unwrap();
        let mut reader = client.try_clone().unwrap();
        reader.set_read_timeout(Some(IO_TIMEOUT + Duration::from_secs(1))).unwrap();
        let worker = thread::spawn(move || {
            let enqueue: Arc<dyn Fn(PendingRequest) + Send + Sync> = Arc::new(|_| panic!("an incomplete frame must not be enqueued"));
            serve_connection(server, &identity, &enqueue, &AtomicBool::new(false));
        });
        let writer = thread::spawn(move || {
            for _ in 0..300 {
                if client.write_all(b"D").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let started = Instant::now();
        let mut reply = [0_u8; 1];
        reader.read_exact(&mut reply).unwrap();
        assert_eq!(Reply::from_byte(reply[0]).unwrap(), Reply::Refused);
        assert!(started.elapsed() < IO_TIMEOUT + Duration::from_secs(1));
        worker.join().unwrap();
        writer.join().unwrap();
    }

    #[test]
    fn shutdown_withdraws_the_endpoint_and_interrupts_a_stalled_client() {
        let profile = tempfile::tempdir().unwrap();
        let identity = identity(profile.path());
        let endpoint = endpoint(&identity).unwrap();
        let LaunchAction::Receiver(receiver) = forward_or_claim(identity, Request::Raise).unwrap() else { panic!("receiver") };
        let server = receiver
            .start(Arc::new(|pending| {
                let _ = pending.reply.send(Reply::Accepted);
            }))
            .unwrap();
        let mut client = UnixStream::connect(&endpoint.socket).unwrap();
        client.write_all(b"DPL").unwrap();
        thread::sleep(ACCEPT_POLL * 2);

        let started = Instant::now();
        server.shutdown();

        assert!(started.elapsed() < IO_TIMEOUT, "shutdown must not wait for the client deadline");
        assert!(!endpoint.socket.exists());
    }

    #[test]
    fn receiver_refusal_and_shutdown_are_reported_to_the_launcher() {
        for (reply, expected) in [(Reply::Refused, "refused"), (Reply::ShuttingDown, "quitting")] {
            let profile = tempfile::tempdir().unwrap();
            let identity = identity(profile.path());
            let LaunchAction::Receiver(receiver) = forward_or_claim(identity.clone(), Request::Raise).unwrap() else { panic!("receiver") };
            let server = receiver
                .start(Arc::new(move |pending| {
                    let _ = pending.reply.send(reply);
                }))
                .unwrap();
            let Err(error) = forward_or_claim(identity, Request::Raise) else { panic!("launcher must report the receiver reply") };
            assert!(error.contains(expected), "{error}");
            server.shutdown();
        }
    }
}
