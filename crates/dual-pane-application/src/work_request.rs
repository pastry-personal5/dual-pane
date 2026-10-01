use crate::SettingsSnapshot;
use dual_pane_domain::{BrowserSide, Location, RequestToken, SortSpec, TabId};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkRequest {
    ReadDirectory { browser: BrowserSide, tab: TabId, token: RequestToken, location: Location, sort: SortSpec },
    Cancel { browser: BrowserSide, tab: TabId, token: RequestToken },
    SaveSettings { revision: u64, snapshot: SettingsSnapshot },
}
