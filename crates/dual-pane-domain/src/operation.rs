use std::collections::HashSet;

use crate::{EntryKind, EntryName, Location};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(u64);
impl OperationId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecisionToken(u64);
impl DecisionToken {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationKind {
    Copy,
    Move,
    Rename { to: EntryName },
    NewFolder { name: EntryName },
    MoveToTrash,
    DeletePermanently,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationTarget {
    pub name: EntryName,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationIntent {
    kind: OperationKind,
    source: Location,
    targets: Vec<OperationTarget>,
    destination: Option<Location>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationRejection {
    StaleTab,
    NoTargets,
    StaleSelection,
    DuplicateTarget,
    InvalidTargetCount,
    InvalidName,
    UnchangedName,
    DestinationRequired,
    DestinationNotAllowed,
    SameSourceAndDestination,
    DestinationWithinSource,
    SourceUnavailable,
    DestinationUnavailable,
}

impl OperationIntent {
    pub fn new(kind: OperationKind, source: Location, targets: Vec<OperationTarget>, destination: Option<Location>) -> Result<Self, OperationRejection> {
        if matches!(kind, OperationKind::NewFolder { .. }) {
            if !targets.is_empty() {
                return Err(OperationRejection::InvalidTargetCount);
            }
        } else if targets.is_empty() {
            return Err(OperationRejection::NoTargets);
        }
        if matches!(kind, OperationKind::Rename { .. }) && targets.len() != 1 {
            return Err(OperationRejection::InvalidTargetCount);
        }
        let mut seen = HashSet::new();
        if targets.iter().any(|target| !seen.insert(&target.name)) {
            return Err(OperationRejection::DuplicateTarget);
        }
        if let OperationKind::Rename { to } | OperationKind::NewFolder { name: to } = &kind {
            if to.to_text_lossy().trim().is_empty() {
                return Err(OperationRejection::InvalidName);
            }
            if matches!(kind, OperationKind::Rename { .. }) && targets[0].name == *to {
                return Err(OperationRejection::UnchangedName);
            }
        }
        match kind {
            OperationKind::Copy | OperationKind::Move => match &destination {
                None => return Err(OperationRejection::DestinationRequired),
                Some(dest) if dest == &source => return Err(OperationRejection::SameSourceAndDestination),
                Some(dest) if targets.iter().any(|target| target.kind == EntryKind::Directory && starts_with(dest, &source.join(&target.name))) => return Err(OperationRejection::DestinationWithinSource),
                Some(_) => {}
            },
            _ if destination.is_some() => return Err(OperationRejection::DestinationNotAllowed),
            _ => {}
        }
        Ok(Self { kind, source, targets, destination })
    }
    pub fn kind(&self) -> &OperationKind {
        &self.kind
    }
    pub fn source(&self) -> &Location {
        &self.source
    }
    pub fn targets(&self) -> &[OperationTarget] {
        &self.targets
    }
    pub fn destination(&self) -> Option<&Location> {
        self.destination.as_ref()
    }
}

pub fn starts_with(location: &Location, root: &Location) -> bool {
    location.components().starts_with(root.components())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationIssue {
    FileConflict,
    LinkCollision,
    KindMismatch,
    RecoverableError,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationChoice {
    Skip,
    Replace,
    TryAgain,
    Cancel,
}
impl OperationIssue {
    pub fn permits(self, choice: OperationChoice, apply_to_all: bool) -> bool {
        let allowed = match self {
            Self::FileConflict => matches!(choice, OperationChoice::Skip | OperationChoice::Replace | OperationChoice::Cancel),
            Self::LinkCollision | Self::KindMismatch | Self::RecoverableError => matches!(choice, OperationChoice::TryAgain | OperationChoice::Skip | OperationChoice::Cancel),
        };
        allowed && (!apply_to_all || self == Self::FileConflict && choice != OperationChoice::Cancel)
    }
}
