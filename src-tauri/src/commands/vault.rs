use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use bip39::Mnemonic;
use serde::Serialize;
use tauri::{Manager, State};

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
    let setup_complete = settings_db.get_value("vault_encrypted_master_key").is_some();
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
    if settings_db.get_value("vault_encrypted_master_key").is_some() {
        return Err("Vault already exists".into());
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

    let mnemonic =
        Mnemonic::from_entropy(master_key.as_bytes()).map_err(|e| format!("BIP39 error: {}", e))?;
    let mnemonic_entropy = mnemonic.to_entropy();
    let words: Vec<String> = mnemonic.words().map(String::from).collect();

    let wrapping_key = derive_wrapping_key(&password, &mnemonic_entropy)?;
    let wrapped = wrap_master_key(&wrapping_key, &master_key)?;

    let (pw_hash, pw_salt) = hash_password(&password)?;

    settings_db.set_value("vault_encrypted_master_key", &B64.encode(&wrapped))?;
    settings_db.set_value("vault_password_hash", &B64.encode(pw_hash))?;
    settings_db.set_value("vault_password_salt", &B64.encode(pw_salt))?;

    keychain.save_seed(master_key.as_bytes())?;

    let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());
    db.set_encryption_key(&app_keys.db_key)?;
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

            let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());
            db.set_encryption_key(&app_keys.db_key)?;
            *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

            start_monitoring(&vault, &db, &settings_db);
            vault.setup_complete.store(true, Ordering::Relaxed);
            log::info!("Vault unlocked via keychain");
            Ok(())
        }
        Err(_) => Err("NEEDS_RECOVERY".into()),
    }
}

