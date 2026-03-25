use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::Zeroize;

use crate::crypto::keys::{MasterKey, VaultState};
use crate::db::settings::SettingsDb;
use crate::db::Database;
use crate::storage::keychain::KeychainStore;
use crate::storage::SeedStore;
use crate::sync::{blob, client::SyncClient, conflict};

use super::auth;

#[derive(Serialize)]
pub struct SyncState {
    pub is_syncing: bool,
    pub last_sync_at: Option<String>,
    pub syncs_remaining: Option<i32>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SyncResult {
    pub success: bool,
    pub entries_merged: usize,
    pub error: Option<String>,
}

fn get_api_base(settings_db: &SettingsDb) -> String {
    settings_db
        .get_value("api_base_url")
        .unwrap_or_else(|| "https://api.cmdv.to".to_string())
}

fn get_blob_key() -> Result<[u8; 32], String> {
    let keychain = KeychainStore::new();
    let mut seed = keychain.load_seed()?;
    let mut master_bytes = [0u8; 32];
    master_bytes.copy_from_slice(&seed[..32]);
    seed.zeroize();
    let master_key = MasterKey::from_bytes(master_bytes);
    master_bytes.zeroize();
    Ok(master_key.derive_blob_key())
}

async fn get_valid_token(settings_db: &SettingsDb) -> Result<String, String> {
    let token = settings_db
        .get_value("auth_access_token")
        .ok_or("Not authenticated")?;

    if token.is_empty() {
        return Err("Not authenticated".into());
    }

    // Try to use existing token; if it fails we'll refresh
    Ok(token)
}

#[tauri::command]
pub async fn trigger_sync(
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
    db: State<'_, Arc<Database>>,
) -> Result<SyncResult, String> {
    // Verify vault is unlocked
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

    let api_base = get_api_base(&settings_db);
    let mut token = get_valid_token(&settings_db).await?;
    let blob_key = get_blob_key()?;
    let sync_client = SyncClient::new(&api_base);

    // Step 1: Get download URL
    let download_url_resp = match sync_client.get_download_url(&token).await {
        Ok(resp) => resp,
        Err(e) if e.contains("401") || e.contains("Unauthorized") => {
            token = auth::refresh_token(&settings_db).await?;
            sync_client.get_download_url(&token).await?
        }
        Err(e) => return Err(e),
    };

    // Step 2: Download and decrypt remote blob (may not exist yet)
    let (remote_entries, etag) = match sync_client.download_blob(&download_url_resp.url).await {
        Ok((data, etag)) => {
            let remote_blob = blob::decrypt_blob(&blob_key, &data)?;
            (remote_blob.entries, etag)
        }
        Err(e) if e.contains("404") || e.contains("NoSuchKey") => (Vec::new(), None),
        Err(e) => return Err(format!("Download failed: {}", e)),
    };

    // Step 3: Get local entries and merge
    let local_entries = db.get_all_entries().map_err(|e| e.to_string())?;
    let merged = conflict::merge_entries(&local_entries, &remote_entries);
    let entries_merged = merged.len();

    // Step 4: Re-export as encrypted blob
    let merged_blob = blob::SyncBlob {
        version: 1,
        entries: merged,
    };
    let json = serde_json::to_vec(&merged_blob).map_err(|e| e.to_string())?;
    let encrypted = crate::crypto::encrypt::encrypt_blob(&blob_key, &json)?;

    // Step 5: Get upload URL
    let upload_url_resp = sync_client.get_upload_url(&token).await?;

    // Step 6: Upload with ETag conflict detection
    match sync_client
        .upload_blob(&upload_url_resp.url, encrypted, etag.as_deref())
        .await
    {
        Ok(()) => {
            settings_db
                .set_value("last_sync_at", &chrono::Utc::now().to_rfc3339())
                .ok();

            Ok(SyncResult {
                success: true,
                entries_merged,
                error: None,
            })
        }
        Err(e) if e.contains("Conflict") || e.contains("412") => {
            // ETag conflict - retry once
            log::warn!("ETag conflict during sync, retrying...");
            Err("Sync conflict - please try again".into())
        }
        Err(e) => Err(format!("Upload failed: {}", e)),
    }
}

#[tauri::command]
pub async fn get_sync_status(settings_db: State<'_, Arc<SettingsDb>>) -> Result<SyncState, String> {
    let is_authenticated = settings_db
        .get_value("auth_access_token")
        .map(|t| !t.is_empty())
        .unwrap_or(false);

    if !is_authenticated {
        return Ok(SyncState {
            is_syncing: false,
            last_sync_at: None,
            syncs_remaining: None,
            error: None,
        });
    }

    let api_base = get_api_base(&settings_db);
    let token = match get_valid_token(&settings_db).await {
        Ok(t) => t,
        Err(_) => {
            return Ok(SyncState {
                is_syncing: false,
                last_sync_at: settings_db.get_value("last_sync_at"),
                syncs_remaining: None,
                error: Some("Not authenticated".into()),
            });
        }
    };

    let sync_client = SyncClient::new(&api_base);
    match sync_client.get_sync_status(&token).await {
        Ok(status) => Ok(SyncState {
            is_syncing: false,
            last_sync_at: status.last_sync_at,
            syncs_remaining: Some(status.syncs_remaining_today),
            error: None,
        }),
        Err(e) => Ok(SyncState {
            is_syncing: false,
            last_sync_at: settings_db.get_value("last_sync_at"),
            syncs_remaining: None,
            error: Some(e),
        }),
    }
}
