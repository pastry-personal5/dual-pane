use super::shortcuts::{capture_key, captured_shortcut, scope_code, shortcut_label};
use super::*;
use dual_pane_application::{ActionId, default_shortcut};

#[test]
fn captured_qt_modifiers_become_the_keys_a_person_held() {
    let key_c = i32::from(b'C');
    assert_eq!(captured_shortcut(key_c, 2), Shortcut::new(true, false, false, false, Key::Character('C')), "Qt Control is Command");
    assert_eq!(captured_shortcut(key_c, 1), Shortcut::new(false, false, false, true, Key::Character('C')), "Qt Meta is Control");
    assert_eq!(captured_shortcut(key_c, 4 | 8), Shortcut::new(false, true, true, false, Key::Character('C')));
    for shortcut in [captured_shortcut(key_c, 2), captured_shortcut(key_c, 1 | 2 | 4 | 8)] {
        assert_eq!(captured_shortcut(key_c, qt_flags(&key_sequence(shortcut))), shortcut, "capture and key sequence agree on {}", key_sequence(shortcut));
    }
}

/// The capture flags Qt reports for the modifiers in a portable sequence.
fn qt_flags(sequence: &str) -> i32 {
    [("Meta+", 1), ("Ctrl+", 2), ("Alt+", 4), ("Shift+", 8)].iter().filter(|(name, _)| sequence.contains(name)).map(|(_, flag)| flag).sum()
}

#[test]
fn delivered_shortcuts_map_to_qt_key_sequences() {
    let sequence = |action| default_shortcut(action).map(key_sequence);
    assert_eq!(sequence(ActionId::NewTab).as_deref(), Some("Ctrl+T"));
    assert_eq!(sequence(ActionId::CloseTab).as_deref(), Some("Ctrl+W"));
    assert_eq!(sequence(ActionId::NavigateBack).as_deref(), Some("Alt+Left"));
    assert_eq!(sequence(ActionId::NavigateForward).as_deref(), Some("Alt+Right"));
    assert_eq!(sequence(ActionId::RefreshFolder).as_deref(), Some("Ctrl+R"));
    assert_eq!(sequence(ActionId::FocusOtherBrowser).as_deref(), Some("Alt+F"));
    assert_eq!(sequence(ActionId::NavigateParent).as_deref(), Some("Ctrl+Up"));
    assert_eq!(sequence(ActionId::OpenItem).as_deref(), Some("Ctrl+Down"));
    assert_eq!(sequence(ActionId::QuitApplication).as_deref(), Some("Ctrl+Q"));
    assert_eq!(sequence(ActionId::NewFolder).as_deref(), Some("Ctrl+Shift+N"));
    assert_eq!(sequence(ActionId::CopyItems).as_deref(), Some("Ctrl+C"));
    assert_eq!(sequence(ActionId::CutItems).as_deref(), Some("Ctrl+X"));
    assert_eq!(sequence(ActionId::PasteItems).as_deref(), Some("Ctrl+V"));
    assert_eq!(sequence(ActionId::CloseWindow), None);
    assert_eq!(default_shortcuts(ActionId::MoveToTrash).into_iter().map(key_sequence).collect::<Vec<_>>(), ["Ctrl+Backspace", "Ctrl+Delete", "Shift+Backspace", "Shift+Delete"], "Qt's Delete is Forward Delete");
    assert_eq!(key_sequence(Shortcut::new(true, false, true, true, Key::Delete)), "Ctrl+Alt+Meta+Backspace");
    assert_eq!(key_sequence(Shortcut::new(false, false, false, false, Key::Function(2))), "F2");
}
#[test]
fn capture_normalizes_shifted_keys_and_writes_full_modifier_names() {
    assert_eq!(capture_key('!' as i32), Key::Character('1'));
    assert_eq!(capture_key('?' as i32), Key::Character('/'));
    assert_eq!(capture_key(0x01000047), Key::Function(24));
    let shortcut = Shortcut::new(true, true, true, true, Key::ForwardDelete);
    assert_eq!(shortcut_label(shortcut), "Control+Option+Shift+Command+Forward Delete");
}

#[test]
fn scope_codes_match_the_qt_constants() {
    assert_eq!(scope_code(ActionId::MoveToTrash.scope()), 0, "scope_folder_items_list");
    assert_eq!(scope_code(ActionId::NewFolder.scope()), 1, "scope_window");
    assert_eq!(scope_code(ActionId::QuitApplication.scope()), 2, "scope_application");
}

#[test]
fn quick_look_gesture_seam_covers_native_panel_boundaries() {
    assert!(ffi::quick_look_seam_tests());
}
