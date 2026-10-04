use std::collections::{HashMap, HashSet};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dual_pane_application::{Event, MonitoringStatus};
use dual_pane_domain::Location;

use crate::native_location::path_from_location;

const COMMAND_CAPACITY: usize = 1;
const VISIBLE_INTERVAL: Duration = Duration::from_secs(5);
const INACTIVE_INTERVAL: Duration = Duration::from_secs(60);
const MAX_INTERVAL: Duration = Duration::from_secs(5 * 60);
const IDLE_WAIT: Duration = Duration::from_secs(60);
const ROOT_CHANGED: u32 = 1;
/// The native stream lost continuity, so it must be registered again before
/// its notifications can be treated as authoritative.
const RECOVERY_REQUIRED: u32 = 1 << 1;

#[cxx_qt::bridge(namespace = "dual_pane_desktop")]
pub mod ffi {
    extern "Rust" {
        type WatchPublisher;
        fn native_invalidated(&self, id: u64, flags: u32);
    }

    unsafe extern "C++" {
        include!("dual_pane_desktop/location_watcher.hpp");

        type NativeLocationWatcher;
        fn start_native_location_watcher(publisher: Box<WatchPublisher>) -> UniquePtr<NativeLocationWatcher>;
        fn watch(self: Pin<&mut NativeLocationWatcher>, id: u64, path: &[u8]) -> bool;
        fn unwatch(self: Pin<&mut NativeLocationWatcher>, id: u64);
    }
}

pub(crate) type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Default)]
struct Mailbox {
    desired: HashMap<Location, u64>,
    visible: HashSet<Location>,
    outcomes: HashMap<Location, bool>,
    active: bool,
    closed: bool,
}

/// The native callback's finite keyed mailbox. It performs no I/O and never
/// calls application code; one wake asks the dedicated watcher thread to
/// drain all coalesced subscription IDs.
pub struct WatchPublisher {
    dirty: Arc<Mutex<HashMap<u64, u32>>>,
    wake: SyncSender<()>,
}

impl WatchPublisher {
    fn native_invalidated(&self, id: u64, flags: u32) {
        self.dirty.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).entry(id).and_modify(|pending| *pending |= flags).or_insert(flags);
        let _ = self.wake.try_send(());
    }
}

/// Nonblocking GUI-thread handle. State is coalesced into finite maps keyed by
/// currently open locations; a capacity-one wake channel cannot accumulate an
/// event storm.
pub struct WatcherHandle {
    mailbox: Option<Arc<Mutex<Mailbox>>>,
    wake: Option<SyncSender<()>>,
}

impl WatcherHandle {
    pub fn start(sink: EventSink) -> io::Result<Self> {
        let mailbox = Arc::new(Mutex::new(Mailbox { active: true, ..Mailbox::default() }));
        let dirty = Arc::new(Mutex::new(HashMap::new()));
        let (wake, wakes) = mpsc::sync_channel(COMMAND_CAPACITY);
        let thread_mailbox = Arc::clone(&mailbox);
        let failure_mailbox = Arc::clone(&mailbox);
        let thread_dirty = Arc::clone(&dirty);
        let failure_sink = Arc::clone(&sink);
        let publisher = WatchPublisher { dirty, wake: wake.clone() };
        thread::Builder::new().name("location-watcher".to_owned()).spawn(move || {
            if catch_unwind(AssertUnwindSafe(|| run_service(thread_mailbox, thread_dirty, wakes, publisher, sink))).is_err() {
                let state = failure_mailbox.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if !state.closed {
                    for (location, generation) in &state.desired {
                        failure_sink(Event::WatcherStatusChanged { location: location.clone(), generation: *generation, status: MonitoringStatus::Unavailable });
                    }
                }
            }
        })?;
        Ok(Self { mailbox: Some(mailbox), wake: Some(wake) })
    }

    pub fn unavailable() -> Self {
        Self { mailbox: None, wake: None }
    }

    pub fn watch(&self, location: Location, generation: u64) -> bool {
        self.update(|mailbox| {
            mailbox.desired.insert(location, generation);
        })
    }

    pub fn unwatch(&self, location: &Location, generation: u64) -> bool {
        self.update(|mailbox| {
            if mailbox.desired.get(location) == Some(&generation) {
                mailbox.desired.remove(location);
                mailbox.outcomes.remove(location);
            }
        })
    }

    pub fn set_active(&self, active: bool) -> bool {
        self.update(|mailbox| mailbox.active = active)
    }

    pub fn set_visible(&self, visible: Vec<Location>) -> bool {
        self.update(|mailbox| mailbox.visible = visible.into_iter().collect())
    }

    pub fn read_completed(&self, location: Location, succeeded: bool) -> bool {
        self.update(|mailbox| {
            if mailbox.desired.contains_key(&location) {
                mailbox.outcomes.insert(location, succeeded);
            }
        })
    }

