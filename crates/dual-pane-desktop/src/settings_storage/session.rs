//! The saved window layout and browser tabs.

use super::*;

pub(super) fn save(connection: &mut Connection, session: &WorkspaceSnapshot) -> Result<(), SettingsStorageError> {
    if !session.layout.valid() {
        return Err(SettingsStorageError::InvalidData("window layout"));
    }
    let tx = connection.transaction()?;
    tx.execute("DELETE FROM session_tab", [])?;
    tx.execute("DELETE FROM session_browser", [])?;
    tx.execute("DELETE FROM session_window", [])?;
    for (number, browser) in [(0_i64, &session.left), (1, &session.right)] {
        let active = browser.active_tab.ok_or(SettingsStorageError::InvalidData("active tab"))?;
        if browser.tabs.is_empty() || browser.tabs.len() > 8 || active >= browser.tabs.len() {
            return Err(SettingsStorageError::InvalidData("browser tabs"));
        }
        tx.execute("INSERT INTO session_browser(browser, active_position, active) VALUES (?1, ?2, ?3)", params![number, active as i64, i64::from(session.active_browser == if number == 0 { BrowserSide::Left } else { BrowserSide::Right })])?;
        for (position, tab) in browser.tabs.iter().enumerate() {
            tx.execute("INSERT INTO session_tab(browser, position, location) VALUES (?1, ?2, ?3)", params![number, position as i64, encode_location(&tab.location)])?;
        }
    }
    let layout = session.layout;
    tx.execute("INSERT INTO session_window(id, x, y, width, height, state, sidebar_splitter, browser_splitter, left_preview_height, right_preview_height) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", params![layout.frame.x, layout.frame.y, layout.frame.width, layout.frame.height, encode_window_state(layout.state), layout.sidebar_splitter, layout.browser_splitter, layout.left_preview_height, layout.right_preview_height])?;
    tx.commit()?;
    Ok(())
}

/// Session corruption is deliberately non-fatal: settings and Favorites can
/// still load, and the next valid session save replaces these rows.
pub(super) fn read_session(connection: &Connection) -> Result<SessionSnapshot, SettingsStorageError> {
    let browser_rows = connection.prepare("SELECT browser, active_position, active FROM session_browser ORDER BY browser")?.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let tab_rows = connection.prepare("SELECT browser, position, location FROM session_tab ORDER BY browser, position")?.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let layout_rows = connection.prepare("SELECT x, y, width, height, state, sidebar_splitter, browser_splitter, left_preview_height, right_preview_height FROM session_window")?.query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?, row.get::<_, i32>(2)?, row.get::<_, i32>(3)?, row.get::<_, i64>(4)?, row.get::<_, i32>(5)?, row.get::<_, i32>(6)?, row.get::<_, rusqlite::types::Value>(7)?, row.get::<_, rusqlite::types::Value>(8)?)))?.collect::<Result<Vec<_>, _>>()?;
    if browser_rows.is_empty() && tab_rows.is_empty() && layout_rows.is_empty() {
        return Ok(SessionSnapshot::Absent);
    }
    let damaged = || Ok(SessionSnapshot::Damaged);
    if browser_rows.len() != 2 || layout_rows.len() != 1 {
        return damaged();
    }
    let layout_row = &layout_rows[0];
    let Some(state) = decode_window_state(layout_row.4) else { return damaged() };
    let preview_height = |value: &rusqlite::types::Value| match value {
        rusqlite::types::Value::Integer(height) if (0..=32_767).contains(height) => *height as i32,
        _ => 0,
    };
    let layout = WindowLayout { frame: WindowFrame { x: layout_row.0, y: layout_row.1, width: layout_row.2, height: layout_row.3 }, state, sidebar_splitter: layout_row.5, browser_splitter: layout_row.6, left_preview_height: preview_height(&layout_row.7), right_preview_height: preview_height(&layout_row.8) };
    if !layout.valid() {
        return damaged();
    }
    let mut browsers = Vec::new();
    let mut active_browser = None;
    for (expected, (number, active_position, active)) in browser_rows.into_iter().enumerate() {
        if number != expected as i64 || !(0..=1).contains(&active) || (active == 1 && active_browser.replace(number).is_some()) {
            return damaged();
        }
        let rows = tab_rows.iter().filter(|(browser, _, _)| *browser == number).collect::<Vec<_>>();
        if rows.is_empty() || rows.len() > 8 || active_position < 0 || active_position as usize >= rows.len() || rows.iter().enumerate().any(|(position, (_, stored, _))| *stored != position as i64) {
            return damaged();
        }
        let locations = rows.into_iter().map(|(_, _, bytes)| decode_location(bytes)).collect::<Result<Vec<_>, _>>();
        let Ok(locations) = locations else { return damaged() };
        browsers.push(BrowserSnapshot { tabs: locations.into_iter().map(|location| dual_pane_application::TabSnapshot { location }).collect(), active_tab: Some(active_position as usize) });
    }
    if tab_rows.iter().any(|(browser, _, _)| *browser != 0 && *browser != 1) {
        return damaged();
    }
    let Some(active_browser) = active_browser else { return damaged() };
    Ok(SessionSnapshot::Saved(WorkspaceSnapshot { left: browsers.remove(0), right: browsers.remove(0), active_browser: if active_browser == 0 { BrowserSide::Left } else { BrowserSide::Right }, layout }))
}

pub(super) fn encode_window_state(state: WindowState) -> i64 {
    match state {
        WindowState::Normal => 0,
        WindowState::Zoomed => 1,
        WindowState::FullScreen => 2,
    }
}

pub(super) fn decode_window_state(value: i64) -> Option<WindowState> {
    match value {
        0 => Some(WindowState::Normal),
        1 => Some(WindowState::Zoomed),
        2 => Some(WindowState::FullScreen),
        _ => None,
    }
}
