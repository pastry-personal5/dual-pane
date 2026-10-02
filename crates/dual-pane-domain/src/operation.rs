use std::collections::HashSet;

use crate::{EntryKind, EntryName, Location};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(u64);
impl OperationId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecisionToken(u64);
impl DecisionToken {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn get(self) -> u64 {
        self.0
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

/// An operation kind without the name a Rename or New Folder proposes. It
/// names a file command for availability, before any name is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationCommand {
    Copy,
    Move,
    Rename,
    NewFolder,
    MoveToTrash,
    DeletePermanently,
}

impl OperationCommand {
    pub const ALL: [Self; 6] = [Self::Copy, Self::Move, Self::Rename, Self::NewFolder, Self::MoveToTrash, Self::DeletePermanently];
}

impl OperationKind {
    pub fn command(&self) -> OperationCommand {
        match self {
            Self::Copy => OperationCommand::Copy,
            Self::Move => OperationCommand::Move,
            Self::Rename { .. } => OperationCommand::Rename,
            Self::NewFolder { .. } => OperationCommand::NewFolder,
            Self::MoveToTrash => OperationCommand::MoveToTrash,
            Self::DeletePermanently => OperationCommand::DeletePermanently,
        }
    }
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
    /// Copy needs the operation safety journal, which could not be opened.
    JournalUnavailable,
}

impl OperationIntent {
    pub fn new(kind: OperationKind, source: Location, targets: Vec<OperationTarget>, destination: Option<Location>) -> Result<Self, OperationRejection> {
        Self::check(kind.command(), &source, &targets, destination.as_ref())?;
        if let OperationKind::Rename { to } | OperationKind::NewFolder { name: to } = &kind {
            if to.to_text_lossy().trim().is_empty() {
                return Err(OperationRejection::InvalidName);
            }
            if matches!(kind, OperationKind::Rename { .. }) && targets[0].name == *to {
                return Err(OperationRejection::UnchangedName);
            }
        }
        Ok(Self { kind, source, targets, destination })
    }
    /// The checks that do not depend on a proposed name, so a file command's
    /// availability is known before a name is typed. [`Self::new`] applies
    /// them first.
    pub fn check(command: OperationCommand, source: &Location, targets: &[OperationTarget], destination: Option<&Location>) -> Result<(), OperationRejection> {
        if command == OperationCommand::NewFolder {
            if !targets.is_empty() {
                return Err(OperationRejection::InvalidTargetCount);
            }
        } else if targets.is_empty() {
            return Err(OperationRejection::NoTargets);
        }
        if command == OperationCommand::Rename && targets.len() != 1 {
            return Err(OperationRejection::InvalidTargetCount);
        }
        let mut seen = HashSet::new();
        if targets.iter().any(|target| !seen.insert(&target.name)) {
            return Err(OperationRejection::DuplicateTarget);
        }
        match command {
            OperationCommand::Copy | OperationCommand::Move => match destination {
                None => Err(OperationRejection::DestinationRequired),
                Some(dest) if dest == source => Err(OperationRejection::SameSourceAndDestination),
                Some(dest) if targets.iter().any(|target| target.kind == EntryKind::Directory && starts_with(dest, &source.join(&target.name))) => Err(OperationRejection::DestinationWithinSource),
                Some(_) => Ok(()),
            },
            _ if destination.is_some() => Err(OperationRejection::DestinationNotAllowed),
            _ => Ok(()),
        }
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
