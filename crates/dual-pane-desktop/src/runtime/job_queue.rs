//! Bounded queues that hand jobs to the listing workers.

use super::*;

pub(super) struct JobQueue {
    pub(super) state: Mutex<QueueState>,
    pub(super) ready: Condvar,
}

#[derive(Default)]
pub(super) struct QueueState {
    pub(super) pending: VecDeque<Job>,
    pub(super) closed: bool,
    pub(super) receivers: usize,
}

pub(super) const PENDING_READ_CAPACITY: usize = 16;

/// Probes that may wait for the probe lane before admission fails.
pub(super) const PENDING_PROBE_CAPACITY: usize = 16;

/// Concurrent readers per Browser. Provisional: P3-M6 and later measurement may
/// tune it, but it stays a finite bound so blocked I/O cannot grow threads.
pub const READER_LANES_PER_BROWSER: usize = 4;

pub(super) struct JobSender(Arc<JobQueue>);

pub(super) struct JobReceiver(Arc<JobQueue>);

pub(super) fn job_queue() -> (JobSender, JobReceiver) {
    let queue = Arc::new(JobQueue { state: Mutex::new(QueueState { receivers: 1, ..QueueState::default() }), ready: Condvar::new() });
    (JobSender(Arc::clone(&queue)), JobReceiver(queue))
}

impl JobSender {
    pub(super) fn send(&self, job: Job) -> Result<(), (Job, bool)> {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending.retain(|pending| !pending.state.cancelled.load(Ordering::Relaxed));
        if state.closed || state.pending.len() >= PENDING_READ_CAPACITY {
            return Err((job, state.closed));
        }
        state.pending.push_back(job);
        self.0.ready.notify_one();
        Ok(())
    }
}

impl Drop for JobSender {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        self.0.ready.notify_all();
    }
}

impl Iterator for JobReceiver {
    type Item = Job;

    fn next(&mut self) -> Option<Self::Item> {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(job) = state.pending.pop_front() {
                return Some(job);
            }
            if state.closed {
                return None;
            }
            state = self.0.ready.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Every lane of a Browser holds a receiver for the same queue.
impl Clone for JobReceiver {
    fn clone(&self) -> Self {
        self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).receivers += 1;
        Self(Arc::clone(&self.0))
    }
}

impl Drop for JobReceiver {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receivers = state.receivers.saturating_sub(1);
        if state.receivers == 0 {
            state.closed = true;
        }
        self.0.ready.notify_all();
    }
}
