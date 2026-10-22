//! The one catalogue of actions. Each row holds everything the rest of the
//! application, the Settings pages, and the Folder Items Context Menu need to
//! know about an action, so adding an action means adding a row here and, only
//! when it needs a new kind of availability rule or invocation, one case in the
//! small interpreters in [`crate::Workspace`].
//!
//! Rows follow the declaration order of [`ActionId`]; a compile-time check
//! enforces it.

use dual_pane_domain::{OperationCommand, OperationKind};

use crate::{ActionId, ClipboardAction, ContextMenuGroup, Key, Shortcut, ShortcutScope};

/// The Shortcuts page section an action is listed under, in page order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutCategory {
    Navigation,
    Window,
    FolderItems,
    Sorting,
}

/// What a Folder Items Context Menu action acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTarget {
    /// The selected Items. Needs a row target and stops being valid once the
    /// selection changes.
    Item,
    /// The Folder shown. Also valid from empty space.
    Folder,
}

/// When a Folder Items Context Menu action is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAvailability {
    /// The file command would be accepted now.
    Command(OperationCommand),
    ClipboardCapture,
    ClipboardPaste,
    /// Exactly one Item is selected and it has an activation.
    OpenableItem,
    /// Exactly one Item is selected and it is a package.
    PackageItem,
    /// Exactly one Item is selected.
    SingleItem,
}

/// The file operations a Folder Items Context Menu action can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOperation {
    Copy,
    Move,
    MoveToTrash,
    DeletePermanently,
}

impl MenuOperation {
    pub fn kind(self) -> OperationKind {
        match self {
            Self::Copy => OperationKind::Copy,
            Self::Move => OperationKind::Move,
            Self::MoveToTrash => OperationKind::MoveToTrash,
            Self::DeletePermanently => OperationKind::DeletePermanently,
        }
    }
}

/// What invoking a Folder Items Context Menu action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuInvocation {
    NameEditor(OperationCommand),
    Operation(MenuOperation),
    ClipboardCapture(ClipboardAction),
    ClipboardPaste,
    OpenItem,
    ShowPackageContents,
    RevealInFinder,
}

/// The Folder Items Context Menu facts of an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderItemSpec {
    pub group: ContextMenuGroup,
    /// Position in the fixed row order, counted across all groups.
    pub position: u8,
    /// Whether the Normal Context Menu shows the action by default. The
    /// Comprehensive Context Menu shows every action by default.
    pub normal_default: bool,
    pub target: MenuTarget,
    /// Whether the action needs the other Browser's folder as its destination.
    pub needs_other_browser: bool,
    pub availability: MenuAvailability,
    pub invocation: MenuInvocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    pub id: ActionId,
    /// The stable, persisted name.
    pub name: &'static str,
    pub scope: ShortcutScope,
    pub category: ShortcutCategory,
    /// The factory shortcuts. The first is the primary one.
    pub shortcuts: &'static [Shortcut],
    pub folder_item: Option<FolderItemSpec>,
}

const fn command(key: Key) -> Shortcut {
    Shortcut::new(true, false, false, false, key)
}
const fn option(key: Key) -> Shortcut {
    Shortcut::new(false, false, true, false, key)
}
const fn plain(key: Key) -> Shortcut {
    Shortcut::new(false, false, false, false, key)
}
const fn shift(key: Key) -> Shortcut {
    Shortcut::new(false, true, false, false, key)
}
const fn letter(character: char) -> Key {
    Key::Character(character)
}

const fn folder_item(group: ContextMenuGroup, position: u8, normal_default: bool, target: MenuTarget, availability: MenuAvailability, invocation: MenuInvocation) -> Option<FolderItemSpec> {
    Some(FolderItemSpec { group, position, normal_default, target, needs_other_browser: false, availability, invocation })
}

const fn other_browser(group: ContextMenuGroup, position: u8, command: OperationCommand, kind: MenuOperation) -> Option<FolderItemSpec> {
    Some(FolderItemSpec { group, position, normal_default: true, target: MenuTarget::Item, needs_other_browser: true, availability: MenuAvailability::Command(command), invocation: MenuInvocation::Operation(kind) })
}

const fn row(id: ActionId, name: &'static str, scope: ShortcutScope, category: ShortcutCategory, shortcuts: &'static [Shortcut], folder_item: Option<FolderItemSpec>) -> ActionSpec {
    ActionSpec { id, name, scope, category, shortcuts, folder_item }
}

