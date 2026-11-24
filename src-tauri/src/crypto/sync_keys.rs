//! Keys for end-to-end encrypted sync. Each account has one key bundle, made on its first
//! device and stored on the server sealed under a key derived from the recovery phrase. Item
//! keys come from the bundle's random data key, so a new phrase can keep or replace them
//! separately.

use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::encrypt;
use super::keys::{derive_key, MasterKey};

const WRAP_INFO: &[u8] = b"cmdv-sync-wrap-v1";
const ITEM_ENC_INFO: &[u8] = b"cmdv-sync-item-enc-v1";
const ITEM_ID_INFO: &[u8] = b"cmdv-sync-item-id-v1";

/// Format byte, key version (u32 big-endian), data key, hash key.
const BUNDLE_FORMAT: u8 = 1;
const BUNDLE_LEN: usize = 1 + 4 + 32 + 32;

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct KeyBundle {
    version: u32,
    data_key: [u8; 32],
    hash_key: [u8; 32],
}

/// The keys one key version encrypts and names items with.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ItemKeys {
    pub key_version: u32,
    pub enc_key: [u8; 32],
    pub id_key: [u8; 32],
}

impl KeyBundle {
    /// A bundle with a new random data key. `hash_key` is carried as given, so content hashes
    /// stay comparable across key versions.
    pub fn generate(version: u32, hash_key: [u8; 32]) -> Self {
        let mut data_key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut data_key);
        Self {
            version,
            data_key,
            hash_key,
        }
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn hash_key(&self) -> &[u8; 32] {
        &self.hash_key
    }

    pub fn item_keys(&self) -> ItemKeys {
        ItemKeys {
            key_version: self.version,
            enc_key: derive_key(&self.data_key, ITEM_ENC_INFO),
            id_key: derive_key(&self.data_key, ITEM_ID_INFO),
        }
    }

    pub fn seal(&self, master: &MasterKey) -> Result<Vec<u8>, String> {
        let mut plain = Zeroizing::new(Vec::with_capacity(BUNDLE_LEN));
        plain.push(BUNDLE_FORMAT);
        plain.extend_from_slice(&self.version.to_be_bytes());
        plain.extend_from_slice(&self.data_key);
        plain.extend_from_slice(&self.hash_key);

        let wrap_key = Zeroizing::new(derive_key(master.as_bytes(), WRAP_INFO));
        encrypt::encrypt_blob(&wrap_key, &plain)
    }

    /// Fails when the bundle was sealed under another recovery phrase, or was changed.
    pub fn open(master: &MasterKey, sealed: &[u8]) -> Result<Self, String> {
        let wrap_key = Zeroizing::new(derive_key(master.as_bytes(), WRAP_INFO));
        let plain = Zeroizing::new(
            encrypt::decrypt_blob(&wrap_key, sealed)
                .map_err(|_| "The sync key doesn't open with this recovery phrase".to_string())?,
        );
        if plain.len() != BUNDLE_LEN || plain[0] != BUNDLE_FORMAT {
            return Err("Unknown sync key format".into());
        }

        let mut bundle = Self {
            version: u32::from_be_bytes(plain[1..5].try_into().unwrap()),
            data_key: [0u8; 32],
            hash_key: [0u8; 32],
        };
        bundle.data_key.copy_from_slice(&plain[5..37]);
        bundle.hash_key.copy_from_slice(&plain[37..69]);
        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_bundle_opens_with_the_same_phrase() {
        let master = MasterKey::from_bytes([7u8; 32]);
        let bundle = KeyBundle::generate(1, master.derive_hash_key());
        let sealed = bundle.seal(&master).unwrap();

        let opened = KeyBundle::open(&MasterKey::from_bytes([7u8; 32]), &sealed).unwrap();
        assert_eq!(opened.version(), 1);
        assert_eq!(opened.hash_key(), &master.derive_hash_key());
        assert_eq!(opened.item_keys().enc_key, bundle.item_keys().enc_key);
        assert_eq!(opened.item_keys().id_key, bundle.item_keys().id_key);
    }

    #[test]
    fn sealed_bundle_does_not_open_with_another_phrase() {
        let master = MasterKey::from_bytes([7u8; 32]);
        let sealed = KeyBundle::generate(1, master.derive_hash_key()).seal(&master).unwrap();
        assert!(KeyBundle::open(&MasterKey::from_bytes([8u8; 32]), &sealed).is_err());
    }

    #[test]
    fn changed_bundle_does_not_open() {
        let master = MasterKey::from_bytes([7u8; 32]);
        let mut sealed = KeyBundle::generate(1, master.derive_hash_key()).seal(&master).unwrap();
        *sealed.last_mut().unwrap() ^= 1;
        assert!(KeyBundle::open(&master, &sealed).is_err());
    }

    #[test]
    fn item_keys_are_distinct_and_follow_the_data_key() {
        let a = KeyBundle::generate(1, [0u8; 32]);
        let b = KeyBundle::generate(1, [0u8; 32]);
        let keys = a.item_keys();
        assert_ne!(keys.enc_key, keys.id_key);
        assert_ne!(keys.enc_key, a.data_key);
        assert_ne!(keys.enc_key, b.item_keys().enc_key);
        assert_eq!(keys.key_version, 1);
    }
}
