//! The lane that filters one Browser tab at a time.

use super::*;

pub(super) struct FilterLane {
    pub(super) queue: Arc<(Mutex<FilterQueue>, Condvar)>,
}

#[derive(Default)]
pub(super) struct FilterQueue {
    pub(super) pending: VecDeque<FilterRequest>,
    pub(super) latest: HashMap<TabId, u64>,
    pub(super) closed: bool,
}

impl FilterLane {
    pub(super) fn start(browser: BrowserSide, delivery: Delivery) -> io::Result<Self> {
        let queue = Arc::new((Mutex::new(FilterQueue::default()), Condvar::new()));
        let worker_queue = Arc::clone(&queue);
        thread::Builder::new().name(format!("filter-worker-{browser:?}")).spawn(move || {
            loop {
                let request = {
                    let (lock, ready) = &*worker_queue;
                    let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    while state.pending.is_empty() && !state.closed {
                        state = ready.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
                    }
                    if state.closed {
                        break;
                    }
                    state.pending.pop_front()
                };
                let Some(request) = request else { continue };
                let result = panic::catch_unwind(AssertUnwindSafe(|| project_filter(&request))).map_err(|_| FilterFailure::WorkerUnavailable);
                let current = {
                    let (lock, _) = &*worker_queue;
                    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).latest.get(&request.tab).copied() == Some(request.generation)
                };
                if current {
                    deliver_event(&delivery, Event::FilterProjected { request, result });
                }
            }
        })?;
        Ok(Self { queue })
    }

    pub(super) fn enqueue(&self, request: FilterRequest) -> bool {
        let (lock, ready) = &*self.queue;
        let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closed {
            return false;
        }
        state.pending.retain(|pending| pending.tab != request.tab);
        if state.pending.len() >= BrowserTabs::LIMIT {
            return false;
        }
        state.latest.insert(request.tab, request.generation);
        state.pending.push_back(request);
        ready.notify_one();
        true
    }

    pub(super) fn cancel(&self, tab: TabId) {
        let (lock, _) = &*self.queue;
        let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending.retain(|pending| pending.tab != tab);
        state.latest.remove(&tab);
    }
}

impl Drop for FilterLane {
    fn drop(&mut self) {
        let (lock, ready) = &*self.queue;
        lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).closed = true;
        ready.notify_all();
    }
}

pub(super) const PENDING_PREVIEW_CAPACITY: usize = 1;

pub(super) fn preview_index(browser: BrowserSide) -> usize {
    match browser {
        BrowserSide::Left => 0,
        BrowserSide::Right => 1,
    }
}
