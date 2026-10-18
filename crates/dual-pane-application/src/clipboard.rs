use std::sync::Arc;

use dual_pane_domain::{BrowserSide, Location, OperationRoot, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipboardGeneration(u64);
impl ClipboardGeneration {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClipboardRequestId(u64);
impl ClipboardRequestId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId(u64);
impl TransferId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAction {
    Copy,
    Cut,
}

/// Cheap native type information used only to enable Paste. Invocation still
/// performs a generation-tagged read and validates the complete snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardOffer {
    Private,
    Files,
    PrivateAndFiles,
    #[default]
    Unsupported,
}

impl ClipboardOffer {
    pub const fn supports_paste(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardSnapshot {
    Private {
        transfer: TransferId,
    },
    ExternalFiles {
        sources: Arc<[Location]>,
    },
    PrivateAndExternal {
        transfer: TransferId,
        sources: Arc<[Location]>,
    },
    Unsupported,
    /// The clipboard worker could not take the read, so the content is unknown.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardRejection {
    StaleTab,
    StaleObservation,
    NoSelection,
    SourceUnavailable,
    DestinationUnavailable,
    PublicationFailed,
    ClipboardBusy,
    ClipboardReplaced,
    UnknownPrivateTransfer,
    TransferReserved,
    UnsupportedContent,
    MalformedExternalContent,
    DuplicateSource,
    SourceChanged,
    SameSourceAndDestination,
    DestinationWithinSource,
    JournalUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapturedTransfer {
    pub browser: BrowserSide,
    pub tab: TabId,
    pub action: ClipboardAction,
    pub roots: Arc<[OperationRoot]>,
    pub reserved: bool,
}