/// Auto-unlock without password if `require_password_on_open` is disabled.
/// Returns true if the vault was unlocked, false if a password is still needed.
#[tauri::command]
pub fn try_auto_unlock(
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<bool, String> {
    if settings_db.get_value("vault_encrypted_master_key").is_none() {
        return Ok(false);
    }

    if vault.keys.lock().map_err(|_| "Lock poisoned")?.is_some() {
        return Ok(true);
    }

    let settings = settings_db.get_settings();
    if settings.require_password_on_open {
        return Ok(false);
    }

    let keychain = KeychainStore::new();
    match keychain.load_seed() {
        Ok(seed) => {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&seed[..32]);
            let master_key = MasterKey::from_bytes(bytes);

            let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());
            db.set_encryption_key(&app_keys.db_key)?;
            *vault.keys.lock().map_err(|_| "Lock poisoned")? = Some(app_keys);

            start_monitoring(&vault, &db, &settings_db);
            vault.setup_complete.store(true, Ordering::Relaxed);
            log::info!("Vault auto-unlocked (lock screen disabled)");
            Ok(true)
        }
        Err(_) => Ok(false),
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
    stored_hash.copy_from_slice(&B64.decode(&stored_hash_b64).map_err(|e| e.to_string())?[..32]);
    stored_salt.copy_from_slice(&B64.decode(&stored_salt_b64).map_err(|e| e.to_string())?[..32]);

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

    let app_keys = AppKeys::new(master_key.derive_hash_key(), master_key.derive_db_key());
    db.set_encryption_key(&app_keys.db_key)?;
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

#[tauri::command]
pub fn reset_vault(
    app: tauri::AppHandle,
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
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

    for key in &[
        "vault_encrypted_master_key",
        "vault_password_hash",
        "vault_password_salt",
        "auth_email",
        "auth_access_token",
        "auth_refresh_token",
        "auth_has_subscription",
        "last_sync_at",
    ] {
        settings_db.delete_value(key).ok();
    }

    log::info!("Vault reset complete — exiting app");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn export_mnemonic(path: String, words: Vec<String>, format: String) -> Result<(), String> {
    let content = match format.as_str() {
        "txt" => generate_txt(&words),
        "pdf" => return write_pdf(&path, &words),
        _ => return Err(format!("Unknown format: {}", format)),
    };
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file: {}", e))
}

fn generate_txt(words: &[String]) -> String {
    let mut out = String::new();
    out.push_str("CMDV Recovery Phrase\n");
    out.push_str("===================\n\n");
    out.push_str("Keep this file in a safe place. You need these 24 words\n");
    out.push_str("plus your vault password to recover your data.\n\n");
    for (i, word) in words.iter().enumerate() {
        out.push_str(&format!("{:>2}. {}\n", i + 1, word));
    }
    out.push_str("\nWARNING: Anyone with these words and your password can\n");
    out.push_str("decrypt your clipboard data. Delete this file after\n");
    out.push_str("storing the phrase securely.\n");
    out
}

fn write_pdf(path: &str, words: &[String]) -> Result<(), String> {
    let mut lines: Vec<String> = Vec::new();
    lines.push("CMDV Recovery Kit".into());
    lines.push(String::new());
    lines.push("Keep this document in a safe place. You need these 24 words".into());
    lines.push("plus your vault password to recover your data.".into());
    lines.push(String::new());

    for (chunk_idx, chunk) in words.chunks(4).enumerate() {
        let idx_start = chunk_idx * 4;
        let row: String = chunk
            .iter()
            .enumerate()
            .map(|(j, w)| format!("{:>2}. {:<14}", idx_start + j + 1, w))
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(row);
    }

    lines.push(String::new());
    lines.push("Vault Password: ________________________________________".into());
    lines.push(String::new());
    lines.push("WARNING: Anyone with these words and your password can".into());
    lines.push("decrypt your clipboard data. Delete this file after".into());
    lines.push("storing the phrase securely.".into());

    let font_size = 11;
    let title_size = 18;
    let leading = 16;
    let page_width = 595;
    let page_height = 842;
    let margin = 50;

    let mut text_ops = String::new();
    text_ops.push_str("BT\n");

    for (i, line) in lines.iter().enumerate() {
        let y = page_height - margin - (i as i32) * leading;
        if y < margin {
            break;
        }
        let size = if i == 0 { title_size } else { font_size };
        let escaped = line
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        if i == 0 {
            text_ops.push_str(&format!(
                "/F1 {} Tf {} {} Td ({}) Tj\n",
                size, margin, y, escaped
            ));
        } else {
            text_ops.push_str(&format!(
                "/F1 {} Tf 0 -{} Td ({}) Tj\n",
                size, leading, escaped
            ));
        }
    }
    text_ops.push_str("ET\n");

    let stream = text_ops;
    let stream_len = stream.len();

    let mut pdf = String::new();
    let mut offsets: Vec<usize> = Vec::new();

    pdf.push_str("%PDF-1.4\n");

    offsets.push(pdf.len());
    pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.push_str(&format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        page_width, page_height
    ));

    offsets.push(pdf.len());
    pdf.push_str(&format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
        stream_len, stream
    ));

    offsets.push(pdf.len());
    pdf.push_str("5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>\nendobj\n");

    let xref_offset = pdf.len();
    pdf.push_str("xref\n");
    pdf.push_str(&format!("0 {}\n", offsets.len() + 1));
    pdf.push_str("0000000000 65535 f \n");
    for off in &offsets {
        pdf.push_str(&format!("{:010} 00000 n \n", off));
    }

    pdf.push_str("trailer\n");
    pdf.push_str(&format!("<< /Size {} /Root 1 0 R >>\n", offsets.len() + 1));
    pdf.push_str("startxref\n");
    pdf.push_str(&format!("{}\n", xref_offset));
    pdf.push_str("%%EOF\n");

    std::fs::write(path, pdf.as_bytes()).map_err(|e| format!("Failed to write PDF: {}", e))
}

fn start_monitoring(vault: &VaultState, db: &Arc<Database>, settings_db: &Arc<SettingsDb>) {
    vault.monitor_stop.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(100));

    let stop = vault.monitor_stop.clone();
    stop.store(false, Ordering::Relaxed);

    let guard = vault.keys.lock().unwrap();
    let keys = guard.as_ref().expect("keys must be set before monitoring");
    let hash_key = keys.hash_key;
    drop(guard);

    let settings = settings_db.get_settings();
    let max_entry_size = settings.max_entry_size_bytes as usize;
    let max_total_size = settings.max_total_size_bytes;
    let excluded_apps = settings.excluded_apps.clone();
    let poll_db = db.clone();

    std::thread::spawn(move || {
        let mut monitor = clipboard::ClipboardMonitor::new().with_excluded_apps(excluded_apps);
        monitor.seed_from_clipboard(&hash_key);
        while !stop.load(Ordering::Relaxed) {
            match monitor.poll_once(&poll_db, &hash_key, max_entry_size) {
                Ok(Some(id)) => {
                    log::info!("Captured clipboard entry: {}", id);
                    enforce_storage_limit(&poll_db, max_total_size);
                }
                Ok(None) => {}
                Err(e) => log::warn!("Clipboard poll error: {}", e),
            }

            std::thread::sleep(Duration::from_secs(1));
        }
        log::info!("Clipboard monitoring stopped");
    });

    log::info!("Clipboard monitoring started");
}

