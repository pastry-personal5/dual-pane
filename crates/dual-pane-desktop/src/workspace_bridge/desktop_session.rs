//! The process-wide Workspace session and single-instance request queue.

use super::*;

/// The one session both Browser models and the workspace binder share. Only
/// the GUI thread uses it, so the mutex is a holder rather than
/// synchronization; its order comes from the GUI thread's event loop. Each
/// caller releases it before notifying Qt, which may call back into a binder.
pub(super) static SESSION: OnceLock<Mutex<Option<WorkspaceSession>>> = OnceLock::new();

pub(super) static INSTANCE_REQUESTS: OnceLock<Mutex<VecDeque<PendingRequest>>> = OnceLock::new();

pub(super) static INSTANCE_SERVER: OnceLock<Mutex<Option<Server>>> = OnceLock::new();

pub(super) static INSTANCE_STOPPING: AtomicBool = AtomicBool::new(false);

pub(super) const INSTANCE_QUEUE_CAPACITY: usize = 16;

/// Runs `action` on the session, if it has started.
pub(crate) fn with_session<T>(action: impl FnOnce(&mut WorkspaceSession) -> T) -> Option<T> {
    SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_mut().map(action)
}

/// Ends the session after the Qt event loop has exited: unsaved settings get
/// a bounded flush, and outstanding reads are cancelled.
pub(super) fn shutdown_desktop() {
    INSTANCE_STOPPING.store(true, Ordering::Release);
    let pending = INSTANCE_REQUESTS.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).drain(..).collect::<Vec<_>>();
    for pending in pending {
        let _ = pending.reply.send(Reply::ShuttingDown);
    }
    if let Some(server) = INSTANCE_SERVER.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() {
        server.shutdown();
    }
    let session = SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    if let Some(session) = session {
        session.shutdown(SHUTDOWN_TIMEOUT);
    }
}

pub(super) fn drain_instance_requests() -> bool {
    let requests = {
        let mut queue = INSTANCE_REQUESTS.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let count = queue.len().min(INSTANCE_QUEUE_CAPACITY);
        let requests = queue.drain(..count).collect::<Vec<_>>();
        (requests, !queue.is_empty())
    };
    for pending in requests.0 {
        let reply = with_session(|session| {
            if session.is_quitting() {
                return Reply::ShuttingDown;
            }
            match pending.request {
                Request::Raise => session.submit(Command::RaiseWindow),
                Request::Open(LaunchTarget { folder, select }) => session.submit(Command::OpenLaunchTarget { folder, select }),
            }
            Reply::Accepted
        })
        .unwrap_or(Reply::Refused);
        let _ = pending.reply.send(reply);
    }
    requests.1
}
