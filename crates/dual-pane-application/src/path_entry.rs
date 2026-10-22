//! Parsing of entered path text into a candidate [`Location`].
//!
//! Parsing is pure and never touches the file system. The candidate keeps the
//! typed spelling of every component, so a worker still has to resolve it to
//! the stored names before the tab navigates.

use dual_pane_domain::{EntryName, ListingErrorKind, Location};

/// What the person submitted from the Path Edit Control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSubmission {
    /// Text the person typed or edited.
    Typed(String),
    /// Return pressed without editing: the exact location the editor was
    /// filled with, which is never reparsed from its lossy display text.
    Unchanged(Location),
    /// Text that could not be decoded, for example an unpaired surrogate.
    Undecodable,
}

/// Why entered text was refused before any check ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathRejection {
    /// Empty, a control character, or otherwise not a path.
    Malformed,
    /// A relative path was entered while the tab has no confirmed folder.
    NoConfirmedFolder,
    /// The text shows undecodable bytes (U+FFFD) and does not name them.
    UndecodableText,
}

/// Parses `text` against the submitting tab's confirmed folder and `home`.
///
/// `..` removes the previous component lexically and never checks that it
/// exists. At the root it stays at the root. Spaces are literal.
pub fn parse_path(text: &str, base: Option<&Location>, home: &Location) -> Result<Location, PathRejection> {
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(PathRejection::Malformed);
    }
    if text.contains('\u{FFFD}') {
        return Err(PathRejection::UndecodableText);
    }
    let (start, rest) = if let Some(rest) = text.strip_prefix('/') {
        (Location::root(), rest)
    } else if text == "~" {
        (home.clone(), "")
    } else if let Some(rest) = text.strip_prefix("~/") {
        (home.clone(), rest)
    } else {
        (base.ok_or(PathRejection::NoConfirmedFolder)?.clone(), text)
    };
    let mut components: Vec<EntryName> = start.components().to_vec();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            name => components.push(EntryName::new(name).map_err(|_| PathRejection::Malformed)?),
        }
    }
    Ok(Location::from_components(components))
}

/// Why a submission failed, either before any check or in the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathEntryFailure {
    Rejected(PathRejection),
    Listing(ListingErrorKind),
}

/// What a resolved entered path names. Packages and links to folders or
/// packages are `Folder`; a file or a link to one is `File`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathTarget {
    Folder,
    File,
}

/// An entered path rebuilt from stored names. A link keeps its own name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    pub location: Location,
    pub target: PathTarget,
}
