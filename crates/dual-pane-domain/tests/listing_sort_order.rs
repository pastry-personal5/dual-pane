use std::cmp::Ordering;

use dual_pane_domain::{Entry, EntryKind, EntryMetadata, EntryName, SortDirection, SortField, SortSpec, listing_sort_key, sort_entries};
use proptest::prelude::*;

fn entry(name: &[u8], kind: EntryKind) -> Entry {
    Entry::new(EntryName::new(name.to_vec()).unwrap(), kind)
}

fn file(name: &str) -> Entry {
    entry(name.as_bytes(), EntryKind::File)
}

fn order(a: &Entry, b: &Entry) -> Ordering {
    listing_sort_key(a).cmp(&listing_sort_key(b))
}

fn sorted(mut entries: Vec<Entry>) -> Vec<String> {
    entries.sort_by_cached_key(listing_sort_key);
    entries.iter().map(|e| e.name().to_text_lossy().into_owned()).collect()
}

#[test]
fn folders_and_links_to_folders_come_first() {
    let entries = vec![file("a"), entry(b"z-dir", EntryKind::Directory), entry(b"b-link", EntryKind::Symlink { points_to_directory: false }), entry(b"m-link", EntryKind::Symlink { points_to_directory: true }), entry(b"c-other", EntryKind::Other)];
    assert_eq!(sorted(entries), ["m-link", "z-dir", "a", "b-link", "c-other"]);
}

#[test]
fn names_compare_case_insensitively() {
    assert_eq!(sorted(vec![file("beta"), file("Alpha"), file("alpha2"), file("ALPHA1")]), ["Alpha", "ALPHA1", "alpha2", "beta"]);
}

#[test]
fn digit_runs_compare_by_numeric_value() {
    assert_eq!(sorted(vec![file("file10"), file("file2"), file("file1"), file("file02b")]), ["file1", "file2", "file02b", "file10"]);
}

#[test]
fn numbers_longer_than_any_integer_do_not_overflow() {
    let big = "123456789012345678901234567890";
    let bigger = "123456789012345678901234567891";
    assert_eq!(sorted(vec![file(bigger), file(big)]), [big, bigger]);
}

#[test]
fn exact_name_breaks_ties() {
    assert_eq!(order(&file("a"), &file("A")), b"a".as_slice().cmp(b"A".as_slice()));
    assert_eq!(order(&file("7"), &file("007")), b"7".as_slice().cmp(b"007".as_slice()));
    assert_ne!(order(&entry(b"a\xFF", EntryKind::File), &entry(b"a\xFE", EntryKind::File)), Ordering::Equal);
}

#[test]
fn composed_and_decomposed_accents_sort_together() {
    // "é" written as one character (NFC) and as "e" plus a combining accent (NFD).
    // Characters compare by code point, so "é" sorts after "z", not beside "e".
    let entries = vec![file("e\u{301}z"), file("\u{e9}b"), file("e\u{301}a"), file("z")];
    assert_eq!(sorted(entries), ["z", "e\u{301}a", "\u{e9}b", "e\u{301}z"]);
}

#[test]
fn names_that_differ_only_in_composition_stay_distinct() {
    assert_ne!(order(&file("\u{e9}"), &file("e\u{301}")), Ordering::Equal);
}

#[test]
fn every_sort_mode_keeps_folders_first_and_unknown_metadata_last() {
    let folder = Entry::with_metadata(EntryName::new("folder").unwrap(), EntryKind::Directory, EntryMetadata::new(Some(4), Some(4)));
    let small = Entry::with_metadata(EntryName::new("small").unwrap(), EntryKind::File, EntryMetadata::new(Some(2), Some(2)));
    let large = Entry::with_metadata(EntryName::new("large").unwrap(), EntryKind::File, EntryMetadata::new(Some(3), Some(3)));
    let unknown = file("unknown");
    for spec in [SortSpec::new(SortField::Name, SortDirection::Ascending), SortSpec::new(SortField::Name, SortDirection::Descending), SortSpec::new(SortField::Type, SortDirection::Ascending), SortSpec::new(SortField::Type, SortDirection::Descending), SortSpec::new(SortField::Modified, SortDirection::Ascending), SortSpec::new(SortField::Modified, SortDirection::Descending), SortSpec::new(SortField::Size, SortDirection::Ascending), SortSpec::new(SortField::Size, SortDirection::Descending)] {
        let mut entries = vec![unknown.clone(), large.clone(), folder.clone(), small.clone()];
        sort_entries(&mut entries, spec);
        assert_eq!(entries[0].name(), folder.name());
        if matches!(spec.field(), SortField::Modified | SortField::Size) {
            assert_eq!(entries.last().unwrap().name(), unknown.name());
        }
    }
}

#[test]
fn non_name_ties_use_name_ascending_in_both_directions() {
    let a = Entry::with_metadata(EntryName::new("a").unwrap(), EntryKind::File, EntryMetadata::new(Some(1), Some(1)));
    let b = Entry::with_metadata(EntryName::new("b").unwrap(), EntryKind::File, EntryMetadata::new(Some(1), Some(1)));
    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        let mut entries = vec![b.clone(), a.clone()];
        sort_entries(&mut entries, SortSpec::new(SortField::Size, direction));
        assert_eq!(entries.iter().map(|entry| entry.name()).collect::<Vec<_>>(), vec![a.name(), b.name()]);
    }
}

fn arbitrary_entry() -> impl Strategy<Value = Entry> {
    let name = proptest::collection::vec(prop_oneof![Just(b'a'), Just(b'B'), Just(b'0'), Just(b'1'), Just(b'9'), Just(b'-'), Just(0xC3), Just(0x89), Just(0xA9), Just(b'e'), Just(0xCC), Just(0x81), Just(0xFF)], 1..6).prop_filter_map("valid entry name", |bytes| EntryName::new(bytes).ok());
    let kind = prop_oneof![Just(EntryKind::Directory), Just(EntryKind::File), Just(EntryKind::Other), any::<bool>().prop_map(|points_to_directory| EntryKind::Symlink { points_to_directory })];
    (name, kind).prop_map(|(name, kind)| Entry::new(name, kind))
}

proptest! {
    #[test]
    fn order_is_reflexive_and_antisymmetric(a in arbitrary_entry(), b in arbitrary_entry()) {
        prop_assert_eq!(order(&a, &a), Ordering::Equal);
        prop_assert_eq!(order(&a, &b), order(&b, &a).reverse());
        if order(&a, &b) == Ordering::Equal {
            prop_assert_eq!(a.name(), b.name());
        }
    }

    #[test]
    fn order_is_transitive(a in arbitrary_entry(), b in arbitrary_entry(), c in arbitrary_entry()) {
        if order(&a, &b) != Ordering::Greater && order(&b, &c) != Ordering::Greater {
            prop_assert_ne!(order(&a, &c), Ordering::Greater);
        }
    }
}
