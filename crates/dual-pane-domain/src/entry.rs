use std::cmp::{Ordering, Reverse};

use unicode_normalization::UnicodeNormalization;

use crate::{EntryName, SortDirection, SortField, SortSpec};

/// What kind of object a directory entry is, as classified by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
    Symlink { points_to_directory: bool },
    Other,
}

/// One entry in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    name: EntryName,
    kind: EntryKind,
    metadata: EntryMetadata,
}

/// Direct, non-recursive metadata supplied by a directory reader. Missing
/// values stay unknown; sorting never triggers another filesystem read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntryMetadata {
    modified_unix_seconds: Option<i64>,
    size_bytes: Option<u64>,
}

impl EntryMetadata {
    pub const fn new(modified_unix_seconds: Option<i64>, size_bytes: Option<u64>) -> Self {
        Self { modified_unix_seconds, size_bytes }
    }

    pub const fn modified_unix_seconds(self) -> Option<i64> {
        self.modified_unix_seconds
    }

    pub const fn size_bytes(self) -> Option<u64> {
        self.size_bytes
    }
}

impl Entry {
    pub fn new(name: EntryName, kind: EntryKind) -> Self {
        Self::with_metadata(name, kind, EntryMetadata::default())
    }

    pub fn with_metadata(name: EntryName, kind: EntryKind, metadata: EntryMetadata) -> Self {
        Self { name, kind, metadata }
    }

    pub fn name(&self) -> &EntryName {
        &self.name
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn metadata(&self) -> EntryMetadata {
        self.metadata
    }

    /// Whether opening this entry navigates into it: a directory, or a
    /// symbolic link to a directory.
    pub fn can_enter(&self) -> bool {
        matches!(self.kind, EntryKind::Directory | EntryKind::Symlink { points_to_directory: true })
    }
}

/// The position of an entry in a listing. Compute it once per entry, for
/// example with `sort_by_cached_key(listing_sort_key)`, and compare keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ListingSortKey {
    /// 0 for entries that can be entered, so they come first.
    group: u8,
    name: Vec<NameToken>,
    /// The exact bytes break every remaining tie, so the order is total.
    exact: Box<[u8]>,
}

/// The sort key for `entry`.
///
/// Entries that can be entered come first. Names then compare in natural,
/// case-insensitive order on their canonically composed (NFC) text: runs of
/// ASCII digits compare by numeric value and other characters by their
/// Unicode lowercase form. Names that differ only in Unicode composition
/// therefore sort together; they stay distinct entries.
pub fn listing_sort_key(entry: &Entry) -> ListingSortKey {
    let text: String = entry.name.to_text_lossy().nfc().collect();
    ListingSortKey { group: u8::from(!entry.can_enter()), name: name_tokens(&text), exact: entry.name.as_bytes().into() }
}

/// The Type text shown for `entry` and compared by Type sort: `[DIR]` for a
/// folder, `[LNK]` for any symbolic link, otherwise the lowercase text after
/// the last `.` in the name, which is empty for a name without one.
pub fn entry_type_text(entry: &Entry) -> String {
    match entry.kind {
        EntryKind::Directory => "[DIR]".to_owned(),
        EntryKind::Symlink { .. } => "[LNK]".to_owned(),
        EntryKind::File | EntryKind::Other => {
            let name = entry.name.to_text_lossy();
            name.rfind('.').map_or_else(String::new, |dot| name[dot + 1..].to_lowercase())
        }
    }
}

/// A sort value in its requested direction. One sort uses one variant, so the
/// variant order never decides a comparison.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Directed<T> {
    Ascending(T),
    Descending(Reverse<T>),
}

impl<T> Directed<T> {
    fn new(value: T, direction: SortDirection) -> Self {
        match direction {
            SortDirection::Ascending => Self::Ascending(value),
            SortDirection::Descending => Self::Descending(Reverse(value)),
        }
    }
}

/// Sorts entries according to one location-shared choice. Folders and links to
/// folders always lead; unavailable metadata follows known values. Every field
/// falls back to natural Name ascending so the order remains deterministic.
pub fn sort_entries(entries: &mut [Entry], spec: SortSpec) {
    let direction = spec.direction();
    let group = |entry: &Entry| u8::from(!entry.can_enter());
    match spec.field() {
        SortField::Name if direction == SortDirection::Ascending => entries.sort_by_cached_key(listing_sort_key),
        SortField::Name => entries.sort_by_cached_key(|entry| (group(entry), Reverse(listing_sort_key(entry)))),
        SortField::Type => entries.sort_by_cached_key(|entry| (group(entry), Directed::new(entry_type_text(entry), direction), listing_sort_key(entry))),
        field @ (SortField::Modified | SortField::Size) => entries.sort_by_cached_key(|entry| {
            let value = match field {
                SortField::Modified => entry.metadata.modified_unix_seconds.map(i128::from),
                _ => entry.metadata.size_bytes.map(i128::from),
            };
            (group(entry), u8::from(value.is_none()), value.map(|value| Directed::new(value, direction)), listing_sort_key(entry))
        }),
    }
}

/// A unit of natural ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NameToken {
    /// A run of ASCII digits with leading zeros removed, so the numeric value
    /// is compared by length and then digit by digit, without overflow.
    Number(Box<str>),
    /// The lowercase form of one character that is not an ASCII digit. A
    /// lowercase form has at most three characters; the rest is padded with
    /// NUL, which no name contains, so it compares like the lowercase text
    /// without allocating.
    Text([char; 3]),
}

impl Ord for NameToken {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(x), Self::Number(y)) => x.len().cmp(&y.len()).then_with(|| x.cmp(y)),
            (Self::Text(x), Self::Text(y)) => x.cmp(y),
            // Text never starts with an ASCII digit, so '0' stands for every
            // number and a number never equals text.
            (Self::Number(_), Self::Text(y)) => '0'.cmp(&y[0]),
            (Self::Text(x), Self::Number(_)) => x[0].cmp(&'0'),
        }
    }
}

impl PartialOrd for NameToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn name_tokens(text: &str) -> Vec<NameToken> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        if first.is_ascii_digit() {
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let (digits, tail) = rest.split_at(end);
            tokens.push(NameToken::Number(digits.trim_start_matches('0').into()));
            rest = tail;
        } else {
            let mut lowercase = ['\0'; 3];
            for (slot, character) in lowercase.iter_mut().zip(first.to_lowercase()) {
                *slot = character;
            }
            tokens.push(NameToken::Text(lowercase));
            rest = &rest[first.len_utf8()..];
        }
    }
    tokens
}