use ContextMenuGroup as Group;
use ShortcutCategory as Category;
use ShortcutScope::{Application, FolderItemsList, Window};

const CATALOGUE: &[ActionSpec] = &[row(ActionId::FocusOtherBrowser, "FocusOtherBrowser", Window, Category::Navigation, &[option(letter('F'))], None), row(ActionId::NavigateParent, "NavigateParent", Window, Category::Navigation, &[command(Key::Up)], None), row(ActionId::CloseWindow, "CloseWindow", Window, Category::Window, &[], None), row(ActionId::QuitApplication, "QuitApplication", Application, Category::Window, &[command(letter('Q'))], None), row(ActionId::NewFolder, "NewFolder", Window, Category::FolderItems, &[Shortcut::new(true, true, false, false, letter('N'))], folder_item(Group::RenameAndCreate, 1, false, MenuTarget::Folder, MenuAvailability::Command(OperationCommand::NewFolder), MenuInvocation::NameEditor(OperationCommand::NewFolder))), row(ActionId::SortByNameAscending, "SortByNameAscending", Window, Category::Sorting, &[], None), row(ActionId::SortByNameDescending, "SortByNameDescending", Window, Category::Sorting, &[], None), row(ActionId::SortByTypeAscending, "SortByTypeAscending", Window, Category::Sorting, &[], None), row(ActionId::SortByTypeDescending, "SortByTypeDescending", Window, Category::Sorting, &[], None), row(ActionId::SortByDateAscending, "SortByDateAscending", Window, Category::Sorting, &[], None), row(ActionId::SortByDateDescending, "SortByDateDescending", Window, Category::Sorting, &[], None), row(ActionId::SortBySizeAscending, "SortBySizeAscending", Window, Category::Sorting, &[], None), row(ActionId::SortBySizeDescending, "SortBySizeDescending", Window, Category::Sorting, &[], None), row(ActionId::NewTab, "NewTab", Window, Category::Window, &[command(letter('T'))], None), row(ActionId::CloseTab, "CloseTab", Window, Category::Window, &[command(letter('W'))], None), row(ActionId::NavigateBack, "NavigateBack", Window, Category::Navigation, &[option(Key::Left)], None), row(ActionId::NavigateForward, "NavigateForward", Window, Category::Navigation, &[option(Key::Right)], None), row(ActionId::RefreshFolder, "RefreshFolder", Window, Category::Navigation, &[command(letter('R'))], None), row(ActionId::CopyToOtherBrowser, "CopyToOtherBrowser", FolderItemsList, Category::FolderItems, &[option(letter('C'))], other_browser(Group::OtherBrowser, 8, OperationCommand::Copy, MenuOperation::Copy)), row(ActionId::MoveToOtherBrowser, "MoveToOtherBrowser", FolderItemsList, Category::FolderItems, &[option(letter('M'))], other_browser(Group::OtherBrowser, 9, OperationCommand::Move, MenuOperation::Move)), row(ActionId::RenameItem, "RenameItem", FolderItemsList, Category::FolderItems, &[plain(Key::Function(2)), plain(Key::Return)], folder_item(Group::RenameAndCreate, 0, false, MenuTarget::Item, MenuAvailability::Command(OperationCommand::Rename), MenuInvocation::NameEditor(OperationCommand::Rename))), row(ActionId::MoveToTrash, "MoveToTrash", FolderItemsList, Category::FolderItems, &[command(Key::Delete), command(Key::ForwardDelete), shift(Key::Delete), shift(Key::ForwardDelete)], folder_item(Group::Delete, 10, true, MenuTarget::Item, MenuAvailability::Command(OperationCommand::MoveToTrash), MenuInvocation::Operation(MenuOperation::MoveToTrash))), row(ActionId::DeletePermanently, "DeletePermanently", FolderItemsList, Category::FolderItems, &[Shortcut::new(true, false, true, false, Key::Delete)], folder_item(Group::Delete, 11, false, MenuTarget::Item, MenuAvailability::Command(OperationCommand::DeletePermanently), MenuInvocation::Operation(MenuOperation::DeletePermanently))), row(ActionId::ShowPackageContents, "ShowPackageContents", FolderItemsList, Category::FolderItems, &[], folder_item(Group::Open, 3, false, MenuTarget::Item, MenuAvailability::PackageItem, MenuInvocation::ShowPackageContents)), row(ActionId::OpenItem, "OpenItem", FolderItemsList, Category::FolderItems, &[command(Key::Down)], folder_item(Group::Open, 2, true, MenuTarget::Item, MenuAvailability::OpenableItem, MenuInvocation::OpenItem)), row(ActionId::OpenInFinder, "OpenInFinder", FolderItemsList, Category::FolderItems, &[], folder_item(Group::Open, 4, false, MenuTarget::Item, MenuAvailability::SingleItem, MenuInvocation::RevealInFinder)), row(ActionId::FocusFolderItemsFilter, "FocusFolderItemsFilter", Window, Category::Navigation, &[command(letter('F'))], None), row(ActionId::CopyItems, "CopyItems", FolderItemsList, Category::FolderItems, &[command(letter('C'))], folder_item(Group::Clipboard, 5, true, MenuTarget::Item, MenuAvailability::ClipboardCapture, MenuInvocation::ClipboardCapture(ClipboardAction::Copy))), row(ActionId::CutItems, "CutItems", FolderItemsList, Category::FolderItems, &[command(letter('X'))], folder_item(Group::Clipboard, 6, true, MenuTarget::Item, MenuAvailability::ClipboardCapture, MenuInvocation::ClipboardCapture(ClipboardAction::Cut))), row(ActionId::PasteItems, "PasteItems", FolderItemsList, Category::FolderItems, &[command(letter('V'))], folder_item(Group::Clipboard, 7, true, MenuTarget::Folder, MenuAvailability::ClipboardPaste, MenuInvocation::ClipboardPaste)), row(ActionId::EditPath, "EditPath", Window, Category::Navigation, &[command(letter('L'))], None)];

