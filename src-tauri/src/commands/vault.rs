use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use bip39::Mnemonic;
use serde::Serialize;
use tauri::State;

use crate::clipboard;
use crate::crypto::keys::{
    derive_wrapping_key, hash_password, unwrap_master_key, verify_password, wrap_master_key,
    AppKeys, MasterKey, VaultState,
};
use crate::db::settings::SettingsDb;
use crate::db::Database;
use crate::storage::keychain::KeychainStore;
use crate::storage::SeedStore;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

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
    settings_db: State<'_, Arc<SettingsDb>>,
) -> Result<VaultStatus, String> {
    let setup_complete = settings_db
        .get_value("vault_setup_complete")
        .map(|v| v == "true")
        .unwrap_or(false);
    let locked = vault.keys.lock().map_err(|_| "Lock poisoned")?.is_none();
    Ok(VaultStatus {
        setup_complete,
        locked,
    })
}

#[tauri::command]
pub fn setup_vault(
    password: String,
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<SetupResult, String> {
    if settings_db
        .get_value("vault_setup_complete")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        return Err("Vault already set up".into());
    }

    if password.len() < 8 {
        return Err("Password must be at least 8 characters".into());
    }

    let keychain = KeychainStore::new();
    let master_key = match keychain.exists() {
        Ok(true) => {
            let seed = keychain.load_seed()?;
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&seed[..32]);
            log::info!("Migrating existing master key from keychain");
            MasterKey::from_bytes(bytes)
        }
        _ => {
            log::info!("Generating new master key");
            MasterKey::generate()
        }
    };

    let mnemonic = Mnemonic::from_entropy(master_key.as_bytes())
        .map_err(|e| format!("BIP39 error: {}", e))?;
    let mnemonic_entropy = mnemonic.to_entropy();
    let words: Vec<String> = mnemonic.words().map(String::from).collect();

    let wrapping_key = derive_wrapping_key(&password, &mnemonic_entropy)?;
    let wrapped = wrap_master_key(&wrapping_key, &master_key)?;

    let (pw_hash, pw_salt) = hash_password(&password)?;

    settings_db.set_value("vault_encrypted_master_key", &B64.encode(&wrapped))?;
    settings_db.set_value("vault_password_hash", &B64.encode(pw_hash))?;
    settings_db.set_value("vault_password_salt", &B64.encode(pw_salt))?;
    settings_db.set_value("vault_setup_complete", "true")?;

    keychain.save_seed(master_key.as_bytes())?;

    let app_keys = AppKeys {
        entry_key: master_key.derive_entry_key(),
        hash_key: master_key.derive_hash_key(),
    };
    *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

    start_monitoring(&vault, &db, &settings_db);

    log::info!("Vault setup complete");
    Ok(SetupResult { mnemonic: words })
}

#[tauri::command]
pub fn unlock_vault(
    password: String,
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    let stored_hash_b64 = settings_db
        .get_value("vault_password_hash")
        .ok_or("Vault not set up")?;
    let stored_salt_b64 = settings_db
        .get_value("vault_password_salt")
        .ok_or("Vault not set up")?;

    let stored_hash_vec = B64.decode(&stored_hash_b64).map_err(|e| e.to_string())?;
    let stored_salt_vec = B64.decode(&stored_salt_b64).map_err(|e| e.to_string())?;

    let mut stored_hash = [0u8; 32];
    let mut stored_salt = [0u8; 32];
    stored_hash.copy_from_slice(&stored_hash_vec);
    stored_salt.copy_from_slice(&stored_salt_vec);

    if !verify_password(&password, &stored_hash, &stored_salt)? {
        return Err("Wrong password".into());
    }

    let keychain = KeychainStore::new();
    match keychain.load_seed() {
        Ok(seed) => {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&seed[..32]);
            let master_key = MasterKey::from_bytes(bytes);

            let app_keys = AppKeys {
                entry_key: master_key.derive_entry_key(),
                hash_key: master_key.derive_hash_key(),
            };
            *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

            start_monitoring(&vault, &db, &settings_db);
            log::info!("Vault unlocked via keychain");
            Ok(())
        }
        Err(_) => Err("NEEDS_RECOVERY".into()),
    }
}

#[tauri::command]
pub fn recover_vault(
    password: String,
    mnemonic_words: String,
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    let stored_hash_b64 = settings_db
        .get_value("vault_password_hash")
        .ok_or("Vault not set up")?;
    let stored_salt_b64 = settings_db
        .get_value("vault_password_salt")
        .ok_or("Vault not set up")?;

    let mut stored_hash = [0u8; 32];
    let mut stored_salt = [0u8; 32];
    stored_hash.copy_from_slice(
        &B64.decode(&stored_hash_b64).map_err(|e| e.to_string())?[..32],
    );
    stored_salt.copy_from_slice(
        &B64.decode(&stored_salt_b64).map_err(|e| e.to_string())?[..32],
    );

    if !verify_password(&password, &stored_hash, &stored_salt)? {
        return Err("Wrong password".into());
    }

    let mnemonic = Mnemonic::parse_normalized(&mnemonic_words)
        .map_err(|e| format!("Invalid mnemonic: {}", e))?;
    let mnemonic_entropy = mnemonic.to_entropy();

    let wrapped_b64 = settings_db
        .get_value("vault_encrypted_master_key")
        .ok_or("No encrypted master key found")?;
    let wrapped = B64.decode(&wrapped_b64).map_err(|e| e.to_string())?;

    let wrapping_key = derive_wrapping_key(&password, &mnemonic_entropy)?;
    let master_key = unwrap_master_key(&wrapping_key, &wrapped)?;

    let keychain = KeychainStore::new();
    keychain.save_seed(master_key.as_bytes())?;

    let app_keys = AppKeys {
        entry_key: master_key.derive_entry_key(),
        hash_key: master_key.derive_hash_key(),
    };
    *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

    start_monitoring(&vault, &db, &settings_db);
    log::info!("Vault recovered via mnemonic");
    Ok(())
}

#[tauri::command]
pub fn lock_vault(vault: State<'_, Arc<VaultState>>) -> Result<(), String> {
    vault.monitor_stop.store(true, Ordering::Relaxed);
    *vault.keys.lock().map_err(|_| "Lock poisoned")? = None;
    log::info!("Vault locked");
    Ok(())
}

fn start_monitoring(vault: &VaultState, db: &Arc<Database>, settings_db: &Arc<SettingsDb>) {
    vault.monitor_stop.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(100));

    let stop = vault.monitor_stop.clone();
    stop.store(false, Ordering::Relaxed);

    let guard = vault.keys.lock().unwrap();
    let keys = guard.as_ref().expect("keys must be set before monitoring");
    let entry_key = keys.entry_key;
    let hash_key = keys.hash_key;
    drop(guard);

    let settings = settings_db.get_settings();
    let max_entry_size = settings.max_entry_size_bytes as usize;
    let poll_db = db.clone();

    std::thread::spawn(move || {
        let mut monitor = clipboard::ClipboardMonitor::new();
        while !stop.load(Ordering::Relaxed) {
            match monitor.poll_once(&poll_db, &entry_key, &hash_key, max_entry_size) {
                Ok(Some(id)) => log::info!("Captured clipboard entry: {}", id),
                Ok(None) => {}
                Err(e) => log::warn!("Clipboard poll error: {}", e),
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        log::info!("Clipboard monitoring stopped");
    });

    log::info!("Clipboard monitoring started");
}
