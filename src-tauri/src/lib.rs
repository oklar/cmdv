pub mod clipboard;
pub mod commands;
pub mod crypto;
pub mod db;
pub mod image;
pub mod storage;
pub mod sync;

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_positioner::{Position, WindowExt};

static SUPPRESS_BLUR_HIDE: AtomicBool = AtomicBool::new(false);

/// Set when the process was started with `--tray` (e.g. OS login autostart entry).
static LAUNCHED_WITH_TRAY: OnceLock<bool> = OnceLock::new();
static AUTOSTART_TRAY_APPLIED: AtomicBool = AtomicBool::new(false);

fn launched_with_tray() -> bool {
    *LAUNCHED_WITH_TRAY.get_or_init(|| std::env::args().any(|a| a == "--tray"))
}

use crypto::keys::VaultState;

/// Applies the global show/hide shortcut from stored settings (replaces any previous registration).
pub(crate) fn apply_global_toggle_shortcut(app: &tauri::AppHandle, shortcut_str: &str) -> Result<(), String> {
    let trimmed = shortcut_str.trim();
    if trimmed.is_empty() {
        return Err("Shortcut cannot be empty.".to_string());
    }
    let parsed = Shortcut::from_str(trimmed).map_err(|e| format!("Invalid shortcut: {e}"))?;

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(parsed).map_err(|e| e.to_string())
}

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

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some("CMDV Clipboard Manager"));
    }
}

#[tauri::command]
fn hide_to_tray(
    app: tauri::AppHandle,
    vault: tauri::State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    let unlocked = vault.keys.lock().map(|g| g.is_some()).unwrap_or(false);
    let setup_done = vault.setup_complete.load(std::sync::atomic::Ordering::Relaxed);

    if !unlocked {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some("CMDV — Setup incomplete. Click to continue."));
        }

        let _ = notify_rust::Notification::new()
            .summary("Setup incomplete")
            .appname("CMDV")
            .body("Setup is incomplete. Click the tray icon to continue.")
            .auto_icon()
            .timeout(10000)
            .show();
    } else if !setup_done {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some("CMDV — Save your recovery phrase. Click to continue."));
        }

        let _ = notify_rust::Notification::new()
            .summary("Save your recovery phrase")
            .appname("CMDV")
            .body("Store it safely, then click the tray icon to continue setup.")
            .auto_icon()
            .timeout(10000)
            .show();
    }

    Ok(())
}

/// Shown when `check()` finds a newer version on startup (does not install automatically).
#[tauri::command]
fn notify_update_available(version: String) -> Result<(), String> {
    notify_rust::Notification::new()
        .summary("Update available")
        .appname("CMDV")
        .body(&format!(
            "Version {version} is ready. Install it from Settings."
        ))
        .auto_icon()
        .timeout(15_000)
        .show()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// After vault unlock: hide main window once and notify, only if we were started with `--tray`.
#[tauri::command]
fn apply_autostart_tray(app: tauri::AppHandle) -> Result<(), String> {
    if !launched_with_tray() {
        return Ok(());
    }
    if AUTOSTART_TRAY_APPLIED.swap(true, Ordering::Relaxed) {
        return Ok(());
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    notify_rust::Notification::new()
        .summary("Cmdv")
        .appname("CMDV")
        .body("Running in the system tray — click the icon to open.")
        .auto_icon()
        .timeout(15_000)
        .show()
        .map_err(|e| e.to_string())?;

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some("CMDV Clipboard Manager"));
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--tray"])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
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

            TrayIconBuilder::with_id("main")
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
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;

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

            let initial_settings = settings_db.get_settings();
            apply_global_toggle_shortcut(&app.handle(), &initial_settings.global_toggle_shortcut)?;

            if !cfg!(debug_assertions) {
                let s = &initial_settings;
                if s.login_autostart {
                    let _ = app.autolaunch().enable();
                } else {
                    let _ = app.autolaunch().disable();
                }
            }

            app.manage(Arc::new(settings_db));

            // --- Vault state (locked until user authenticates) ---
            let vault = Arc::new(VaultState::new());
            app.manage(vault);

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let _ = window.hide();
                api.prevent_close();
            }
            tauri::WindowEvent::Moved { .. } | tauri::WindowEvent::Resized { .. } => {
                SUPPRESS_BLUR_HIDE.store(true, Ordering::Relaxed);
            }
            tauri::WindowEvent::Focused(false) => {
                SUPPRESS_BLUR_HIDE.store(false, Ordering::Relaxed);
                let handle = window.app_handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if SUPPRESS_BLUR_HIDE.load(Ordering::Relaxed) {
                        return;
                    }
                    let should_hide = handle
                        .try_state::<Arc<VaultState>>()
                        .map(|v| {
                            v.keys.lock().map(|g| g.is_some()).unwrap_or(false)
                                && v.setup_complete.load(Ordering::Relaxed)
                        })
                        .unwrap_or(false);
                    if !should_hide {
                        return;
                    }
                    if let Some(w) = handle.get_webview_window("main") {
                        if !w.is_focused().unwrap_or(false) {
                            let _ = w.hide();
                        }
                    }
                });
            }
            tauri::WindowEvent::Focused(true) => {
                SUPPRESS_BLUR_HIDE.store(true, Ordering::Relaxed);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            hide_to_tray,
            notify_update_available,
            apply_autostart_tray,
            commands::clipboard::get_entries,
            commands::clipboard::search_entries,
            commands::clipboard::toggle_favorite,
            commands::clipboard::delete_entry,
            commands::clipboard::clear_all_entries,
            commands::clipboard::get_stats,
            commands::clipboard::force_clipboard_poll,
            commands::clipboard::copy_entry_to_clipboard,
            commands::clipboard::simulate_paste,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::vault::get_vault_status,
            commands::vault::setup_vault,
            commands::vault::finish_setup,
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
