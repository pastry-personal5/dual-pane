//! Shortcuts Settings invokables and key capture.

use super::*;

impl ffi::WorkspaceBridge {
    pub(super) fn binding_count(&self) -> i32 {
        count(self.rust().shown.bindings().len())
    }
    pub(super) fn binding_action(&self, binding: i32) -> QString {
        usize::try_from(binding).ok().and_then(|binding| self.rust().shown.bindings().get(binding)).map_or_else(QString::default, |binding| QString::from(binding.action.as_str()))
    }
    pub(super) fn binding_sequence(&self, binding: i32) -> QString {
        usize::try_from(binding).ok().and_then(|binding| self.rust().shown.bindings().get(binding)).and_then(|binding| binding.shortcut).map_or_else(QString::default, |shortcut| QString::from(key_sequence(shortcut).as_str()))
    }
    pub(super) fn binding_scope(&self, binding: i32) -> i32 {
        usize::try_from(binding).ok().and_then(|binding| self.rust().shown.bindings().get(binding)).map_or(-1, |binding| scope_code(binding.action.scope()))
    }
    pub(super) fn action_count(&self) -> i32 {
        count(ActionId::ALL.len())
    }
    pub(super) fn action_id(&self, index: i32) -> QString {
        action_at(index).map_or_else(QString::default, |action| QString::from(action.as_str()))
    }
    pub(super) fn action_scope(&self, index: i32) -> i32 {
        action_at(index).map_or(-1, |action| scope_code(action.scope()))
    }
    pub(super) fn action_category(&self, index: i32) -> i32 {
        action_at(index).map_or(-1, |action| action_spec(action).category as i32)
    }
    pub(super) fn action_shortcut_count(&self, index: i32) -> i32 {
        action_at(index).map_or(0, |action| count(self.rust().shown.bindings().iter().filter(|binding| binding.action == action && binding.shortcut.is_some()).count()))
    }
    pub(super) fn action_shortcut_label(&self, index: i32, position: i32) -> QString {
        let Some(action) = action_at(index) else { return QString::default() };
        usize::try_from(position).ok().and_then(|position| self.rust().shown.bindings().iter().filter(|binding| binding.action == action).filter_map(|binding| binding.shortcut).nth(position)).map_or_else(QString::default, |shortcut| QString::from(shortcut_label(shortcut).as_str()))
    }
    pub(super) fn action_default_shortcut_label(&self, index: i32) -> QString {
        action_at(index).map(default_shortcuts).map(|shortcuts| shortcuts.into_iter().map(shortcut_label).collect::<Vec<_>>().join("\n")).map_or_else(QString::default, |label| QString::from(label.as_str()))
    }
    pub(super) fn add_action_shortcut(mut self: Pin<&mut Self>, index: i32, key: i32, modifiers: i32) -> QString {
        let Some(action) = action_at(index) else { return QString::from("Unknown action") };
        let shortcut = captured_shortcut(key, modifiers);
        let result = with_session(|session| {
            let mut shortcuts = session.shortcuts(action);
            shortcuts.push(shortcut);
            session.submit_shortcuts(action, shortcuts)
        })
        .flatten();
        self.as_mut().session_changed();
        rejection_text(result)
    }
    pub(super) fn remove_action_shortcut(mut self: Pin<&mut Self>, index: i32, position: i32) -> QString {
        let Some(action) = action_at(index) else { return QString::from("Unknown action") };
        let result = with_session(|session| {
            let mut shortcuts = session.shortcuts(action);
            if let Ok(position) = usize::try_from(position)
                && position < shortcuts.len()
            {
                shortcuts.remove(position);
            }
            session.submit_shortcuts(action, shortcuts)
        })
        .flatten();
        self.as_mut().session_changed();
        rejection_text(result)
    }
    pub(super) fn restore_action_shortcuts(mut self: Pin<&mut Self>, index: i32) -> QString {
        let Some(action) = action_at(index) else { return QString::from("Unknown action") };
        let result = with_session(|session| session.restore_shortcuts(action)).flatten();
        self.as_mut().session_changed();
        rejection_text(result)
    }
    pub(super) fn set_bindings_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().bindings_revision != value {
            self.as_mut().rust_mut().get_mut().bindings_revision = value;
            self.bindings_revision_changed();
        }
    }
}

