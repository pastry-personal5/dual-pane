use std::sync::Arc;

use dual_pane_domain::{BrowserSide, EntryName, Location, OperationRoot, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DragId(u64);

impl DragId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAction {
    Move,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    BrowserFolder { browser: BrowserSide, tab: TabId },
    FolderRow { browser: BrowserSide, tab: TabId, visible_revision: u64, name: EntryName },
    BrowserTab { browser: BrowserSide, tab: TabId },
}

impl DropTarget {
    pub const fn browser(&self) -> BrowserSide {
        match self {
            Self::BrowserFolder { browser, .. } | Self::FolderRow { browser, .. } | Self::BrowserTab { browser, .. } => *browser,
        }
    }
    pub const fn tab(&self) -> TabId {
        match self {
            Self::BrowserFolder { tab, .. } | Self::FolderRow { tab, .. } | Self::BrowserTab { tab, .. } => *tab,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropRejection {
    UnknownDrag,
    StaleSource,
    StaleTarget,
    NoSelection,
    UnsupportedSource,
    UnsupportedTarget,
    UnsupportedAction,
    DestinationUnavailable,
    SameSourceAndDestination,
    DestinationWithinSource,
    DuplicateSource,
    JournalUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropPreview {
    pub drag: DragId,
    pub browser: BrowserSide,
    pub tab: TabId,
    pub action: DropAction,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DropRequestId(u64);

impl DropRequestId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropPreflight {
    pub request: DropRequestId,
    pub candidate: Location,
    pub roots: Arc<[OperationRoot]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDropDestination {
    pub canonical: Location,
    pub canonical_roots: Arc<[OperationRoot]>,
}

#[derive(Debug)]
pub(crate) struct CapturedDrag {
    pub id: DragId,
    pub browser: BrowserSide,
    pub tab: TabId,
    pub source: Location,
    pub listing: dual_pane_domain::RequestToken,
    pub visible_revision: u64,
    pub roots: Arc<[OperationRoot]>,
}

#[derive(Debug)]
pub(crate) struct PendingDrop {
    pub drag: CapturedDrag,
    pub target: DropTarget,
    pub candidate: Location,
    pub action: DropAction,
}
