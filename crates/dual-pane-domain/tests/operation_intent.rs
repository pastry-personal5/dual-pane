use dual_pane_domain::{DecisionToken, EntryKind, EntryName, Location, OperationChoice, OperationId, OperationIntent, OperationIssue, OperationKind, OperationRejection, OperationTarget};

fn name(text: &str) -> EntryName {
    EntryName::new(text).unwrap()
}
fn path(parts: &[&str]) -> Location {
    parts.iter().fold(Location::root(), |location, part| location.join(&name(part)))
}
fn target(text: &str, kind: EntryKind) -> OperationTarget {
    OperationTarget { name: name(text), kind }
}

#[test]
fn copy_and_move_need_a_different_destination_outside_directory_targets() {
    let dir = vec![target("dir", EntryKind::Directory)];
    assert_eq!(OperationIntent::new(OperationKind::Copy, path(&["a"]), dir.clone(), None).unwrap_err(), OperationRejection::DestinationRequired);
    assert_eq!(OperationIntent::new(OperationKind::Move, path(&["a"]), dir.clone(), Some(path(&["a"]))).unwrap_err(), OperationRejection::SameSourceAndDestination);
    assert_eq!(OperationIntent::new(OperationKind::Copy, path(&["a"]), dir.clone(), Some(path(&["a", "dir"]))).unwrap_err(), OperationRejection::DestinationWithinSource);
    assert_eq!(OperationIntent::new(OperationKind::Move, path(&["a"]), dir.clone(), Some(path(&["a", "dir", "deep"]))).unwrap_err(), OperationRejection::DestinationWithinSource);
    assert!(OperationIntent::new(OperationKind::Copy, path(&["a"]), dir, Some(path(&["a", "dirty"]))).is_ok(), "a sibling sharing a name prefix is not inside the folder");
    let link = vec![target("link", EntryKind::Symlink { points_to_directory: true })];
    assert!(OperationIntent::new(OperationKind::Move, path(&["a"]), link, Some(path(&["a", "link"]))).is_ok(), "a link is never traversed");
}

#[test]
fn other_kinds_reject_destinations_duplicates_and_empty_targets() {
    let file = vec![target("f", EntryKind::File)];
    assert_eq!(OperationIntent::new(OperationKind::MoveToTrash, path(&["a"]), file.clone(), Some(path(&["b"]))).unwrap_err(), OperationRejection::DestinationNotAllowed);
    assert_eq!(OperationIntent::new(OperationKind::DeletePermanently, path(&["a"]), vec![], None).unwrap_err(), OperationRejection::NoTargets);
    assert_eq!(OperationIntent::new(OperationKind::MoveToTrash, path(&["a"]), vec![file[0].clone(), file[0].clone()], None).unwrap_err(), OperationRejection::DuplicateTarget);
    assert_eq!(OperationIntent::new(OperationKind::NewFolder { name: name("n") }, path(&["a"]), file, None).unwrap_err(), OperationRejection::InvalidTargetCount);
    assert!(OperationIntent::new(OperationKind::NewFolder { name: name("n") }, path(&["a"]), vec![], None).is_ok());
}

#[test]
fn decision_policy_limits_choices_and_apply_to_all() {
    use OperationChoice::{Cancel, Replace, Skip, TryAgain};
    use OperationIssue::{FileConflict, KindMismatch, LinkCollision, RecoverableError};
    for choice in [Skip, Replace, Cancel] {
        assert!(FileConflict.permits(choice, false));
    }
    assert!(!FileConflict.permits(TryAgain, false));
    assert!(FileConflict.permits(Replace, true) && FileConflict.permits(Skip, true));
    assert!(!FileConflict.permits(Cancel, true), "cancel is never remembered");
    for issue in [LinkCollision, KindMismatch, RecoverableError] {
        assert!(issue.permits(TryAgain, false) && issue.permits(Skip, false) && issue.permits(Cancel, false));
        assert!(!issue.permits(Replace, false), "only a regular-file conflict may be replaced");
        assert!(!issue.permits(Skip, true) && !issue.permits(TryAgain, true));
    }
}

#[test]
fn identifiers_compare_by_value() {
    assert_eq!(OperationId::new(1), OperationId::new(1));
    assert_ne!(DecisionToken::new(1), DecisionToken::new(2));
}