/// How many actions are catalogued.
pub const ACTION_COUNT: usize = CATALOGUE.len();

static ACTIONS: &[ActionSpec] = CATALOGUE;

/// How many actions the Folder Items Context Menu can offer.
pub const FOLDER_ITEM_ACTION_COUNT: usize = {
    let mut count = 0;
    let mut index = 0;
    while index < CATALOGUE.len() {
        if CATALOGUE[index].folder_item.is_some() {
            count += 1;
        }
        index += 1;
    }
    count
};

/// Every catalogued action in declaration order.
pub(crate) const ALL_ACTIONS: [ActionId; ACTION_COUNT] = {
    let mut all = [ActionId::FocusOtherBrowser; ACTION_COUNT];
    let mut index = 0;
    while index < CATALOGUE.len() {
        assert!(CATALOGUE[index].id as usize == index, "catalogue rows follow the ActionId declaration order");
        all[index] = CATALOGUE[index].id;
        index += 1;
    }
    all
};

/// The actions eligible for Folder Items Context Menus, in fixed group and row
/// order.
pub(crate) const FOLDER_ITEM_ACTIONS: [ActionId; FOLDER_ITEM_ACTION_COUNT] = {
    let mut ordered = [ActionId::RenameItem; FOLDER_ITEM_ACTION_COUNT];
    let mut filled = [false; FOLDER_ITEM_ACTION_COUNT];
    let mut index = 0;
    while index < CATALOGUE.len() {
        if let Some(item) = CATALOGUE[index].folder_item {
            let position = item.position as usize;
            assert!(position < FOLDER_ITEM_ACTION_COUNT && !filled[position], "Folder Items Context Menu positions are unique and dense");
            filled[position] = true;
            ordered[position] = CATALOGUE[index].id;
        }
        index += 1;
    }
    let mut position = 1;
    while position < FOLDER_ITEM_ACTION_COUNT {
        let before = ordered[position - 1] as usize;
        let after = ordered[position] as usize;
        let (Some(before), Some(after)) = (CATALOGUE[before].folder_item, CATALOGUE[after].folder_item) else { panic!("eligible actions have Folder Items Context Menu facts") };
        assert!(before.group as usize <= after.group as usize, "positions follow the fixed group order");
        position += 1;
    }
    ordered
};

/// The catalogue row of `action`.
pub const fn spec(action: ActionId) -> &'static ActionSpec {
    &ACTIONS[action as usize]
}

/// The Folder Items Context Menu facts of `action`, or `None` when the action
/// is not eligible for a Folder Items Context Menu.
pub const fn folder_item_spec(action: ActionId) -> Option<&'static FolderItemSpec> {
    spec(action).folder_item.as_ref()
}
