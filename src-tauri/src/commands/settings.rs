use crate::db::settings::{AppSettings, SettingsDb};
use crate::apply_global_toggle_shortcut;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_settings(settings_db: State<'_, Arc<SettingsDb>>) -> Result<AppSettings, String> {
    Ok(settings_db.get_settings())
}

#[tauri::command]
pub fn update_settings(
    app: tauri::AppHandle,
    settings_db: State<'_, Arc<SettingsDb>>,
    settings: AppSettings,
) -> Result<(), String> {
    let previous = settings_db.get_settings();
    settings_db.save_settings(&settings)?;
    if let Err(e) = apply_global_toggle_shortcut(&app, &settings.global_toggle_shortcut) {
        let _ = settings_db.save_settings(&previous);
        let _ = apply_global_toggle_shortcut(&app, &previous.global_toggle_shortcut);
        return Err(e);
    }
    Ok(())
}
