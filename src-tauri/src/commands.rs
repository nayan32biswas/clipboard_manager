//! Tauri commands the UI calls. Each one is a thin wrapper over `History`.

use tauri::{AppHandle, State};

use crate::history::{now_ms, History, ItemView};
use crate::settings::Settings;
use crate::window;

type CmdResult<T> = Result<T, String>;

fn to_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn search(history: State<'_, History>, query: String) -> CmdResult<Vec<ItemView>> {
    history.search(&query).map_err(to_err)
}

#[tauri::command]
pub fn copy_item(app: AppHandle, history: State<'_, History>, id: i64) -> CmdResult<()> {
    history.copy_item(id, now_ms()).map_err(to_err)?;
    window::hide(&app);
    Ok(())
}

#[tauri::command]
pub fn toggle_pin(history: State<'_, History>, id: i64) -> CmdResult<bool> {
    history.toggle_pin(id).map_err(to_err)
}

#[tauri::command]
pub fn delete_item(history: State<'_, History>, id: i64) -> CmdResult<()> {
    history.delete(id).map_err(to_err)
}

#[tauri::command]
pub fn clear_history(history: State<'_, History>) -> CmdResult<usize> {
    history.clear().map_err(to_err)
}

#[tauri::command]
pub fn get_settings(history: State<'_, History>) -> Settings {
    history.settings()
}

/// Checks the hotkey before saving, so a typo can't leave the app without one.
#[tauri::command]
pub fn set_settings(app: AppHandle, history: State<'_, History>, settings: Settings) -> CmdResult<Settings> {
    window::resume_hotkey(&app);
    let old = history.settings();
    if settings.hotkey != old.hotkey {
        window::change_hotkey(&app, &old.hotkey, &settings.hotkey)?;
    }
    let saved = history.update_settings(settings, now_ms()).map_err(to_err)?;
    window::apply_autostart(&app, saved.launch_at_login);
    Ok(saved)
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    window::hide(&app);
}

#[tauri::command]
pub fn pause_hotkey(app: AppHandle) {
    window::pause_hotkey(&app);
}

#[tauri::command]
pub fn resume_hotkey(app: AppHandle) {
    window::resume_hotkey(&app);
}
