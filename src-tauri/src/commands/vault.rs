use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use bip39::Mnemonic;
use serde::Serialize;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroize;

use crate::clipboard;
use crate::crypto::keys::{AppKeys, MasterKey, VaultState};
use crate::db::settings::SettingsDb;
use crate::db::Database;
use crate::storage::keychain::KeychainStore;
use crate::storage::SeedStore;

#[derive(Serialize)]
pub struct VaultStatus {
    pub setup_complete: bool,
    pub locked: bool,
}

#[derive(Serialize)]
pub struct SetupResult {
    pub mnemonic: Vec<String>,
}

#[tauri::command]
pub fn get_vault_status(
    vault: State<'_, Arc<VaultState>>,
) -> Result<VaultStatus, String> {
    let setup_complete = KeychainStore::new().exists()?;
    let locked = vault.keys.lock().map_err(|_| "Lock poisoned")?.is_none();
    Ok(VaultStatus {
        setup_complete,
        locked,
    })
}

#[tauri::command]
pub fn setup_vault(
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<SetupResult, String> {
    let keychain = KeychainStore::new();
    let master_key = match keychain.exists() {
        Ok(true) => {
            let mut seed = keychain.load_seed()?;
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&seed[..32]);
            seed.zeroize();
            log::info!("Reusing existing master key from keychain");
            let mk = MasterKey::from_bytes(bytes);
            bytes.zeroize();
            mk
        }
        _ => {
            log::info!("Generating new master key");
            MasterKey::generate()
        }
    };

    let mnemonic =
        Mnemonic::from_entropy(master_key.as_bytes()).map_err(|e| format!("BIP39 error: {}", e))?;
    let words: Vec<String> = mnemonic.words().map(String::from).collect();

    let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());
    db.set_encryption_key(&app_keys.db_key)?;
    keychain.save_seed(master_key.as_bytes())?;

    *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

    start_monitoring(&vault, &db, &settings_db);

    log::info!("Vault setup complete");
    Ok(SetupResult { mnemonic: words })
}

#[tauri::command]
pub fn finish_setup(vault: State<'_, Arc<VaultState>>) {
    vault.setup_complete.store(true, Ordering::Relaxed);
    log::info!("Setup flow finished, auto-hide enabled");
}

/// Auto-unlock from the keychain-stored master key. Returns true if the vault was unlocked,
/// false if the keychain has no seed (e.g. a new device that needs recovery).
#[tauri::command]
pub fn try_auto_unlock(
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<bool, String> {
    if vault.keys.lock().map_err(|_| "Lock poisoned")?.is_some() {
        return Ok(true);
    }

    let keychain = KeychainStore::new();
    match keychain.load_seed() {
        Ok(mut seed) => {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&seed[..32]);
            seed.zeroize();
            let master_key = MasterKey::from_bytes(bytes);
            bytes.zeroize();

            let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());
            db.set_encryption_key(&app_keys.db_key)?;
            *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

            start_monitoring(&vault, &db, &settings_db);
            vault.setup_complete.store(true, Ordering::Relaxed);
            log::info!("Vault auto-unlocked from keychain");
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Restore the vault on a device whose keychain seed is missing (new device or keychain loss)
/// by reconstructing the master key directly from the 24-word recovery phrase.
#[tauri::command]
pub fn recover_vault(
    mnemonic_words: String,
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    let mnemonic = Mnemonic::parse_normalized(mnemonic_words.trim())
        .map_err(|e| format!("Invalid recovery phrase: {}", e))?;
    let mut entropy = mnemonic.to_entropy();
    if entropy.len() != 32 {
        entropy.zeroize();
        return Err("Recovery phrase must be 24 words".into());
    }

    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&entropy);
    entropy.zeroize();
    let master_key = MasterKey::from_bytes(bytes);
    bytes.zeroize();

    let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());

    // Validate the key against the local DB (if any) before persisting it, so a wrong phrase
    // can't poison the keychain and lock the user out on the next launch.
    db.set_encryption_key(&app_keys.db_key)?;
    KeychainStore::new().save_seed(master_key.as_bytes())?;

    *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

    start_monitoring(&vault, &db, &settings_db);
    vault.setup_complete.store(true, Ordering::Relaxed);
    log::info!("Vault recovered via recovery phrase");
    Ok(())
}

#[tauri::command]
pub fn lock_vault(vault: State<'_, Arc<VaultState>>) -> Result<(), String> {
    vault.monitor_stop.store(true, Ordering::Relaxed);
    *vault.keys.lock().map_err(|_| "Lock poisoned")? = None;
    log::info!("Vault locked");
    Ok(())
}

#[tauri::command]
pub fn reset_vault(
    app: tauri::AppHandle,
    vault: State<'_, Arc<VaultState>>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    vault.monitor_stop.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(200));

    *vault.keys.lock().map_err(|_| "Lock poisoned")? = None;

    let keychain = KeychainStore::new();
    let _ = keychain.delete_seed();

    // Close the DB connection so the file lock is released
    db.close();

    // Delete DB files
    let db_path = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    for name in &["cmdv.db", "cmdv.db-wal", "cmdv.db-shm"] {
        let f = db_path.join(name);
        if f.exists() {
            std::fs::remove_file(&f).ok();
        }
    }

    log::info!("Vault reset complete — exiting app");
    app.exit(0);
    Ok(())
}

