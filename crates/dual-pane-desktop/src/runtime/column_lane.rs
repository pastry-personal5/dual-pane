//! Isolated, bounded reads for expanded path columns.

use super::*;
use dual_pane_domain::EntryName;

use crate::folder_items::{FolderChoicePage, ListingRules, read_folder_choices};
use crate::operation_journal::is_reserved_temporary;

pub const PENDING_COLUMN_CAPACITY: usize = 8;

#[derive(Clone)]
pub struct FolderChoiceRequest {
    pub id: u64,
    pub browser: BrowserSide,
    pub tab: TabId,
    pub source: Location,
    pub visible_revision: u64,
    pub parent: Location,
    pub selected: EntryName,
    pub after: Option<EntryName>,
    pub visibility: VisibilityPolicy,
    pub generation: u64,
}

pub struct FolderChoiceResult {
    pub request: FolderChoiceRequest,
    pub outcome: Result<FolderChoicePage, ListingErrorKind>,
}

pub(super) struct ColumnJob {
    pub(super) request: FolderChoiceRequest,
    pub(super) cancelled: Arc<AtomicBool>,
}

pub(super) fn start_column_lanes(delivery: Delivery) -> io::Result<([SyncSender<ColumnJob>; 2], Receiver<FolderChoiceResult>)> {
    let (results, receiver) = mpsc::channel();
    let mut senders = Vec::with_capacity(2);
    for browser in [BrowserSide::Left, BrowserSide::Right] {
        let (sender, jobs) = mpsc::sync_channel::<ColumnJob>(PENDING_COLUMN_CAPACITY);
        let results = results.clone();
        let delivery = delivery.clone();
        thread::Builder::new().name(format!("path-columns-{browser:?}")).spawn(move || {
            for job in jobs {
                if job.cancelled.load(Ordering::Relaxed) {
                    continue;
                }
                let outcome = panic::catch_unwind(AssertUnwindSafe(|| read_folder_choices(&job.request.parent, Some(&job.request.selected), job.request.after.as_ref(), job.request.visibility, &job.cancelled, &ListingRules { hidden: &is_reserved_temporary, package: &crate::native_shell::is_bundle, volume_root: &crate::native_calls::is_volume_root, invisible: &crate::native_calls::is_macos_invisible }))).unwrap_or(Some(Err(ListingErrorKind::Internal)));
                if let Some(outcome) = outcome
                    && !job.cancelled.load(Ordering::Relaxed)
                {
                    results.send(FolderChoiceResult { request: job.request, outcome }).ok();
                    wake(&delivery);
                }
            }
        })?;
        senders.push(sender);
    }
    Ok((senders.try_into().map_err(|_| io::Error::from(io::ErrorKind::Other))?, receiver))
}