/// The Qt code for `scope`, matching the `scope_*` constants in
/// `desktop_window.cpp`.
pub(super) fn scope_code(scope: ShortcutScope) -> i32 {
    match scope {
        ShortcutScope::FolderItemsList => 0,
        ShortcutScope::Window => 1,
        ShortcutScope::Application => 2,
    }
}

pub(super) fn action_at(index: i32) -> Option<ActionId> {
    usize::try_from(index).ok().and_then(|index| ActionId::ALL.get(index).copied())
}

/// The shortcut a capture recorded. Its flags name Qt modifiers: 1 Meta, 2
/// Control, 4 Alt, 8 Shift. On macOS Qt's Control is Command and its Meta is
/// Control, the same swap [`key_sequence`] applies in reverse.
pub(super) fn captured_shortcut(key: i32, modifiers: i32) -> Shortcut {
    Shortcut::new(modifiers & 2 != 0, modifiers & 8 != 0, modifiers & 4 != 0, modifiers & 1 != 0, capture_key(key))
}

pub(super) fn capture_key(key: i32) -> Key {
    match key {
        0x01000000 => Key::Escape,
        0x01000001 | 0x01000002 => Key::Tab,
        0x01000003 => Key::Delete,
        0x01000004 | 0x01000005 => Key::Return,
        0x01000007 => Key::ForwardDelete,
        0x01000012 => Key::Left,
        0x01000013 => Key::Up,
        0x01000014 => Key::Right,
        0x01000015 => Key::Down,
        0x20 => Key::Space,
        0x01000030..=0x01000047 => Key::Function((key - 0x01000030 + 1) as u8),
        _ => {
            let character = char::from_u32(key as u32).unwrap_or('\0');
            let unshifted = match character {
                '!' => '1',
                '@' => '2',
                '#' => '3',
                '$' => '4',
                '%' => '5',
                '^' => '6',
                '&' => '7',
                '*' => '8',
                '(' => '9',
                ')' => '0',
                '_' => '-',
                '+' => '=',
                '{' => '[',
                '}' => ']',
                '|' => '\\',
                ':' => ';',
                '"' => '\'',
                '<' => ',',
                '>' => '.',
                '?' => '/',
                '~' => '`',
                other => other,
            };
            Key::Character(unshifted.to_ascii_uppercase())
        }
    }
}

pub(super) fn shortcut_label(shortcut: Shortcut) -> String {
    let mut result = String::new();
    for (held, label) in [(shortcut.control, "Control+"), (shortcut.option, "Option+"), (shortcut.shift, "Shift+"), (shortcut.command, "Command+")] {
        if held {
            result.push_str(label);
        }
    }
    let key_label = match shortcut.key {
        Key::Delete => "Backspace".to_owned(),
        Key::ForwardDelete => "Forward Delete".to_owned(),
        key => key.as_text(),
    };
    result.push_str(&key_label);
    result
}

/// Qt portable key-sequence text for `shortcut`. Qt swaps the keys on macOS:
/// `ControlModifier` (`Ctrl+`) is Command and `MetaModifier` (`Meta+`) is Control.
pub(crate) fn key_sequence(shortcut: Shortcut) -> String {
    let mut text = String::new();
    for (held, name) in [(shortcut.command, "Ctrl+"), (shortcut.option, "Alt+"), (shortcut.shift, "Shift+"), (shortcut.control, "Meta+")] {
        if held {
            text.push_str(name);
        }
    }
    match shortcut.key {
        Key::Character(key) => text.push(key),
        Key::Up => text.push_str("Up"),
        Key::Down => text.push_str("Down"),
        Key::Left => text.push_str("Left"),
        Key::Right => text.push_str("Right"),
        Key::Return => text.push_str("Return"),
        Key::Delete => text.push_str("Backspace"),
        Key::ForwardDelete => text.push_str("Delete"),
        Key::Space => text.push_str("Space"),
        Key::Tab => text.push_str("Tab"),
        Key::Escape => text.push_str("Esc"),
        Key::Function(number) => text.push_str(&format!("F{number}")),
    }
    text
}