#[tauri::command]
pub fn export_database(
    path: String,
    vault: tauri::State<'_, Arc<VaultState>>,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<usize, String> {
    let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    guard.as_ref().ok_or("Vault is locked")?;
    let blob_key = {
        let keychain = crate::storage::keychain::KeychainStore::new();
        let seed = keychain.load_seed()?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        let master_key = crate::crypto::keys::MasterKey::from_bytes(master_bytes);
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
        path
    );
    Ok(entry_count)
}

#[tauri::command]
pub fn import_database(
    path: String,
    vault: tauri::State<'_, Arc<VaultState>>,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<usize, String> {
    let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    guard.as_ref().ok_or("Vault is locked")?;
    let blob_key = {
        let keychain = crate::storage::keychain::KeychainStore::new();
        let seed = keychain.load_seed()?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        let master_key = crate::crypto::keys::MasterKey::from_bytes(master_bytes);
        master_key.derive_blob_key()
    };
    drop(guard);

    let data = std::fs::read(&path).map_err(|e| format!("Failed to read import file: {}", e))?;
    let blob = crate::sync::blob::decrypt_blob(&blob_key, &data)?;

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

    log::info!("Imported {} new entries from {}", imported, path);
    Ok(imported)
}

#[tauri::command]
pub fn generate_pairing_qr(vault: tauri::State<'_, Arc<VaultState>>) -> Result<String, String> {
    let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    if guard.is_none() {
        return Err("Vault is locked".into());
    }
    drop(guard);

    let keychain = crate::storage::keychain::KeychainStore::new();
    let seed = keychain.load_seed()?;
    let mut master_bytes = [0u8; 32];
    master_bytes.copy_from_slice(&seed[..32]);
    let master_key = crate::crypto::keys::MasterKey::from_bytes(master_bytes);

    let mnemonic = bip39::Mnemonic::from_entropy(master_key.as_bytes())
        .map_err(|e| format!("BIP39 error: {}", e))?;
    let words: Vec<String> = mnemonic.words().map(String::from).collect();
    let payload = words.join(" ");

    use qrcode::render::svg;
    use qrcode::QrCode;

    let code =
        QrCode::new(payload.as_bytes()).map_err(|e| format!("QR generation error: {}", e))?;
    let svg_str = code.render::<svg::Color>().min_dimensions(256, 256).build();

    let data_url = format!(
        "data:image/svg+xml;base64,{}",
        base64::Engine::encode(&B64, svg_str.as_bytes())
    );

    Ok(data_url)
}

#[tauri::command]
pub async fn switch_to_cloud(
    vault: tauri::State<'_, Arc<VaultState>>,
    settings_db: tauri::State<'_, Arc<SettingsDb>>,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<(), String> {
    {
        let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
        if guard.is_none() {
            return Err("Vault is locked".into());
        }
    }

    let has_sub = settings_db
        .get_value("auth_has_subscription")
        .map(|v| v == "true")
        .unwrap_or(false);

    if !has_sub {
        return Err("Cloud sync requires an active subscription".into());
    }

    let mut settings = settings_db.get_settings();
    settings.mode = crate::db::settings::AppMode::Cloud;
    settings_db.save_settings(&settings)?;

    // Trigger initial sync upload
    let blob_key = {
        let keychain = crate::storage::keychain::KeychainStore::new();
        let seed = keychain.load_seed().map_err(|e| e.to_string())?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        let master_key = crate::crypto::keys::MasterKey::from_bytes(master_bytes);
        master_key.derive_blob_key()
    };

    let encrypted = crate::sync::blob::export_to_blob(&db, &blob_key)?;
    log::info!("Initial cloud sync: exported {} bytes", encrypted.len());

    Ok(())
}

#[tauri::command]
pub fn switch_to_local(settings_db: tauri::State<'_, Arc<SettingsDb>>) -> Result<(), String> {
    let mut settings = settings_db.get_settings();
    settings.mode = crate::db::settings::AppMode::Local;
    settings_db.save_settings(&settings)?;
    log::info!("Switched to local-only mode");
    Ok(())
}

fn enforce_storage_limit(db: &Database, max_total_size: i64) {
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
