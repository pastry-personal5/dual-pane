use std::cmp::Ordering;

use unicode_normalization::UnicodeNormalization;

use crate::EntryName;

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
}

impl Entry {
    pub fn new(name: EntryName, kind: EntryKind) -> Self {
        Self { name, kind }
    }

    pub fn name(&self) -> &EntryName {
        &self.name
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
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

/// A unit of natural ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NameToken {
    /// A run of ASCII digits with leading zeros removed, so the numeric value
    /// is compared by length and then digit by digit, without overflow.
    Number(Box<str>),
    /// The lowercase form of one character that is not an ASCII digit.
    Text(Box<str>),
}

impl Ord for NameToken {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(x), Self::Number(y)) => x.len().cmp(&y.len()).then_with(|| x.cmp(y)),
            (Self::Text(x), Self::Text(y)) => x.cmp(y),
            // Text never starts with an ASCII digit, so "0" stands for every
            // number and a number never equals text.
            (Self::Number(_), Self::Text(y)) => "0".cmp(y),
            (Self::Text(x), Self::Number(_)) => (**x).cmp("0"),
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
            tokens.push(NameToken::Text(first.to_lowercase().collect::<String>().into()));
            rest = &rest[first.len_utf8()..];
        }
    }
    tokens
}