fn start_monitoring(vault: &VaultState, db: &Arc<Database>, settings_db: &Arc<SettingsDb>) {
    vault.monitor_stop.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(100));

    let stop = vault.monitor_stop.clone();
    stop.store(false, Ordering::Relaxed);

    let wake = vault.monitor_wake.clone();

    let guard = vault.keys.lock().unwrap();
    let keys = guard.as_ref().expect("keys must be set before monitoring");
    let hash_key = keys.hash_key;
    drop(guard);

    let settings = settings_db.get_settings();
    let max_entry_size = settings.max_entry_size_bytes as usize;
    let max_total_size = settings.max_total_size_bytes;
    let excluded_apps = settings.excluded_apps.clone();
    let poll_db = db.clone();
    let skip_hash = vault.clipboard_skip_hash.clone();
    let secure_paste_in_flight = vault.secure_paste_in_flight.clone();

    std::thread::spawn(move || {
        let mut monitor = clipboard::ClipboardMonitor::new().with_excluded_apps(excluded_apps);
        monitor.seed_from_clipboard(&hash_key);
        let (lock, cvar) = &*wake;
        while !stop.load(Ordering::Relaxed) {
            if !secure_paste_in_flight.load(Ordering::Acquire) {
                match monitor.poll_once(&poll_db, &hash_key, max_entry_size, &skip_hash) {
                    Ok(Some(id)) => {
                        log::info!("Captured clipboard entry: {}", id);
                        enforce_storage_limit(&poll_db, max_total_size);
                        crate::commands::sync::trigger_sync();
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("Clipboard poll error: {}", e),
                }
            }

            let mut woken = lock.lock().unwrap();
            if !*woken {
                let (guard, _) = cvar.wait_timeout(woken, Duration::from_secs(1)).unwrap();
                woken = guard;
            }
            *woken = false;
        }
        log::info!("Clipboard monitoring stopped");
    });

    log::info!("Clipboard monitoring started");
}

/// Returns None if the user cancels the save dialog.
#[tauri::command]
pub async fn export_database(
    window: tauri::WebviewWindow,
    vault: tauri::State<'_, Arc<VaultState>>,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Option<usize>, String> {
    vault.keys.lock().map_err(|_| "Lock poisoned")?.as_ref().ok_or("Vault is locked")?;
    let dialog_window = window.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_window
            .dialog()
            .file()
            .set_parent(&dialog_window)
            .set_file_name("cmdv-backup.bin")
            .add_filter("CMDV Backup", &["bin"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;

    let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    guard.as_ref().ok_or("Vault is locked")?;
    let blob_key = {
        let keychain = crate::storage::keychain::KeychainStore::new();
        let mut seed = keychain.load_seed()?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        seed.zeroize();
        let master_key = crate::crypto::keys::MasterKey::from_bytes(master_bytes);
        master_bytes.zeroize();
        master_key.derive_blob_key()
    };
    drop(guard);

    let encrypted = crate::sync::blob::export_to_blob(&db, &blob_key)?;
    let entry_count = db.get_entry_count().map_err(|e| e.to_string())? as usize;
    std::fs::write(&path, &encrypted).map_err(|e| format!("Failed to write export: {}", e))?;

    log::info!(
        "Exported {} entries ({} bytes) to {}",
        entry_count,
        encrypted.len(),
        path.display()
    );
    Ok(Some(entry_count))
}

/// Returns None if the user cancels the open dialog.
#[tauri::command]
pub async fn import_database(
    window: tauri::WebviewWindow,
    vault: tauri::State<'_, Arc<VaultState>>,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Option<usize>, String> {
    vault.keys.lock().map_err(|_| "Lock poisoned")?.as_ref().ok_or("Vault is locked")?;
    let dialog_window = window.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_window
            .dialog()
            .file()
            .set_parent(&dialog_window)
            .add_filter("CMDV Backup", &["bin"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;

    let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    guard.as_ref().ok_or("Vault is locked")?;
    let blob_key = {
        let keychain = crate::storage::keychain::KeychainStore::new();
        let mut seed = keychain.load_seed()?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        seed.zeroize();
        let master_key = crate::crypto::keys::MasterKey::from_bytes(master_bytes);
        master_bytes.zeroize();
        master_key.derive_blob_key()
    };
    drop(guard);

    let data = std::fs::read(&path).map_err(|e| format!("Failed to read import file: {}", e))?;
    let blob = crate::sync::blob::decrypt_blob(&blob_key, &data).map_err(|_| {
        "Could not decrypt this backup. It was created on a device with a different recovery \
         phrase. On this device, reset the vault, choose Restore from recovery phrase, then \
         import again. A damaged file can also cause this."
            .to_string()
    })?;

    let local_entries = db.get_all_entries().map_err(|e| e.to_string())?;
    let merged = crate::sync::conflict::merge_entries(&local_entries, &blob.entries);

    let mut imported = 0;
    for entry in &merged {
        if !db
            .entry_exists_by_hash(&entry.content_hash)
            .map_err(|e| e.to_string())?
        {
            let new_entry = crate::db::NewEntry {
                content: entry.content.clone(),
                content_type: crate::db::EntryType::from_str(&entry.content_type),
                content_hash: entry.content_hash.clone(),
                size_bytes: entry.size_bytes,
                is_favorite: entry.is_favorite,
            };
            db.insert_entry(&new_entry).map_err(|e| e.to_string())?;
            imported += 1;
        }
    }

    log::info!("Imported {} new entries from {}", imported, path.display());
    if imported > 0 {
        crate::commands::sync::trigger_sync();
    }
    Ok(Some(imported))
}

pub(crate) fn enforce_storage_limit(db: &Database, max_total_size: i64) {
    let total = match db.get_total_size() {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Failed to get total size: {}", e);
            return;
        }
    };
    if total > max_total_size {
        let excess = total - max_total_size;
        match db.prune_oldest_non_favorites(excess) {
            Ok(n) if n > 0 => log::info!(
                "Pruned {} entries to free {} bytes (total was {}, limit {})",
                n,
                excess,
                total,
                max_total_size
            ),
            Err(e) => log::warn!("Prune error: {}", e),
            _ => {}
        }
    }
}
