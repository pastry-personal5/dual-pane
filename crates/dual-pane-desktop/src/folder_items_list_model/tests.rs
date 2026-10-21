use super::operations::command_code;
use super::*;

#[test]
fn sort_choices_round_trip_every_direct_sort_button() {
    for choice in 0..8 {
        assert_eq!(sort_choice(sort_from_choice(choice)), choice);
    }
    assert_eq!(sort_choice(None), -1);
    assert_eq!(sort_from_choice(-1), None);
    assert_eq!(sort_from_choice(8), None);
    assert_eq!(sort_from_choice(5), Some(SortSpec::new(SortField::Modified, SortDirection::Descending)));
}

#[test]
fn file_command_codes_match_the_qt_constants() {
    // The `command_*` constants in desktop_window.cpp.
    let expected = [(0, OperationCommand::Copy), (1, OperationCommand::Move), (2, OperationCommand::Rename), (3, OperationCommand::NewFolder), (4, OperationCommand::MoveToTrash), (5, OperationCommand::DeletePermanently)];
    for (code, command) in expected {
        assert_eq!(operation_command(code), Some(command));
        assert_eq!(command_code(command), code);
    }
    assert_eq!(operation_command(-1), None);
    assert_eq!(operation_command(6), None);
    assert_eq!(operation_id(-1), None);
}
