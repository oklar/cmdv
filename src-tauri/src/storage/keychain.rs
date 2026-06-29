use keyring::Entry;
use zeroize::Zeroize;

use super::SeedStore;

const SERVICE_NAME: &str = "com.cmdv.clipboard";
const SEED_KEY: &str = "master_key";
const ACCOUNT_REFRESH_KEY: &str = "account_refresh_token";
const ACCOUNT_EMAIL_KEY: &str = "account_email";

pub struct KeychainStore;

impl KeychainStore {
    pub fn new() -> Self {
        Self
    }

    fn entry() -> Result<Entry, String> {
        Entry::new(SERVICE_NAME, SEED_KEY).map_err(|e| e.to_string())
    }

    fn account_entry(key: &str) -> Result<Entry, String> {
        Entry::new(SERVICE_NAME, key).map_err(|e| e.to_string())
    }

    /// Persist the desktop account session: the (secret) refresh token plus the email used
    /// only to render the logged-in state before the first `/auth/me` round-trip.
    pub fn save_account_session(&self, refresh_token: &str, email: &str) -> Result<(), String> {
        Self::account_entry(ACCOUNT_REFRESH_KEY)?
            .set_password(refresh_token)
            .map_err(|e| format!("Failed to save account session: {}", e))?;
        Self::account_entry(ACCOUNT_EMAIL_KEY)?
            .set_password(email)
            .map_err(|e| format!("Failed to save account email: {}", e))
    }

    pub fn load_account_refresh_token(&self) -> Result<Option<String>, String> {
        match Self::account_entry(ACCOUNT_REFRESH_KEY)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Failed to load account session: {}", e)),
        }
    }

    pub fn load_account_email(&self) -> Result<Option<String>, String> {
        match Self::account_entry(ACCOUNT_EMAIL_KEY)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Failed to load account email: {}", e)),
        }
    }

    pub fn delete_account_session(&self) -> Result<(), String> {
        for key in [ACCOUNT_REFRESH_KEY, ACCOUNT_EMAIL_KEY] {
            match Self::account_entry(key)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(format!("Failed to delete account session: {}", e)),
            }
        }
        Ok(())
    }
}

impl SeedStore for KeychainStore {
    fn save_seed(&self, seed: &[u8]) -> Result<(), String> {
        let entry = Self::entry()?;
        let mut encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, seed);
        let result = entry
            .set_password(&encoded)
            .map_err(|e| format!("Failed to save seed to keychain: {}", e));
        encoded.zeroize();
        result
    }

    fn load_seed(&self) -> Result<Vec<u8>, String> {
        let entry = Self::entry()?;
        let mut encoded = entry
            .get_password()
            .map_err(|e| format!("Failed to load seed from keychain: {}", e))?;
        let result = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &encoded)
            .map_err(|e| format!("Failed to decode seed: {}", e));
        encoded.zeroize();
        result
    }

    fn delete_seed(&self) -> Result<(), String> {
        let entry = Self::entry()?;
        entry
            .delete_credential()
            .map_err(|e| format!("Failed to delete seed from keychain: {}", e))
    }

    fn exists(&self) -> Result<bool, String> {
        let entry = Self::entry()?;
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(format!("Failed to check keychain: {}", e)),
        }
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}
