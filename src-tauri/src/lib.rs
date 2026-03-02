pub mod clipboard;
pub mod commands;
pub mod crypto;
pub mod db;
pub mod image;
pub mod sensitive;
pub mod storage;
pub mod sync;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_positioner::{Position, WindowExt};

use crypto::keys::VaultState;

fn toggle_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show_window(app);
    }
}

fn show_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.as_ref().window().move_window(Position::Center);
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // --- Tray icon with context menu ---
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("CMDV Clipboard Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // --- Register Ctrl+U global shortcut ---
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyU);
            app.global_shortcut().register(shortcut)?;

            // --- Database ---
            let db_path = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&db_path).ok();

            let db_file = db_path.join("cmdv.db");
            let db = db::Database::open_encrypted(&db_file).expect("failed to open database");
            let db = Arc::new(db);
            app.manage(db.clone());

            let settings_file = db_path.join("settings.db");
            let settings_db =
                db::settings::SettingsDb::open(&settings_file).expect("failed to open settings db");
            app.manage(Arc::new(settings_db));

            // --- Vault state (locked until user authenticates) ---
            let vault = Arc::new(VaultState::new());
            app.manage(vault);

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::clipboard::get_entries,
            commands::clipboard::search_entries,
            commands::clipboard::toggle_favorite,
            commands::clipboard::delete_entry,
            commands::clipboard::clear_all_entries,
            commands::clipboard::get_stats,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::vault::get_vault_status,
            commands::vault::setup_vault,
            commands::vault::unlock_vault,
            commands::vault::try_auto_unlock,
            commands::vault::recover_vault,
            commands::vault::lock_vault,
            commands::vault::export_mnemonic,
            commands::vault::reset_vault,
            commands::auth::get_auth_status,
            commands::auth::register,
            commands::auth::login,
            commands::auth::logout,
            commands::auth::check_subscription,
            commands::sync::trigger_sync,
            commands::sync::get_sync_status,
            commands::vault::switch_to_cloud,
            commands::vault::switch_to_local,
            commands::vault::generate_pairing_qr,
            commands::vault::export_database,
            commands::vault::import_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
