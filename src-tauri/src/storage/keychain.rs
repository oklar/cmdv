use keyring::Entry;
use zeroize::Zeroize;

use super::SeedStore;

const SERVICE_NAME: &str = "com.cmdv.clipboard";
const SEED_KEY: &str = "master_key";

pub struct KeychainStore;

impl KeychainStore {
    pub fn new() -> Self {
        Self
    }

    fn entry() -> Result<Entry, String> {
        Entry::new(SERVICE_NAME, SEED_KEY).map_err(|e| e.to_string())
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

use base64::Engine as _;