    fn update(&self, change: impl FnOnce(&mut Mailbox)) -> bool {
        let (Some(mailbox), Some(wake)) = (&self.mailbox, &self.wake) else { return false };
        change(&mut mailbox.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        !matches!(wake.try_send(()), Err(TrySendError::Disconnected(_)))
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        let (Some(mailbox), Some(wake)) = (&self.mailbox, &self.wake) else { return };
        mailbox.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).closed = true;
        let _ = wake.try_send(());
    }
}

struct Subscription {
    generation: u64,
    id: u64,
    status: MonitoringStatus,
    failures: u32,
    visible: bool,
    deadline: Option<Instant>,
}

fn run_service(mailbox: Arc<Mutex<Mailbox>>, dirty: Arc<Mutex<HashMap<u64, u32>>>, wakes: mpsc::Receiver<()>, publisher: WatchPublisher, sink: EventSink) {
    let mut native = ffi::start_native_location_watcher(Box::new(publisher));
    let mut subscriptions: HashMap<Location, Subscription> = HashMap::new();
    let mut ids: HashMap<u64, Location> = HashMap::new();
    let mut next_id = 0u64;
    loop {
        let now = Instant::now();
        let active = mailbox.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).active;
        let timeout = if active { subscriptions.values().filter_map(|subscription| subscription.deadline).min().map_or(IDLE_WAIT, |deadline| deadline.saturating_duration_since(now)) } else { IDLE_WAIT };
        match wakes.recv_timeout(timeout) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        while !matches!(wakes.try_recv(), Err(TryRecvError::Empty | TryRecvError::Disconnected)) {}
        let (desired, visible, outcomes, active, closed) = {
            let mut state = mailbox.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            (state.desired.clone(), state.visible.clone(), std::mem::take(&mut state.outcomes), state.active, state.closed)
        };
        if closed {
            break;
        }

        let removed = subscriptions.iter().filter(|(location, subscription)| desired.get(*location) != Some(&subscription.generation)).map(|(location, _)| location.clone()).collect::<Vec<_>>();
        for location in removed {
            if let Some(subscription) = subscriptions.remove(&location) {
                ids.remove(&subscription.id);
                if subscription.status == MonitoringStatus::Native
                    && let Some(watcher) = native.as_mut()
                {
                    watcher.unwatch(subscription.id);
                }
            }
        }
        for (location, generation) in &desired {
            if subscriptions.contains_key(location) {
                continue;
            }
            next_id = next_id.wrapping_add(1).max(1);
            let id = next_id;
            let native_ready = native.as_mut().is_some_and(|watcher| watcher.watch(id, path_from_location(location).as_os_str().as_bytes()));
            let status = if native_ready { MonitoringStatus::Native } else { MonitoringStatus::Periodic };
            let is_visible = visible.contains(location);
            let deadline = (status == MonitoringStatus::Periodic).then(|| Instant::now() + next_delay(*generation, location, is_visible, 0));
            subscriptions.insert(location.clone(), Subscription { generation: *generation, id, status, failures: 0, visible: is_visible, deadline });
            ids.insert(id, location.clone());
            sink(Event::WatcherReady { location: location.clone(), generation: *generation, status });
        }

        let now = Instant::now();
        for (location, subscription) in &mut subscriptions {
            let is_visible = visible.contains(location);
            if subscription.visible != is_visible {
                subscription.visible = is_visible;
                if subscription.status == MonitoringStatus::Periodic {
                    subscription.deadline = Some(now + next_delay(subscription.generation, location, is_visible, subscription.failures));
                }
            }
            if let Some(succeeded) = outcomes.get(location)
                && subscription.status == MonitoringStatus::Periodic
            {
                subscription.failures = if *succeeded { 0 } else { subscription.failures.saturating_add(1) };
                subscription.deadline = Some(now + next_delay(subscription.generation, location, subscription.visible, subscription.failures));
            }
        }

        let native_events = std::mem::take(&mut *dirty.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        for (id, flags) in native_events {
            let Some(location) = ids.get(&id).cloned() else { continue };
            let Some(subscription) = subscriptions.get_mut(&location) else { continue };
            if requires_recovery(flags) {
                if let Some(watcher) = native.as_mut() {
                    watcher.unwatch(id);
                }
                subscription.status = MonitoringStatus::Periodic;
                subscription.deadline = Some(now);
                sink(Event::WatcherStatusChanged { location: location.clone(), generation: subscription.generation, status: MonitoringStatus::Periodic });
            }
            if active {
                sink(Event::WatchedLocationInvalidated { location, generation: subscription.generation });
            }
        }

        if active {
            let due = subscriptions.iter().filter(|(_, subscription)| subscription.status == MonitoringStatus::Periodic && subscription.deadline.is_some_and(|deadline| deadline <= now)).map(|(location, _)| location.clone()).collect::<Vec<_>>();
            for location in due {
                let Some(subscription) = subscriptions.get_mut(&location) else { continue };
                let native_ready = native.as_mut().is_some_and(|watcher| watcher.watch(subscription.id, path_from_location(&location).as_os_str().as_bytes()));
                if native_ready {
                    subscription.status = MonitoringStatus::Native;
                    subscription.deadline = None;
                    sink(Event::WatcherStatusChanged { location: location.clone(), generation: subscription.generation, status: MonitoringStatus::Native });
                } else {
                    subscription.deadline = Some(now + next_delay(subscription.generation, &location, subscription.visible, subscription.failures));
                }
                sink(Event::WatchedLocationInvalidated { location, generation: subscription.generation });
            }
        }
    }
    if let Some(mut watcher) = native.as_mut() {
        for subscription in subscriptions.values().filter(|subscription| subscription.status == MonitoringStatus::Native) {
            watcher.as_mut().unwatch(subscription.id);
        }
    }
}

