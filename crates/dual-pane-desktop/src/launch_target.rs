//! The folder or file named on the command line, resolved before Qt starts.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use dual_pane_domain::{EntryName, Location};

use crate::native_location::location_from_path;

pub const USAGE: &str = "Usage: dual-pane [--new-instance] [FOLDER | FILE]\n\nOpens FOLDER in Dual Pane, or the folder containing FILE with FILE selected.\nWithout --new-instance, a running instance for the same profile is reused.";

/// A folder for the application-selected Browser and, for a file argument,
/// the Item to select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTarget {
    pub folder: Location,
    pub select: Option<EntryName>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    Run { target: Option<LaunchTarget>, new_instance: bool },
    Help,
}

/// Interprets the arguments after the program name. Relative paths resolve
/// against `cwd` and `~` against `home`. Any error is a message for stderr.
pub fn parse(args: impl IntoIterator<Item = OsString>, cwd: &Path, home: Option<&Path>) -> Result<Launch, String> {
    let mut paths = Vec::new();
    let mut options_ended = false;
    let mut new_instance = false;
    for arg in args {
        match arg.to_str() {
            Some("--help" | "-h") if !options_ended => return Ok(Launch::Help),
            Some("--new-instance") if !options_ended && !new_instance => new_instance = true,
            Some("--new-instance") if !options_ended => return Err(format!("option --new-instance was given more than once\n{USAGE}")),
            Some("--") if !options_ended => options_ended = true,
            _ if !options_ended && arg.as_bytes().first() == Some(&b'-') => return Err(format!("unknown option {}\n{USAGE}", arg.to_string_lossy())),
            _ => paths.push(arg),
        }
    }
    match paths.as_slice() {
        [] => Ok(Launch::Run { target: None, new_instance }),
        [path] => resolve(path, cwd, home).map(|target| Launch::Run { target: Some(target), new_instance }),
        _ => Err(format!("expected at most one path, got {}\n{USAGE}", paths.len())),
    }
}

fn resolve(arg: &OsStr, cwd: &Path, home: Option<&Path>) -> Result<LaunchTarget, String> {
    let fail = |reason: &dyn std::fmt::Display| format!("cannot open {}: {reason}", arg.to_string_lossy());
    let path = absolute(arg, cwd, home).ok_or_else(|| fail(&"HOME is not set"))?;
    let metadata = fs::metadata(&path).map_err(|error| fail(&error))?;
    let (folder, select) = if metadata.is_dir() {
        (fs::canonicalize(&path).map_err(|error| fail(&error))?, None)
    } else {
        // The file keeps its own name, so a link is selected as the link.
        let name = path.file_name().ok_or_else(|| fail(&"not a file or folder"))?;
        let parent = path.parent().ok_or_else(|| fail(&"not a file or folder"))?;
        (fs::canonicalize(parent).map_err(|error| fail(&error))?, Some(EntryName::new(name.as_bytes()).map_err(|_| fail(&"invalid file name"))?))
    };
    fs::read_dir(&folder).map_err(|error| fail(&error))?;
    let folder = location_from_path(&folder).ok_or_else(|| fail(&"unsupported path"))?;
    Ok(LaunchTarget { folder, select })
}

fn absolute(arg: &OsStr, cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let bytes = arg.as_bytes();
    if bytes == b"~" {
        return home.map(Path::to_path_buf);
    }
    if let Some(rest) = bytes.strip_prefix(b"~/") {
        return home.map(|home| home.join(OsStr::from_bytes(rest)));
    }
    Some(cwd.join(arg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    fn name(value: &str) -> EntryName {
        EntryName::new(value).unwrap()
    }
    fn canonical_location(path: &Path) -> Location {
        location_from_path(&fs::canonicalize(path).unwrap()).unwrap()
    }

    #[test]
    fn no_arguments_run_normally_and_help_prints_usage() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(parse(args(&[]), dir.path(), None), Ok(Launch::Run { target: None, new_instance: false }));
        assert_eq!(parse(args(&["--help"]), dir.path(), None), Ok(Launch::Help));
        assert_eq!(parse(args(&["-h", "ignored"]), dir.path(), None), Ok(Launch::Help));
    }

    #[test]
    fn a_folder_resolves_relative_to_the_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        let parsed = parse(args(&["a/../a/b"]), dir.path(), None).unwrap();
        assert_eq!(parsed, Launch::Run { target: Some(LaunchTarget { folder: canonical_location(&dir.path().join("a/b")), select: None }), new_instance: false });
    }

    #[test]
    fn a_file_opens_its_folder_and_selects_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("note.txt"), b"x").unwrap();
        let parsed = parse(args(&["./note.txt"]), dir.path(), None).unwrap();
        assert_eq!(parsed, Launch::Run { target: Some(LaunchTarget { folder: canonical_location(dir.path()), select: Some(name("note.txt")) }), new_instance: false });
    }

    #[test]
    fn a_link_to_a_file_is_selected_as_the_link() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("real"), b"x").unwrap();
        std::os::unix::fs::symlink("real", dir.path().join("link")).unwrap();
        let Launch::Run { target: Some(target), .. } = parse(args(&["link"]), dir.path(), None).unwrap() else { panic!("target") };
        assert_eq!(target.select, Some(name("link")));
    }

    #[test]
    fn tilde_expands_to_the_given_home() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join("docs")).unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let expected = |sub: &str| Launch::Run { target: Some(LaunchTarget { folder: canonical_location(&home.path().join(sub)), select: None }), new_instance: false };
        assert_eq!(parse(args(&["~"]), cwd.path(), Some(home.path())), Ok(expected("")));
        assert_eq!(parse(args(&["~/docs"]), cwd.path(), Some(home.path())), Ok(expected("docs")));
        assert!(parse(args(&["~"]), cwd.path(), None).unwrap_err().contains("HOME"));
    }

    #[test]
    fn a_missing_path_is_an_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let error = parse(args(&["nope"]), dir.path(), None).unwrap_err();
        assert!(error.starts_with("cannot open nope:"), "{error}");
    }

    #[test]
    fn several_paths_and_unknown_options_are_usage_errors() {
        let dir = tempfile::tempdir().unwrap();
        assert!(parse(args(&["a", "b"]), dir.path(), None).unwrap_err().contains("at most one path"));
        assert!(parse(args(&["--bogus"]), dir.path(), None).unwrap_err().contains("unknown option"));
    }

    #[test]
    fn a_dash_prefixed_name_opens_after_the_option_terminator() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("-odd")).unwrap();
        assert!(matches!(parse(args(&["--", "-odd"]), dir.path(), None), Ok(Launch::Run { target: Some(_), .. })));
    }

    #[test]
    fn new_instance_is_an_option_and_may_precede_a_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("folder")).unwrap();
        assert!(matches!(parse(args(&["--new-instance"]), dir.path(), None), Ok(Launch::Run { target: None, new_instance: true })));
        assert!(matches!(parse(args(&["--new-instance", "folder"]), dir.path(), None), Ok(Launch::Run { target: Some(_), new_instance: true })));
        assert!(parse(args(&["--new-instance", "--new-instance"]), dir.path(), None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_folder_is_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = parse(args(&["locked"]), dir.path(), None);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        // Root can read anything, so only a refusal is asserted when it occurs.
        if fs::read_dir(&locked).is_ok() && result.is_ok() {
            return;
        }
        assert!(result.unwrap_err().starts_with("cannot open locked:"));
    }
}
