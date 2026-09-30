use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dual_pane_domain::{Entry, EntryKind, EntryName, ListingErrorKind, Location, listing_sort_key};

/// How many entries are built between cancellation checks.
const CANCEL_CHECK_INTERVAL: usize = 1024;

/// A stand-in listing source that builds the same deterministic entries for
/// every location, so the delivery path can be exercised at scale.
#[derive(Debug, Clone, Copy)]
pub struct SyntheticListing {
    rows: usize,
}

impl SyntheticListing {
    pub fn new(rows: usize) -> Self {
        Self { rows }
    }

    /// `rows` entries in listing order, every tenth one a folder, or `None`
    /// once `cancelled` is set.
    pub fn read(&self, _location: &Location, cancelled: &AtomicBool) -> Option<Result<Arc<[Entry]>, ListingErrorKind>> {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let mut entries = Vec::with_capacity(self.rows);
        for index in 0..self.rows {
            if index.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancelled.load(Ordering::Relaxed) {
                return None;
            }
            entries.push(synthetic_entry(index));
        }
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        entries.sort_by_cached_key(listing_sort_key);
        Some(Ok(entries.into()))
    }
}

fn synthetic_entry(index: usize) -> Entry {
    let (text, kind) = if index.is_multiple_of(10) { (format!("Folder {index:05}"), EntryKind::Directory) } else { (format!("Item {index:05}.txt"), EntryKind::File) };
    match EntryName::new(text) {
        Ok(name) => Entry::new(name, kind),
        Err(error) => unreachable!("synthetic names are valid: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(rows: usize, cancelled: bool) -> Option<Result<Arc<[Entry]>, ListingErrorKind>> {
        SyntheticListing::new(rows).read(&Location::root(), &AtomicBool::new(cancelled))
    }

    #[test]
    fn returns_the_configured_number_of_entries_in_listing_order() {
        let entries = read(2_500, false).unwrap().unwrap();
        assert_eq!(entries.len(), 2_500);
        assert!(entries.windows(2).all(|pair| listing_sort_key(&pair[0]) < listing_sort_key(&pair[1])));
    }

    #[test]
    fn mixes_folders_first_and_then_files() {
        let entries = read(2_500, false).unwrap().unwrap();
        let folders = entries.iter().take_while(|entry| entry.kind() == EntryKind::Directory).count();
        assert_eq!(folders, 250);
        assert!(entries[folders..].iter().all(|entry| entry.kind() == EntryKind::File));
    }

    #[test]
    fn returns_nothing_when_cancelled() {
        assert_eq!(read(100_000, true), None);
    }

    #[test]
    fn stops_before_building_when_already_cancelled() {
        // Building this many entries would overflow the allocation, so only
        // an early stop can return.
        assert_eq!(read(usize::MAX, true), None);
    }
}
