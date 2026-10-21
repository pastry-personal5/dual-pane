//! Favorites panel invokables and properties.

use super::*;

impl ffi::WorkspaceBridge {
    pub(super) fn group_count(&self) -> i32 {
        count(self.rust().shown.groups().len())
    }
    pub(super) fn group_id(&self, group: i32) -> i64 {
        self.group(group).map_or(-1, |group| group.id)
    }
    pub(super) fn group_name(&self, group: i32) -> QString {
        self.group(group).map_or_else(QString::default, |group| QString::from(group.name.as_str()))
    }
    pub(super) fn item_count(&self, group: i32) -> i32 {
        self.group(group).map_or(0, |group| count(group.items.len()))
    }
    pub(super) fn item_id(&self, group: i32, item: i32) -> i64 {
        self.item(group, item).map_or(-1, |item| item.id)
    }
    pub(super) fn item_alias(&self, group: i32, item: i32) -> QString {
        self.item(group, item).map_or_else(QString::default, |item| QString::from(item.alias.as_str()))
    }
    pub(super) fn item_path(&self, group: i32, item: i32) -> QString {
        self.item(group, item).map_or_else(QString::default, |item| QString::from(item.path.as_str()))
    }
    pub(super) fn group(&self, group: i32) -> Option<&dual_pane_adapters::FavoriteGroupViewModel> {
        usize::try_from(group).ok().and_then(|group| self.rust().shown.groups().get(group))
    }
    pub(super) fn item(&self, group: i32, item: i32) -> Option<&dual_pane_adapters::FavoriteItemViewModel> {
        let item = usize::try_from(item).ok()?;
        self.group(group)?.items.get(item)
    }
    pub(super) fn create_group(self: Pin<&mut Self>, name: &QString) -> QString {
        self.favorites(FavoritesEvent::CreateGroup { name: name.to_string() })
    }
    pub(super) fn rename_group(self: Pin<&mut Self>, id: i64, name: &QString) -> QString {
        self.favorites(FavoritesEvent::RenameGroup { id, name: name.to_string() })
    }
    pub(super) fn move_group(self: Pin<&mut Self>, id: i64, motion: i32) {
        let motion = match motion {
            0 => GroupMotion::Up,
            1 => GroupMotion::Down,
            2 => GroupMotion::Top,
            _ => GroupMotion::Bottom,
        };
        self.favorites(FavoritesEvent::MoveGroup { id, motion });
    }
    pub(super) fn delete_group(self: Pin<&mut Self>, id: i64) {
        self.favorites(FavoritesEvent::DeleteGroup { id });
    }
    pub(super) fn add_item(self: Pin<&mut Self>, group: i64) -> QString {
        self.favorites(FavoritesEvent::AddItem { group_id: group })
    }
    pub(super) fn rename_item(self: Pin<&mut Self>, id: i64, name: &QString) -> QString {
        self.favorites(FavoritesEvent::RenameItem { id, name: name.to_string() })
    }
    pub(super) fn remove_item(self: Pin<&mut Self>, id: i64) {
        self.favorites(FavoritesEvent::RemoveItem { id });
    }
    pub(super) fn drop_item(self: Pin<&mut Self>, id: i64, group: i64, slot: i32) -> QString {
        let Ok(slot) = usize::try_from(slot) else { return QString::default() };
        self.favorites(FavoritesEvent::DropItem { id, group_id: group, slot })
    }
    pub(super) fn open_item(self: Pin<&mut Self>, id: i64) {
        self.favorites(FavoritesEvent::OpenItem { id });
    }
    /// Submits a Sidebar gesture and returns its inline feedback.
    pub(super) fn favorites(mut self: Pin<&mut Self>, event: FavoritesEvent) -> QString {
        let rejection = with_session(|session| session.submit_favorites(event)).flatten();
        self.as_mut().session_changed();
        rejection.and_then(|(edit, reason)| favorite_rejection_text(edit, reason)).map_or_else(QString::default, QString::from)
    }
    pub(super) fn set_favorites_ready(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().favorites_ready != value {
            self.as_mut().rust_mut().get_mut().favorites_ready = value;
            self.favorites_ready_changed();
        }
    }
    pub(super) fn set_favorites_revision(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().favorites_revision != value {
            self.as_mut().rust_mut().get_mut().favorites_revision = value;
            self.favorites_revision_changed();
        }
    }
}
