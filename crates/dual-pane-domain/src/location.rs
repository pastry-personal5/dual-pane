use std::borrow::Cow;
use std::fmt;

/// The exact name of one directory entry, stored as the bytes the file system
/// reports.
///
/// Equality and lookup always use the exact bytes, so two names that render
/// alike stay distinct. Text is only a rendering for display and sorting.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EntryName(Box<[u8]>);

/// Why a byte sequence is not a valid entry name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidEntryName {
    Empty,
    ContainsSlash,
    ContainsNul,
    DotOrDotDot,
}

impl EntryName {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, InvalidEntryName> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(InvalidEntryName::Empty);
        }
        if bytes.contains(&b'/') {
            return Err(InvalidEntryName::ContainsSlash);
        }
        if bytes.contains(&0) {
            return Err(InvalidEntryName::ContainsNul);
        }
        if bytes == b"." || bytes == b".." {
            return Err(InvalidEntryName::DotOrDotDot);
        }
        Ok(Self(bytes.into_boxed_slice()))
    }

    /// The exact name.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// A text rendering for display and sorting only. Bytes that are not
    /// valid UTF-8 appear as U+FFFD.
    pub fn to_text_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

impl fmt::Debug for EntryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntryName(\"{}\")", self.0.escape_ascii())
    }
}

impl fmt::Display for InvalidEntryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::Empty => "an entry name is empty",
            Self::ContainsSlash => "an entry name contains '/'",
            Self::ContainsNul => "an entry name contains a NUL byte",
            Self::DotOrDotDot => "an entry name is '.' or '..'",
        };
        f.write_str(reason)
    }
}

impl std::error::Error for InvalidEntryName {}

/// An absolute, logical location: the ordered entry names below the root.
///
/// A location never resolves symbolic links, so the parent of a location
/// entered through a link is the location that contains the link.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Location {
    components: Vec<EntryName>,
}

impl Location {
    pub fn root() -> Self {
        Self::default()
    }

    pub fn from_components(components: impl IntoIterator<Item = EntryName>) -> Self {
        Self { components: components.into_iter().collect() }
    }

    pub fn components(&self) -> &[EntryName] {
        &self.components
    }

    pub fn is_root(&self) -> bool {
        self.components.is_empty()
    }

    #[must_use]
    pub fn join(&self, name: &EntryName) -> Self {
        let mut components = self.components.clone();
        components.push(name.clone());
        Self { components }
    }

    /// The containing location, or `None` at the root.
    pub fn parent(&self) -> Option<Self> {
        let (_, parent) = self.components.split_last()?;
        Some(Self { components: parent.to_vec() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_exact_bytes_including_invalid_utf8() {
        let name = EntryName::new(b"caf\xFF".to_vec()).unwrap();
        assert_eq!(name.as_bytes(), b"caf\xFF");
        assert_eq!(name.to_text_lossy(), "caf\u{FFFD}");
    }

    #[test]
    fn names_that_render_alike_stay_distinct() {
        let first = EntryName::new(b"a\xFF".to_vec()).unwrap();
        let second = EntryName::new(b"a\xFE".to_vec()).unwrap();
        assert_eq!(first.to_text_lossy(), second.to_text_lossy());
        assert_ne!(first, second);
    }

    #[test]
    fn rejects_invalid_names() {
        assert_eq!(EntryName::new(""), Err(InvalidEntryName::Empty));
        assert_eq!(EntryName::new("a/b"), Err(InvalidEntryName::ContainsSlash));
        assert_eq!(EntryName::new(b"a\0b".to_vec()), Err(InvalidEntryName::ContainsNul));
        assert_eq!(EntryName::new("."), Err(InvalidEntryName::DotOrDotDot));
        assert_eq!(EntryName::new(".."), Err(InvalidEntryName::DotOrDotDot));
    }

    #[test]
    fn accepts_names_that_only_start_with_dots() {
        assert!(EntryName::new(".hidden").is_ok());
        assert!(EntryName::new("...").is_ok());
    }
}
