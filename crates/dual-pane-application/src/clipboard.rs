use std::sync::Arc;

use dual_pane_domain::{Location, OperationRoot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardSnapshot {
    Private { transfer: TransferId },
    ExternalFiles { sources: Arc<[Location]> },
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardRejection {
    StaleTab,
    StaleObservation,
    NoSelection,
    SourceUnavailable,
    DestinationUnavailable,
    PublicationFailed,
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
    pub action: ClipboardAction,
    pub roots: Arc<[OperationRoot]>,
    pub reserved: bool,
}
