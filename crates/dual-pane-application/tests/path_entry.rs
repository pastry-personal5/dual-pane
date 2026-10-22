use dual_pane_application::{PathRejection, parse_path};
use dual_pane_domain::{EntryName, Location};

fn loc(parts: &[&str]) -> Location {
    Location::from_components(parts.iter().map(|part| EntryName::new(*part).unwrap()))
}

#[test]
fn absolute_paths_ignore_the_base() {
    assert_eq!(parse_path("/a/b", Some(&loc(&["x"])), &loc(&["h"])), Ok(loc(&["a", "b"])));
    assert_eq!(parse_path("/", None, &loc(&["h"])), Ok(Location::root()));
}

#[test]
fn repeated_trailing_slashes_and_dots_are_dropped() {
    assert_eq!(parse_path("//a///b/./c//", None, &Location::root()), Ok(loc(&["a", "b", "c"])));
}

#[test]
fn home_relative_uses_the_workspace_home() {
    let home = loc(&["Users", "me"]);
    assert_eq!(parse_path("~", None, &home), Ok(home.clone()));
    assert_eq!(parse_path("~/docs", None, &home), Ok(loc(&["Users", "me", "docs"])));
}

#[test]
fn tilde_name_is_an_ordinary_relative_name() {
    assert_eq!(parse_path("~other", Some(&loc(&["a"])), &loc(&["h"])), Ok(loc(&["a", "~other"])));
}

#[test]
fn relative_paths_resolve_against_the_base() {
    assert_eq!(parse_path("b/c", Some(&loc(&["a"])), &Location::root()), Ok(loc(&["a", "b", "c"])));
    assert_eq!(parse_path("..", Some(&loc(&["a", "b"])), &Location::root()), Ok(loc(&["a"])));
}

#[test]
fn relative_path_without_a_base_is_refused() {
    assert_eq!(parse_path("b", None, &Location::root()), Err(PathRejection::NoConfirmedFolder));
    assert_eq!(parse_path("/b", None, &Location::root()), Ok(loc(&["b"])));
}

#[test]
fn dot_dot_is_lexical_and_stops_at_root() {
    assert_eq!(parse_path("/a/missing/..", None, &Location::root()), Ok(loc(&["a"])));
    assert_eq!(parse_path("/../..", None, &Location::root()), Ok(Location::root()));
    assert_eq!(parse_path("/a/link/../b", None, &Location::root()), Ok(loc(&["a", "b"])));
}

#[test]
fn whitespace_is_a_literal_name() {
    assert_eq!(parse_path(" ", Some(&loc(&["a"])), &Location::root()), Ok(loc(&["a", " "])));
    assert_eq!(parse_path("/a/ b ", None, &Location::root()), Ok(loc(&["a", " b "])));
}

#[test]
fn control_characters_and_empty_text_are_malformed() {
    for text in ["", "/a\nb", "/a\tb", "/a\0b", "/a\u{7f}"] {
        assert_eq!(parse_path(text, None, &Location::root()), Err(PathRejection::Malformed), "{text:?}");
    }
}

#[test]
fn replacement_character_is_refused_not_replaced() {
    assert_eq!(parse_path("/a\u{FFFD}", None, &Location::root()), Err(PathRejection::UndecodableText));
}

#[test]
fn relative_path_keeps_the_base_bytes() {
    let odd = Location::from_components([EntryName::new(b"caf\xFF".to_vec()).unwrap()]);
    let parsed = parse_path("x", Some(&odd), &Location::root()).unwrap();
    assert_eq!(parsed.components()[0].as_bytes(), b"caf\xFF");
    assert_eq!(parsed.components()[1].as_bytes(), b"x");
}
