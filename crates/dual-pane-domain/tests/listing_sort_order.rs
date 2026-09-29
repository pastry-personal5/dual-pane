use std::cmp::Ordering;

use dual_pane_domain::{Entry, EntryKind, EntryName, listing_order};
use proptest::prelude::*;

fn entry(name: &[u8], kind: EntryKind) -> Entry {
    Entry::new(EntryName::new(name.to_vec()).unwrap(), kind)
}

fn file(name: &str) -> Entry {
    entry(name.as_bytes(), EntryKind::File)
}

fn sorted(mut entries: Vec<Entry>) -> Vec<String> {
    entries.sort_by(listing_order);
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
    assert_eq!(listing_order(&file("a"), &file("A")), b"a".as_slice().cmp(b"A".as_slice()));
    assert_eq!(listing_order(&file("7"), &file("007")), b"7".as_slice().cmp(b"007".as_slice()));
    assert_ne!(listing_order(&entry(b"a\xFF", EntryKind::File), &entry(b"a\xFE", EntryKind::File)), Ordering::Equal);
}

fn arbitrary_entry() -> impl Strategy<Value = Entry> {
    let name = proptest::collection::vec(prop_oneof![Just(b'a'), Just(b'B'), Just(b'0'), Just(b'1'), Just(b'9'), Just(b'-'), Just(0xC3), Just(0x89), Just(0xFF)], 1..6).prop_filter_map("valid entry name", |bytes| EntryName::new(bytes).ok());
    let kind = prop_oneof![Just(EntryKind::Directory), Just(EntryKind::File), Just(EntryKind::Other), any::<bool>().prop_map(|points_to_directory| EntryKind::Symlink { points_to_directory })];
    (name, kind).prop_map(|(name, kind)| Entry::new(name, kind))
}

proptest! {
    #[test]
    fn order_is_reflexive_and_antisymmetric(a in arbitrary_entry(), b in arbitrary_entry()) {
        prop_assert_eq!(listing_order(&a, &a), Ordering::Equal);
        prop_assert_eq!(listing_order(&a, &b), listing_order(&b, &a).reverse());
        if listing_order(&a, &b) == Ordering::Equal {
            prop_assert_eq!(a.name(), b.name());
        }
    }

    #[test]
    fn order_is_transitive(a in arbitrary_entry(), b in arbitrary_entry(), c in arbitrary_entry()) {
        if listing_order(&a, &b) != Ordering::Greater && listing_order(&b, &c) != Ordering::Greater {
            prop_assert_ne!(listing_order(&a, &c), Ordering::Greater);
        }
    }
}
