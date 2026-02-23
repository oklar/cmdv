use crate::db::settings::{AppSettings, SettingsDb};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_settings(settings_db: State<'_, Arc<SettingsDb>>) -> Result<AppSettings, String> {
    Ok(settings_db.get_settings())
}

#[tauri::command]
pub fn update_settings(
    settings_db: State<'_, Arc<SettingsDb>>,
    settings: AppSettings,
) -> Result<(), String> {
    settings_db.save_settings(&settings)
}
