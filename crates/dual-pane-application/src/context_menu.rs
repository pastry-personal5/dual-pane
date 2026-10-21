use crate::{ActionId, ContextMenuGroup, ContextMenuKind, Shortcut};
use dual_pane_domain::EntryName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderItemsMenuTarget {
    Row { row: usize, name: EntryName },
    EmptySpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FolderItemsMenuToken(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderItemsMenuAction {
    pub action: ActionId,
    pub shortcut: Option<Shortcut>,
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
}
