use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::Zeroize;

use crate::crypto::keys::{derive_wrapping_key, wrap_master_key, MasterKey, VaultState};
use crate::db::settings::SettingsDb;
use crate::storage::SeedStore;

use base64::Engine;
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub email: Option<String>,
    pub has_subscription: bool,
}

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
    refresh_token: String,
    has_subscription: bool,
}

#[derive(Deserialize)]
struct RegisterResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct SubscriptionStatusResponse {
    active: bool,
}

fn get_api_base(settings_db: &SettingsDb) -> String {
    settings_db
        .get_value("api_base_url")
        .unwrap_or_else(|| "https://api.cmdv.to".to_string())
}

#[tauri::command]
pub fn get_auth_status(settings_db: State<'_, Arc<SettingsDb>>) -> AuthState {
    let email = settings_db.get_value("auth_email");
    let token = settings_db.get_value("auth_access_token");
    let has_sub = settings_db
        .get_value("auth_has_subscription")
        .map(|v| v == "true")
        .unwrap_or(false);

    AuthState {
        is_authenticated: email.is_some() && token.is_some(),
        email,
        has_subscription: has_sub,
    }
}

#[tauri::command]
pub async fn register(
    email: String,
    password: String,
    vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
) -> Result<(), String> {
    let api_base = get_api_base(&settings_db);

    let (auth_hash, encrypted_mk) = {
        let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
        guard.as_ref().ok_or("Vault is locked")?;
        drop(guard);

        let keychain = crate::storage::keychain::KeychainStore::new();
        let mut seed = keychain.load_seed()?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        seed.zeroize();
        let master_key = MasterKey::from_bytes(master_bytes);
        master_bytes.zeroize();

        let mnemonic = bip39::Mnemonic::from_entropy(master_key.as_bytes())
            .map_err(|e| format!("BIP39 error: {}", e))?;
        let mut mnemonic_entropy = mnemonic.to_entropy();

        let wrapping_key = derive_wrapping_key(&password, &mnemonic_entropy)?;
        let auth_hash = crate::crypto::keys::argon2_derive_auth(&password, &mnemonic_entropy)?;
        mnemonic_entropy.zeroize();
        let wrapped = wrap_master_key(&wrapping_key, &master_key)?;

        (B64.encode(auth_hash), B64.encode(wrapped))
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/auth/register", api_base))
        .json(&serde_json::json!({
            "email": email,
            "auth_hash": auth_hash,
            "encrypted_master_key": encrypted_mk,
        }))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Registration failed ({}): {}", status, body));
    }

    let data: RegisterResponse = resp.json().await.map_err(|e| e.to_string())?;

    settings_db.set_value("auth_email", &email)?;
    settings_db.set_value("auth_access_token", &data.access_token)?;
    settings_db.set_value("auth_refresh_token", &data.refresh_token)?;
    settings_db.set_value("auth_has_subscription", "false")?;

    Ok(())
}

#[tauri::command]
pub async fn login(
    email: String,
    password: String,
    _vault: State<'_, Arc<VaultState>>,
    settings_db: State<'_, Arc<SettingsDb>>,
) -> Result<(), String> {
    let api_base = get_api_base(&settings_db);

    let auth_hash = {
        let keychain = crate::storage::keychain::KeychainStore::new();
        let mut seed = keychain.load_seed()?;
        let mut master_bytes = [0u8; 32];
        master_bytes.copy_from_slice(&seed[..32]);
        seed.zeroize();
        let master_key = MasterKey::from_bytes(master_bytes);
        master_bytes.zeroize();

        let mnemonic = bip39::Mnemonic::from_entropy(master_key.as_bytes())
            .map_err(|e| format!("BIP39 error: {}", e))?;
        let mut mnemonic_entropy = mnemonic.to_entropy();

        let result = B64.encode(crate::crypto::keys::argon2_derive_auth(
            &password,
            &mnemonic_entropy,
        )?);
        mnemonic_entropy.zeroize();
        result
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/auth/login", api_base))
        .json(&serde_json::json!({
            "email": email,
            "auth_hash": auth_hash,
        }))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Login failed ({}): {}", status, body));
    }

    let data: LoginResponse = resp.json().await.map_err(|e| e.to_string())?;

    settings_db.set_value("auth_email", &email)?;
    settings_db.set_value("auth_access_token", &data.access_token)?;
    settings_db.set_value("auth_refresh_token", &data.refresh_token)?;
    settings_db.set_value(
        "auth_has_subscription",
        if data.has_subscription {
            "true"
        } else {
            "false"
        },
    )?;

    Ok(())
}

#[tauri::command]
pub async fn logout(settings_db: State<'_, Arc<SettingsDb>>) -> Result<(), String> {
    settings_db.set_value("auth_email", "")?;
    settings_db.set_value("auth_access_token", "")?;
    settings_db.set_value("auth_refresh_token", "")?;
    settings_db.set_value("auth_has_subscription", "false")?;
    Ok(())
}

#[tauri::command]
pub async fn check_subscription(settings_db: State<'_, Arc<SettingsDb>>) -> Result<bool, String> {
    let api_base = get_api_base(&settings_db);
    let token = settings_db
        .get_value("auth_access_token")
        .ok_or("Not authenticated")?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/subscription/status", api_base))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !resp.status().is_success() {
        return Err("Failed to check subscription".into());
    }

    let data: SubscriptionStatusResponse = resp.json().await.map_err(|e| e.to_string())?;
    settings_db.set_value(
        "auth_has_subscription",
        if data.active { "true" } else { "false" },
    )?;

    Ok(data.active)
}

pub async fn refresh_token(settings_db: &SettingsDb) -> Result<String, String> {
    let api_base = get_api_base(settings_db);
    let refresh = settings_db
        .get_value("auth_refresh_token")
        .ok_or("No refresh token")?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/auth/refresh", api_base))
        .json(&serde_json::json!({ "refresh_token": refresh }))
        .send()
        .await
        .map_err(|e| format!("Token refresh error: {}", e))?;

    if !resp.status().is_success() {
        settings_db.set_value("auth_access_token", "").ok();
        settings_db.set_value("auth_refresh_token", "").ok();
        return Err("Session expired, please log in again".into());
    }

    let data: RefreshResponse = resp.json().await.map_err(|e| e.to_string())?;
    settings_db.set_value("auth_access_token", &data.access_token)?;
    settings_db.set_value("auth_refresh_token", &data.refresh_token)?;

    Ok(data.access_token)
}
