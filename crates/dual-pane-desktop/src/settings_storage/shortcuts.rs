//! Action bindings and shortcut lists.

use super::*;

pub(super) type BindingParameters = (&'static str, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<String>);

pub(super) fn load_bindings(connection: &Connection) -> Result<Vec<ActionBinding>, SettingsStorageError> {
    let bindings: Vec<ActionBinding> = ActionId::ALL
        .into_iter()
        .map(|action| {
            connection
                .query_row("SELECT command, shift, option, control, key FROM action_binding WHERE action = ?1", [action.as_str()], |row| {
                    let command = row.get::<_, Option<i64>>(0)?.unwrap_or_default() != 0;
                    let shift = row.get::<_, Option<i64>>(1)?.unwrap_or_default() != 0;
                    let option = row.get::<_, Option<i64>>(2)?.unwrap_or_default() != 0;
                    let control = row.get::<_, Option<i64>>(3)?.unwrap_or_default() != 0;
                    // NULL is a deliberately unbound action. Unknown key text
                    // drops only this binding, so the action uses its default.
                    Ok(match row.get::<_, Option<String>>(4)? {
                        None => Some(ActionBinding { action, shortcut: None }),
                        Some(text) => Key::parse(&text).map(|key| ActionBinding { action, shortcut: Some(Shortcut::new(command, shift, option, control, key)) }),
                    })
                })
                .optional()
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    Ok(bindings)
}

pub(super) fn load_lists(connection: &Connection, bindings: &[ActionBinding]) -> Result<Vec<ActionShortcuts>, SettingsStorageError> {
    let mut shortcut_lists = Vec::new();
    for binding in bindings {
        let mut statement = connection.prepare("SELECT command, shift, option, control, key FROM action_shortcut WHERE action = ?1 ORDER BY position")?;
        let entries = statement
            .query_map([binding.action.as_str()], |row| {
                let text: String = row.get(4)?;
                let command = row.get::<_, i64>(0)? != 0;
                let shift = row.get::<_, i64>(1)? != 0;
                let option = row.get::<_, i64>(2)? != 0;
                let control = row.get::<_, i64>(3)? != 0;
                Ok(Key::parse(&text).map(|key| Shortcut::new(command, shift, option, control, key)))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if entries.iter().all(Option::is_some) && (!entries.is_empty() || binding.shortcut.is_none()) {
            shortcut_lists.push(ActionShortcuts { action: binding.action, shortcuts: entries.into_iter().flatten().collect() });
        }
    }
    Ok(shortcut_lists)
}

pub(super) fn save(tx: &Transaction<'_>, snapshot: &SettingsSnapshot) -> Result<(), SettingsStorageError> {
    tx.execute("DELETE FROM action_shortcut", [])?;
    tx.execute("DELETE FROM action_binding", [])?;
    let lists = if snapshot.shortcut_lists.is_empty() { snapshot.bindings.iter().map(|binding| ActionShortcuts { action: binding.action, shortcuts: binding.shortcut.into_iter().collect() }).collect::<Vec<_>>() } else { snapshot.shortcut_lists.clone() };
    for action in ActionId::ALL {
        let shortcuts = lists.iter().find(|list| list.action == action).map(|list| list.shortcuts.as_slice()).unwrap_or(&[]);
        let first = ActionBinding { action, shortcut: shortcuts.first().copied().or_else(|| (!lists.iter().any(|list| list.action == action)).then(|| default_shortcuts(action).first().copied()).flatten()) };
        insert_binding(tx, &first)?;
        for (position, shortcut) in shortcuts.iter().enumerate() {
            insert_shortcut(tx, action, position, *shortcut)?;
        }
    }
    Ok(())
}

pub(super) fn preload_bindings(connection: &mut Connection) -> Result<(), SettingsStorageError> {
    let tx = connection.transaction()?;
    for binding in dual_pane_application::default_bindings() {
        let inserted = tx.execute("INSERT OR IGNORE INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", binding_params(&binding))?;
        if inserted != 0 {
            for (position, shortcut) in default_shortcuts(binding.action).into_iter().enumerate() {
                insert_shortcut(&tx, binding.action, position, shortcut)?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn binding_params(binding: &ActionBinding) -> BindingParameters {
    let shortcut = binding.shortcut;
    (binding.action.as_str(), shortcut.map(|value| i64::from(value.command)), shortcut.map(|value| i64::from(value.shift)), shortcut.map(|value| i64::from(value.option)), shortcut.map(|value| i64::from(value.control)), shortcut.map(|value| value.key.as_text()))
}

pub(super) fn insert_binding(tx: &Transaction<'_>, binding: &ActionBinding) -> Result<(), rusqlite::Error> {
    tx.execute("INSERT INTO action_binding(action, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", binding_params(binding))?;
    Ok(())
}

pub(super) fn insert_shortcut(tx: &Transaction<'_>, action: ActionId, position: usize, shortcut: Shortcut) -> Result<(), rusqlite::Error> {
    tx.execute("INSERT INTO action_shortcut(action, position, command, shift, option, control, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![action.as_str(), position as i64, i64::from(shortcut.command), i64::from(shortcut.shift), i64::from(shortcut.option), i64::from(shortcut.control), shortcut.key.as_text()])?;
    Ok(())
}

pub(super) fn stored_shortcuts(connection: &Connection, action: ActionId) -> Result<Vec<Shortcut>, rusqlite::Error> {
    let mut statement = connection.prepare("SELECT command, shift, option, control, key FROM action_shortcut WHERE action = ?1 ORDER BY position")?;
    statement
        .query_map([action.as_str()], |row| {
            let key: String = row.get(4)?;
            let command = row.get::<_, i64>(0)? != 0;
            let shift = row.get::<_, i64>(1)? != 0;
            let option = row.get::<_, i64>(2)? != 0;
            let control = row.get::<_, i64>(3)? != 0;
            Ok(Key::parse(&key).map(|key| Shortcut::new(command, shift, option, control, key)))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map(|shortcuts| shortcuts.into_iter().flatten().collect())
}

pub(super) fn replace_shortcuts(tx: &Transaction<'_>, action: ActionId, shortcuts: &[Shortcut]) -> Result<(), rusqlite::Error> {
    tx.execute("DELETE FROM action_shortcut WHERE action = ?1", [action.as_str()])?;
    for (position, shortcut) in shortcuts.iter().enumerate() {
        insert_shortcut(tx, action, position, *shortcut)?;
    }
    let first = shortcuts.first().copied();
    tx.execute("UPDATE action_binding SET command = ?2, shift = ?3, option = ?4, control = ?5, key = ?6 WHERE action = ?1", params![action.as_str(), first.map(|value| i64::from(value.command)), first.map(|value| i64::from(value.shift)), first.map(|value| i64::from(value.option)), first.map(|value| i64::from(value.control)), first.map(|value| value.key.as_text())])?;
    Ok(())
}
