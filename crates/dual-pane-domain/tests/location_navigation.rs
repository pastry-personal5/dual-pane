use dual_pane_domain::{EntryName, Location};
use proptest::prelude::*;

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}

#[test]
fn root_has_no_parent() {
    assert!(Location::root().is_root());
    assert_eq!(Location::root().parent(), None);
}

#[test]
fn join_appends_one_component() {
    let location = Location::root().join(&name("Users")).join(&name("docs"));
    assert_eq!(location.components(), [name("Users"), name("docs")]);
}

#[test]
fn parent_removes_the_last_component() {
    let location = Location::from_components([name("a"), name("b")]);
    assert_eq!(location.parent(), Some(Location::from_components([name("a")])));
    assert_eq!(location.parent().and_then(|p| p.parent()), Some(Location::root()));
}

#[test]
fn parent_after_a_symbolic_link_is_logical() {
    // `link` may point anywhere; its parent is still the location containing it.
    let through_link = Location::from_components([name("a")]).join(&name("link"));
    assert_eq!(through_link.parent(), Some(Location::from_components([name("a")])));
}

fn entry_name() -> impl Strategy<Value = EntryName> {
    proptest::collection::vec(any::<u8>(), 1..8).prop_filter_map("valid entry name", |bytes| EntryName::new(bytes).ok())
}

proptest! {
    #[test]
    fn join_then_parent_returns_the_original(components in proptest::collection::vec(entry_name(), 0..6), child in entry_name()) {
        let location = Location::from_components(components);
        prop_assert_eq!(location.join(&child).parent(), Some(location));
    }
}