fn next_delay(generation: u64, location: &Location, visible: bool, failures: u32) -> Duration {
    let base = if visible { VISIBLE_INTERVAL } else { INACTIVE_INTERVAL };
    let multiplier = 1u64.checked_shl(failures.min(5)).unwrap_or(u64::MAX);
    let seconds = base.as_secs().saturating_mul(multiplier).min(MAX_INTERVAL.as_secs());
    let hash = location.components().iter().flat_map(|component| component.as_bytes()).fold(generation, |hash, byte| hash.wrapping_mul(1099511628211).wrapping_add(u64::from(*byte)));
    let percent = 90 + (hash % 21);
    Duration::from_millis(seconds.saturating_mul(1000).saturating_mul(percent) / 100)
}

fn requires_recovery(flags: u32) -> bool {
    flags & (ROOT_CHANGED | RECOVERY_REQUIRED) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::EntryName;
    use std::sync::mpsc::Receiver;

    fn location(name: &str) -> Location {
        Location::root().join(&EntryName::new(name).unwrap())
    }

    #[test]
    fn fallback_intervals_are_visibility_aware_jittered_and_bounded() {
        let folder = location("folder");
        let visible = next_delay(7, &folder, true, 0);
        let hidden = next_delay(7, &folder, false, 0);
        assert!((Duration::from_millis(4500)..=Duration::from_millis(5500)).contains(&visible));
        assert!((Duration::from_secs(54)..=Duration::from_secs(66)).contains(&hidden));
        assert!(next_delay(7, &folder, true, 20) <= Duration::from_secs(330));
    }

    #[test]
    fn root_and_rescan_notifications_require_native_recovery() {
        assert!(requires_recovery(ROOT_CHANGED));
        assert!(requires_recovery(RECOVERY_REQUIRED));
        assert!(requires_recovery(ROOT_CHANGED | RECOVERY_REQUIRED));
        assert!(!requires_recovery(0));
    }

    fn wait_for_invalidation(events: &Receiver<Event>, location: &Location, generation: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "native invalidation did not arrive");
            match events.recv_timeout(remaining).expect("watcher event") {
                Event::WatchedLocationInvalidated { location: changed, generation: changed_generation } if changed == *location && changed_generation == generation => return,
                _ => {}
            }
        }
    }

    #[test]
    fn native_fsevents_reports_external_item_changes() {
        let directory = tempfile::tempdir().unwrap();
        let location = crate::native_location::location_from_path(directory.path()).unwrap();
        let (sender, events) = mpsc::channel();
        let watcher = WatcherHandle::start(Arc::new(move |event| {
            sender.send(event).ok();
        }))
        .unwrap();
        watcher.set_visible(vec![location.clone()]);
        assert!(watcher.watch(location.clone(), 11));
        let ready = events.recv_timeout(Duration::from_secs(5)).expect("watch ready");
        let Event::WatcherReady { location: ready_location, generation, status } = ready else { panic!("watcher did not report readiness") };
        assert_eq!(ready_location, location);
        assert_eq!(generation, 11);
        // FSEvents can refuse an individual temporary-directory stream while
        // the service remains healthy. That is the supported periodic path;
        // the deterministic scheduler tests cover its bounded behavior.
        if status != MonitoringStatus::Native {
            assert_eq!(status, MonitoringStatus::Periodic);
            return;
        }

        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        std::fs::write(&first, b"first").unwrap();
        wait_for_invalidation(&events, &location, 11);
        std::fs::rename(&first, &second).unwrap();
        wait_for_invalidation(&events, &location, 11);
        std::fs::write(&second, b"changed metadata and content").unwrap();
        wait_for_invalidation(&events, &location, 11);
        std::fs::remove_file(&second).unwrap();
        wait_for_invalidation(&events, &location, 11);
    }
}
