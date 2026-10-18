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

/// One frozen top-level source of a transfer. Its byte-exact location is the
/// identity; the kind is revalidated without following links before work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRoot {
    pub source: Location,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationIntent {
    kind: OperationKind,
    source: Location,
    targets: Vec<OperationTarget>,
    destination: Option<Location>,
    roots: Vec<OperationRoot>,
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
        let roots = match &kind {
            OperationKind::NewFolder { name } => vec![OperationRoot { source: source.join(name), kind: EntryKind::Directory }],
            _ => targets.iter().map(|target| OperationRoot { source: source.join(&target.name), kind: target.kind }).collect(),
        };
        Ok(Self { kind, source, targets, destination, roots })
    }
    /// Creates one Copy or Move from arbitrary frozen roots, including roots
    /// that do not share a parent. Clipboard order is retained.
    pub fn transfer(kind: OperationKind, roots: Vec<OperationRoot>, destination: Location) -> Result<Self, OperationRejection> {
        if !matches!(kind, OperationKind::Copy | OperationKind::Move) {
            return Err(OperationRejection::DestinationNotAllowed);
        }
        if roots.is_empty() {
            return Err(OperationRejection::NoTargets);
        }
        let mut seen = HashSet::new();
        if roots.iter().any(|root| !seen.insert(&root.source)) {
            return Err(OperationRejection::DuplicateTarget);
        }
        if roots.iter().enumerate().any(|(index, root)| roots.iter().skip(index + 1).any(|other| starts_with(&root.source, &other.source) || starts_with(&other.source, &root.source))) {
            return Err(OperationRejection::DuplicateTarget);
        }
        if roots.iter().any(|root| root.source.parent().as_ref() == Some(&destination)) {
            return Err(OperationRejection::SameSourceAndDestination);
        }
        if roots.iter().any(|root| root.kind == EntryKind::Directory && starts_with(&destination, &root.source)) {
            return Err(OperationRejection::DestinationWithinSource);
        }
        let source = roots[0].source.parent().ok_or(OperationRejection::SourceUnavailable)?;
        let targets = roots.iter().map(|root| Ok(OperationTarget { name: root.source.components().last().cloned().ok_or(OperationRejection::SourceUnavailable)?, kind: root.kind })).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { kind, source, targets, destination: Some(destination), roots })
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
    pub fn roots(&self) -> &[OperationRoot] {
        &self.roots
    }
    /// Maps a source item to its destination using the owning top-level root.
    pub fn destination_for(&self, source: &Location) -> Option<Location> {
        let destination = self.destination.as_ref()?;
        let root = self.roots.iter().find(|root| starts_with(source, &root.source))?;
        let relative = &source.components()[root.source.components().len()..];
        let leaf = root.source.components().last()?;
        Some(Location::from_components(destination.components().iter().cloned().chain(std::iter::once(leaf.clone())).chain(relative.iter().cloned())))
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
