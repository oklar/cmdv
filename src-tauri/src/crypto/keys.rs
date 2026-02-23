use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

const SALT_ENTRY_ENC: &[u8] = b"cmdv-entry-encryption";
const SALT_HASH_KEY: &[u8] = b"cmdv-hash-key";
const SALT_BLOB_ENC: &[u8] = b"cmdv-blob-encryption";
const SALT_WRAP: &[u8] = b"cmdv-wrap-derive";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        Self(key)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn derive_entry_key(&self) -> [u8; 32] {
        derive_key(&self.0, SALT_ENTRY_ENC)
    }

    pub fn derive_hash_key(&self) -> [u8; 32] {
        derive_key(&self.0, SALT_HASH_KEY)
    }

    pub fn derive_blob_key(&self) -> [u8; 32] {
        derive_key(&self.0, SALT_BLOB_ENC)
    }
}

pub struct AppKeys {
    pub entry_key: [u8; 32],
    pub hash_key: [u8; 32],
}

pub struct VaultState {
    pub keys: Mutex<Option<AppKeys>>,
    pub monitor_stop: Arc<AtomicBool>,
}

impl VaultState {
    pub fn new() -> Self {
        Self {
            keys: Mutex::new(None),
            monitor_stop: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn with_keys(&self, f: impl FnOnce(&AppKeys) -> Result<(), String>) -> Result<(), String> {
        let guard = self.keys.lock().map_err(|_| "Lock poisoned")?;
        match guard.as_ref() {
            Some(keys) => f(keys),
            None => Err("Vault is locked".into()),
        }
    }
}

pub fn derive_wrapping_key(password: &str, mnemonic_entropy: &[u8]) -> Result<[u8; 32], String> {
    let mut input = Vec::with_capacity(password.len() + mnemonic_entropy.len());
    input.extend_from_slice(password.as_bytes());
    input.extend_from_slice(mnemonic_entropy);

    let result = argon2_derive(&input, SALT_WRAP)?;
    input.zeroize();
    Ok(result)
}

pub fn hash_password(password: &str) -> Result<([u8; 32], [u8; 32]), String> {
    let mut salt = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let hash = argon2_derive(password.as_bytes(), &salt)?;
    Ok((hash, salt))
}

pub fn verify_password(password: &str, stored_hash: &[u8; 32], salt: &[u8; 32]) -> Result<bool, String> {
    let computed = argon2_derive(password.as_bytes(), salt)?;
    Ok(computed == *stored_hash)
}

fn argon2_derive(input: &[u8], salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(65536, 3, 4, Some(32)).map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let padded_salt = if salt.len() < 8 {
        let mut s = vec![0u8; 8];
        s[..salt.len()].copy_from_slice(salt);
        s
    } else {
        salt.to_vec()
    };

    let mut output = [0u8; 32];
    argon2
        .hash_password_into(input, &padded_salt, &mut output)
        .map_err(|e| e.to_string())?;

    Ok(output)
}

fn derive_key(ikm: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("HKDF expand should not fail for 32 bytes");
    okm
}

pub fn wrap_master_key(
    wrapping_key: &[u8; 32],
    master_key: &MasterKey,
) -> Result<Vec<u8>, String> {
    let (nonce, ciphertext) = super::encrypt::encrypt(wrapping_key, master_key.as_bytes())?;
    let mut wrapped = Vec::with_capacity(12 + ciphertext.len());
    wrapped.extend_from_slice(&nonce);
    wrapped.extend_from_slice(&ciphertext);
    Ok(wrapped)
}

pub fn unwrap_master_key(
    wrapping_key: &[u8; 32],
    wrapped: &[u8],
) -> Result<MasterKey, String> {
    if wrapped.len() < 12 {
        return Err("Wrapped key too short".into());
    }
    let (nonce, ciphertext) = wrapped.split_at(12);
    let plaintext = super::encrypt::decrypt(wrapping_key, nonce, ciphertext)?;
    if plaintext.len() != 32 {
        return Err("Unwrapped key has wrong length".into());
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&plaintext);
    Ok(MasterKey::from_bytes(key_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_key_derivation_produces_distinct_keys() {
        let mk = MasterKey::generate();
        let entry_key = mk.derive_entry_key();
        let hash_key = mk.derive_hash_key();
        let blob_key = mk.derive_blob_key();
        assert_ne!(entry_key, hash_key);
        assert_ne!(entry_key, blob_key);
        assert_ne!(hash_key, blob_key);
    }

    #[test]
    fn master_key_derivation_is_deterministic() {
        let bytes = [99u8; 32];
        let mk1 = MasterKey::from_bytes(bytes);
        let mk2 = MasterKey::from_bytes(bytes);
        assert_eq!(mk1.derive_entry_key(), mk2.derive_entry_key());
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let mk = MasterKey::generate();
        let wrapping_key = [77u8; 32];
        let wrapped = wrap_master_key(&wrapping_key, &mk).unwrap();
        let unwrapped = unwrap_master_key(&wrapping_key, &wrapped).unwrap();
        assert_eq!(mk.as_bytes(), unwrapped.as_bytes());
    }

    #[test]
    fn unwrap_with_wrong_key_fails() {
        let mk = MasterKey::generate();
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let wrapped = wrap_master_key(&key1, &mk).unwrap();
        assert!(unwrap_master_key(&key2, &wrapped).is_err());
    }

    #[test]
    fn password_hash_and_verify() {
        let (hash, salt) = hash_password("my_password").unwrap();
        assert!(verify_password("my_password", &hash, &salt).unwrap());
        assert!(!verify_password("wrong_password", &hash, &salt).unwrap());
    }

    #[test]
    fn wrapping_key_derivation() {
        let wk1 = derive_wrapping_key("pass", b"entropy_aaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let wk2 = derive_wrapping_key("pass", b"entropy_bbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        assert_ne!(wk1, wk2);
    }
}
