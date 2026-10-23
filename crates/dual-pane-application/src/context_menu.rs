use crate::{ActionId, ContextMenuGroup, ContextMenuKind, OpenWithMenuAction, Shortcut};
use dual_pane_domain::EntryName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderItemsMenuTarget {
    Row { row: usize, name: EntryName },
    EmptySpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FolderItemsMenuToken(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderItemsMenuAction {
    Static { action: ActionId, shortcut: Option<Shortcut> },
    OpenWith(OpenWithMenuAction),
}

impl FolderItemsMenuAction {
    pub fn static_action(&self) -> Option<ActionId> {
        match self {
            Self::Static { action, .. } => Some(*action),
            Self::OpenWith(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderItemsMenuSection {
    pub group: ContextMenuGroup,
    pub actions: Vec<FolderItemsMenuAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderItemsMenuSnapshot {
    pub token: FolderItemsMenuToken,
    pub kind: ContextMenuKind,
    pub sections: Vec<FolderItemsMenuSection>,
}

#[cfg(test)]
mod tests {
    use crate::{ActionId, ContextMenuKind, ContextMenuPreferenceRejection, ContextMenuPreferences};

    #[test]
    fn factory_preferences_are_complete_and_independent() {
        let mut preferences = ContextMenuPreferences::default();
        assert!(preferences.visible(ContextMenuKind::Normal, ActionId::OpenItem));
        assert!(!preferences.visible(ContextMenuKind::Normal, ActionId::RenameItem));
        assert!(preferences.visible(ContextMenuKind::Comprehensive, ActionId::RenameItem));
        assert!(preferences.set_visible(ContextMenuKind::Normal, ActionId::RenameItem, true).unwrap());
        assert!(preferences.visible(ContextMenuKind::Normal, ActionId::RenameItem));
        assert!(preferences.visible(ContextMenuKind::Comprehensive, ActionId::RenameItem));
    }

    #[test]
    fn final_visible_action_is_rejected_without_mutation() {
        let mut preferences = ContextMenuPreferences::default();
        for action in crate::CONTEXT_MENU_ACTIONS {
            if action != ActionId::OpenItem && preferences.visible(ContextMenuKind::Normal, action) {
                preferences.set_visible(ContextMenuKind::Normal, action, false).unwrap();
            }
        }
        assert_eq!(preferences.set_visible(ContextMenuKind::Normal, ActionId::OpenItem, false), Err(ContextMenuPreferenceRejection::LastVisibleAction(ContextMenuKind::Normal)));
        assert!(preferences.visible(ContextMenuKind::Normal, ActionId::OpenItem));
    }

    #[test]
    fn only_exact_shift_selects_the_comprehensive_variant() {
        assert_eq!(ContextMenuKind::for_modifiers(true, false, false, false), ContextMenuKind::Comprehensive);
        for modifiers in [(false, false, false, false), (true, true, false, false), (true, false, true, false), (true, false, false, true), (false, true, false, false)] {
            assert_eq!(ContextMenuKind::for_modifiers(modifiers.0, modifiers.1, modifiers.2, modifiers.3), ContextMenuKind::Normal);
        }
    }
}
