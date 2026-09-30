use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

use dual_pane_domain::{EntryName, Location};

/// The location of an absolute `path`, with each component's exact bytes, or
/// `None` for a relative path or one containing `.` or `..`.
pub fn location_from_path(path: &Path) -> Option<Location> {
    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return None;
    }
    components
        .map(|component| match component {
            Component::Normal(name) => EntryName::new(name.as_bytes()).ok(),
            Component::Prefix(_) | Component::RootDir | Component::CurDir | Component::ParentDir => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(Location::from_components)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    fn name(bytes: &[u8]) -> EntryName {
        EntryName::new(bytes.to_vec()).unwrap()
    }

    #[test]
    fn converts_an_absolute_path_component_by_component() {
        let location = location_from_path(Path::new("/alpha/beta gamma/.hidden")).unwrap();
        assert_eq!(location, Location::from_components([name(b"alpha"), name(b"beta gamma"), name(b".hidden")]));
    }

    #[test]
    fn converts_the_root() {
        assert_eq!(location_from_path(Path::new("/")), Some(Location::root()));
    }

    #[test]
    fn keeps_names_that_are_not_valid_utf8_byte_exact() {
        let path = Path::new(OsStr::from_bytes(b"/alpha/caf\xFF"));
        let location = location_from_path(path).unwrap();
        assert_eq!(location.components()[1].as_bytes(), b"caf\xFF");
    }

    #[test]
    fn rejects_relative_paths() {
        assert_eq!(location_from_path(Path::new("alpha/beta")), None);
        assert_eq!(location_from_path(Path::new("./alpha")), None);
        assert_eq!(location_from_path(Path::new("")), None);
    }

    #[test]
    fn rejects_paths_containing_parent_components() {
        assert_eq!(location_from_path(Path::new("/alpha/../beta")), None);
        assert_eq!(location_from_path(Path::new("/..")), None);
    }
}
