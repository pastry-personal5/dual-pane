use std::cmp::Ordering;

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

/// The order of entries in a listing.
///
/// Entries that can be entered come first. Names then compare in natural,
/// case-insensitive order: runs of ASCII digits compare by numeric value and
/// other characters compare by their Unicode lowercase form. The exact bytes
/// break every remaining tie, so the order is total.
pub fn listing_order(a: &Entry, b: &Entry) -> Ordering {
    b.can_enter().cmp(&a.can_enter()).then_with(|| natural_order(&a.name.to_text_lossy(), &b.name.to_text_lossy())).then_with(|| a.name.as_bytes().cmp(b.name.as_bytes()))
}

fn natural_order(a: &str, b: &str) -> Ordering {
    let mut a = Tokens { rest: a };
    let mut b = Tokens { rest: b };
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match x.cmp(&y) {
                Ordering::Equal => {}
                unequal => return unequal,
            },
        }
    }
}

/// A unit of natural ordering: a run of ASCII digits or one other character.
#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    /// Digits with leading zeros removed, so the numeric value is compared
    /// by length and then digit by digit, without overflow.
    Number(&'a str),
    Char(char),
}

impl Ord for Token<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Token::Number(x), Token::Number(y)) => x.len().cmp(&y.len()).then_with(|| x.cmp(y)),
            (Token::Char(x), Token::Char(y)) => x.to_lowercase().cmp(y.to_lowercase()),
            // A character token is never an ASCII digit, and its lowercase
            // form never starts with one, so '0' stands for every number.
            (Token::Number(_), Token::Char(y)) => std::iter::once('0').cmp(y.to_lowercase()),
            (Token::Char(x), Token::Number(_)) => x.to_lowercase().cmp(std::iter::once('0')),
        }
    }
}

impl PartialOrd for Token<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Tokens<'a> {
    rest: &'a str,
}

impl<'a> Iterator for Tokens<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let first = self.rest.chars().next()?;
        if first.is_ascii_digit() {
            let end = self.rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(self.rest.len());
            let (digits, rest) = self.rest.split_at(end);
            self.rest = rest;
            Some(Token::Number(digits.trim_start_matches('0')))
        } else {
            self.rest = &self.rest[first.len_utf8()..];
            Some(Token::Char(first))
        }
    }
}
