mod api;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use arboard::Clipboard;
use tauri::State;
use zeroize::Zeroize;

use crate::crypto::hash;
use crate::crypto::keys::VaultState;
use crate::crypto::paste_encrypt;

pub(crate) const MAX_PASTE_BYTES: usize = 128 * 1024;

pub fn api_base_url() -> &'static str {
    if cfg!(debug_assertions) {
        option_env!("CMDV_API_URL").unwrap_or("https://localhost:5000")
    } else {
        "https://api.cmdv.to"
    }
}

pub fn paste_site_url() -> &'static str {
    if cfg!(debug_assertions) {
        option_env!("CMDV_PASTE_SITE_URL").unwrap_or("https://localhost:4321")
    } else {
        "https://cmdv.to"
    }
}

struct SecurePasteInFlightGuard(Arc<VaultState>);

impl Drop for SecurePasteInFlightGuard {
    fn drop(&mut self) {
        self.0
            .secure_paste_in_flight
            .store(false, Ordering::Release);
    }
}

fn try_acquire_in_flight(vault: &Arc<VaultState>) -> Option<SecurePasteInFlightGuard> {
    if vault
        .secure_paste_in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return None;
    }
    Some(SecurePasteInFlightGuard(vault.clone()))
}

/// Shared by global shortcut and `create_secure_paste` command.
pub async fn run_create_secure_paste(vault: &Arc<VaultState>) -> Result<(), String> {
    {
        let Some(_in_flight) = try_acquire_in_flight(vault) else {
            return Err("Secure paste is already in progress.".into());
        };

        tokio::task::spawn_blocking(copy_selection_to_clipboard)
            .await
            .map_err(|e| e.to_string())??;

        let mut text = read_clipboard_text()?;

        set_clipboard_skip_hash(vault, text.as_bytes())?;

        if text.trim().is_empty() {
            text.zeroize();
            return Err("Clipboard has no text to share.".into());
        }
        if text.len() > MAX_PASTE_BYTES {
            text.zeroize();
            return Err("Text is too large (max 128 KB).".into());
        }

        let mut encrypted = paste_encrypt::encrypt_paste(&text)?;
        text.zeroize();

        let paste_id = api::upload_ciphertext(&encrypted.data_b64).await?;
        encrypted.data_b64.zeroize();

        let mut link = format!(
            "{}/paste#id={},{}",
            paste_site_url().trim_end_matches('/'),
            paste_id,
            &encrypted.key_b64
        );
        encrypted.key_b64.zeroize();

        set_clipboard_skip_hash(vault, link.as_bytes())?;
        set_clipboard_text(&link)?;
        notify_secure_paste_created(&paste_id)?;
        link.zeroize();
    }

    wake_clipboard_monitor(vault)?;

    log::info!("Secure paste link created");
    Ok(())
}

#[tauri::command]
pub async fn create_secure_paste(vault: State<'_, Arc<VaultState>>) -> Result<(), String> {
    run_create_secure_paste(&vault).await
}

fn read_clipboard_text() -> Result<String, String> {
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let Ok(mut clipboard) = Clipboard::new() else {
            continue;
        };
        if let Ok(text) = clipboard.get_text() {
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
    }

    Err(
        "Could not capture selected text. Select text in the active window, then press Ctrl+Shift+C."
            .into(),
    )
}

fn copy_selection_to_clipboard() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    std::thread::sleep(Duration::from_millis(50));

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    for key in [Key::Control, Key::Alt, Key::Shift, Key::Meta] {
        let _ = enigo.key(key, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Unicode('c'), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| e.to_string())?;

    std::thread::sleep(Duration::from_millis(120));
    Ok(())
}

fn set_clipboard_text(value: &str) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        let Ok(mut clipboard) = Clipboard::new() else {
            last_err = "Could not open clipboard".into();
            continue;
        };
        match clipboard.set_text(value) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = format!("{e:?}"),
        }
    }
    Err(format!("Could not copy link to clipboard: {last_err}"))
}

fn set_clipboard_skip_hash(vault: &Arc<VaultState>, content: &[u8]) -> Result<(), String> {
    let hash_key = {
        let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
        guard.as_ref().map(|k| k.hash_key)
    };

    let Some(hash_key) = hash_key else {
        return Ok(());
    };

    let content_hash = hash::keyed_hash(&hash_key, content);
    {
        let mut skip = vault
            .clipboard_skip_hash
            .lock()
            .map_err(|_| "Lock poisoned")?;
        *skip = Some(content_hash);
    }

    Ok(())
}

fn wake_clipboard_monitor(vault: &Arc<VaultState>) -> Result<(), String> {
    let (lock, cvar) = &*vault.monitor_wake;
    *lock.lock().map_err(|_| "Lock poisoned")? = true;
    cvar.notify_one();
    Ok(())
}

fn paste_id_hint(paste_id: &str) -> String {
    if paste_id.len() > 6 {
        format!("{}…", &paste_id[..6])
    } else {
        paste_id.to_string()
    }
}

fn notify_secure_paste_created(paste_id: &str) -> Result<(), String> {
    let id_hint = paste_id_hint(paste_id);

    notify_rust::Notification::new()
        .summary("Secure paste ready")
        .appname("CMDV")
        .body(&format!("Link copied · {id_hint}"))
        .auto_icon()
        .timeout(5000)
        .show()
        .map_err(|e| e.to_string())
}

pub fn notify_secure_paste_error(message: &str) {
    let _ = notify_rust::Notification::new()
        .summary("Secure paste failed")
        .appname("CMDV")
        .body(message)
        .auto_icon()
        .timeout(10_000)
        .show();
}
