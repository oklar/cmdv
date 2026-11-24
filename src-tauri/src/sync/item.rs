//! Sync items: an entry or a delete, encrypted on the device. The server sees an item's id,
//! kind, key version, pinned flag and size, and nothing else.

use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::crypto::sync_keys::ItemKeys;
use crate::db::EntryType;

const AD_PREFIX: &[u8] = b"cmdv-item-v1";
const ENTRY_FORMAT: u8 = 1;
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Entry,
    Delete,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Entry => "entry",
            ItemKind::Delete => "delete",
        }
    }
}

/// What an entry item carries. `content_hash` travels with it because an image's hash is taken
/// over its RGBA pixels before WebP encoding, so the receiver can't recompute it.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryPayload {
    pub content_type: EntryType,
    pub content: Vec<u8>,
    pub content_hash: Vec<u8>,
    pub is_favorite: bool,
    pub last_used_at: String,
}

#[derive(Debug, Clone)]
pub struct SyncItem {
    pub id: String,
    pub kind: ItemKind,
    pub key_version: u32,
    /// Plaintext so the server never trims pinned entries. Devices read the encrypted favorite flag.
    pub pinned: bool,
    /// Nonce followed by the AES-256-GCM ciphertext.
    pub ciphertext: Vec<u8>,
}

/// The same content gets the same id on every device of the account.
pub fn item_id(keys: &ItemKeys, content_hash: &[u8]) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&keys.id_key)
        .expect("HMAC accepts any key length");
    mac.update(content_hash);
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub fn encrypt_entry(keys: &ItemKeys, entry: &EntryPayload) -> Result<SyncItem, String> {
    let id = item_id(keys, &entry.content_hash);
    let plaintext = encode_entry(entry)?;
    let ciphertext = seal(keys, &id, ItemKind::Entry, &plaintext)?;
    Ok(SyncItem {
        id,
        kind: ItemKind::Entry,
        key_version: keys.key_version,
        pinned: entry.is_favorite,
        ciphertext,
    })
}

pub fn encrypt_delete(keys: &ItemKeys, content_hash: &[u8]) -> Result<SyncItem, String> {
    let id = item_id(keys, content_hash);
    let ciphertext = seal(keys, &id, ItemKind::Delete, &[])?;
    Ok(SyncItem {
        id,
        kind: ItemKind::Delete,
        key_version: keys.key_version,
        pinned: false,
        ciphertext,
    })
}

/// Returns the entry, or None for a delete. Fails for an item these keys didn't encrypt, and
/// for one whose id, kind or key version was changed after encryption.
pub fn decrypt(keys: &ItemKeys, item: &SyncItem) -> Result<Option<EntryPayload>, String> {
    if item.ciphertext.len() < NONCE_LEN {
        return Err("Sync item is too short".into());
    }
    let (nonce, body) = item.ciphertext.split_at(NONCE_LEN);
    let ad = associated_data(&item.id, item.kind, item.key_version);
    let cipher = Aes256Gcm::new_from_slice(&keys.enc_key).map_err(|e| e.to_string())?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: body, aad: &ad })
            .map_err(|_| "Sync item doesn't decrypt with this account's keys".to_string())?,
    );

    match item.kind {
        ItemKind::Delete if plaintext.is_empty() => Ok(None),
        ItemKind::Delete => Err("Sync delete carries content".into()),
        ItemKind::Entry => {
            let entry = decode_entry(&plaintext)?;
            if item_id(keys, &entry.content_hash) != item.id {
                return Err("Sync item id doesn't match its content".into());
            }
            Ok(Some(entry))
        }
    }
}

fn seal(keys: &ItemKeys, id: &str, kind: ItemKind, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(&keys.enc_key).map_err(|e| e.to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ad = associated_data(id, kind, keys.key_version);
    let body = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad: &ad })
        .map_err(|e| e.to_string())?;

    let mut ciphertext = Vec::with_capacity(NONCE_LEN + body.len());
    ciphertext.extend_from_slice(&nonce);
    ciphertext.extend_from_slice(&body);
    Ok(ciphertext)
}

/// Binds the ciphertext to its id, kind and key version, so the server can't move it to
/// another id or turn an entry into a delete. Lengths are prefixed so no two values collide.
fn associated_data(id: &str, kind: ItemKind, key_version: u32) -> Vec<u8> {
    let kind = kind.as_str().as_bytes();
    let mut ad = Vec::with_capacity(AD_PREFIX.len() + 4 + id.len() + 4 + kind.len() + 4);
    ad.extend_from_slice(AD_PREFIX);
    ad.extend_from_slice(&(id.len() as u32).to_be_bytes());
    ad.extend_from_slice(id.as_bytes());
    ad.extend_from_slice(&(kind.len() as u32).to_be_bytes());
    ad.extend_from_slice(kind);
    ad.extend_from_slice(&key_version.to_be_bytes());
    ad
}

/// Format byte, content type (0 text, 1 image), favorite (0 or 1), content hash and
/// last-used time each with a one-byte length, then the content to the end.
fn encode_entry(entry: &EntryPayload) -> Result<Zeroizing<Vec<u8>>, String> {
    let hash_len = u8::try_from(entry.content_hash.len()).map_err(|_| "Content hash too long")?;
    let time_len = u8::try_from(entry.last_used_at.len()).map_err(|_| "Last-used time too long")?;

    let mut out = Zeroizing::new(Vec::with_capacity(
        5 + entry.content_hash.len() + entry.last_used_at.len() + entry.content.len(),
    ));
    out.push(ENTRY_FORMAT);
    out.push(match entry.content_type {
        EntryType::Text => 0,
        EntryType::Image => 1,
    });
    out.push(entry.is_favorite as u8);
    out.push(hash_len);
    out.extend_from_slice(&entry.content_hash);
    out.push(time_len);
    out.extend_from_slice(entry.last_used_at.as_bytes());
    out.extend_from_slice(&entry.content);
    Ok(out)
}

fn decode_entry(plain: &[u8]) -> Result<EntryPayload, String> {
    let malformed = || "Malformed sync entry".to_string();

    let (&format, rest) = plain.split_first().ok_or_else(malformed)?;
    if format != ENTRY_FORMAT {
        return Err(format!("Unknown sync entry format {format}"));
    }
    let (&content_type, rest) = rest.split_first().ok_or_else(malformed)?;
    let content_type = match content_type {
        0 => EntryType::Text,
        1 => EntryType::Image,
        other => return Err(format!("Unknown sync entry type {other}")),
    };
    let (&favorite, rest) = rest.split_first().ok_or_else(malformed)?;
    if favorite > 1 {
        return Err(malformed());
    }
    let (content_hash, rest) = take_prefixed(rest).ok_or_else(malformed)?;
    let (last_used_at, content) = take_prefixed(rest).ok_or_else(malformed)?;

    Ok(EntryPayload {
        content_type,
        content: content.to_vec(),
        content_hash: content_hash.to_vec(),
        is_favorite: favorite == 1,
        last_used_at: String::from_utf8(last_used_at.to_vec()).map_err(|_| malformed())?,
    })
}

fn take_prefixed(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&len, rest) = input.split_first()?;
    (rest.len() >= len as usize).then(|| rest.split_at(len as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::hash::keyed_hash;
    use crate::crypto::keys::MasterKey;
    use crate::crypto::sync_keys::KeyBundle;

    fn keys() -> ItemKeys {
        KeyBundle::generate(1, [3u8; 32]).item_keys()
    }

    fn text_entry(text: &str) -> EntryPayload {
        EntryPayload {
            content_type: EntryType::Text,
            content: text.as_bytes().to_vec(),
            content_hash: keyed_hash(&[3u8; 32], text.as_bytes()),
            is_favorite: true,
            last_used_at: "2026-11-24 20:15:00".into(),
        }
    }

    #[test]
    fn entry_round_trips() {
        let keys = keys();
        let image = EntryPayload {
            content_type: EntryType::Image,
            content: vec![0x52, 0x49, 0x46, 0x46, 0, 255],
            is_favorite: false,
            ..text_entry("pixels")
        };
        for entry in [text_entry("hello"), image] {
            let item = encrypt_entry(&keys, &entry).unwrap();
            assert_eq!(item.kind, ItemKind::Entry);
            assert_eq!(item.key_version, 1);
            assert_eq!(item.pinned, entry.is_favorite);
            assert_eq!(decrypt(&keys, &item).unwrap(), Some(entry));
        }
    }

    #[test]
    fn delete_round_trips_without_content() {
        let keys = keys();
        let entry = text_entry("gone");
        let item = encrypt_delete(&keys, &entry.content_hash).unwrap();
        assert_eq!(item.id, encrypt_entry(&keys, &entry).unwrap().id);
        assert!(!item.pinned);
        assert_eq!(decrypt(&keys, &item).unwrap(), None);
    }

    #[test]
    fn changed_id_kind_or_key_version_fails() {
        let keys = keys();
        let item = encrypt_entry(&keys, &text_entry("hello")).unwrap();
        let other_id = item_id(&keys, b"other content");

        let moved = SyncItem { id: other_id, ..item.clone() };
        assert!(decrypt(&keys, &moved).is_err());

        let retyped = SyncItem { kind: ItemKind::Delete, ..item.clone() };
        assert!(decrypt(&keys, &retyped).is_err());

        let reversioned = SyncItem { key_version: 2, ..item.clone() };
        assert!(decrypt(&keys, &reversioned).is_err());

        let mut tampered = item.clone();
        *tampered.ciphertext.last_mut().unwrap() ^= 1;
        assert!(decrypt(&keys, &tampered).is_err());
    }

    #[test]
    fn another_accounts_keys_fail() {
        let item = encrypt_entry(&keys(), &text_entry("hello")).unwrap();
        assert!(decrypt(&keys(), &item).is_err());
    }

    #[test]
    fn same_content_gets_the_same_id_on_every_device() {
        let master = MasterKey::from_bytes([5u8; 32]);
        let sealed = KeyBundle::generate(1, master.derive_hash_key()).seal(&master).unwrap();
        let device_a = KeyBundle::open(&master, &sealed).unwrap();
        let device_b = KeyBundle::open(&master, &sealed).unwrap();

        let hash_a = keyed_hash(device_a.hash_key(), b"copied text");
        let hash_b = keyed_hash(device_b.hash_key(), b"copied text");
        let id = item_id(&device_a.item_keys(), &hash_a);
        assert_eq!(id, item_id(&device_b.item_keys(), &hash_b));
        assert_eq!(id.len(), 43);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));

        assert_ne!(id, item_id(&device_a.item_keys(), &keyed_hash(device_a.hash_key(), b"other")));
        assert_ne!(id, item_id(&keys(), &hash_a));
    }

    #[test]
    fn entry_under_an_id_that_isnt_its_own_fails() {
        let keys = keys();
        let entry = text_entry("hello");
        let wrong_id = item_id(&keys, b"other content");
        let item = SyncItem {
            ciphertext: seal(&keys, &wrong_id, ItemKind::Entry, &encode_entry(&entry).unwrap()).unwrap(),
            id: wrong_id,
            kind: ItemKind::Entry,
            key_version: 1,
            pinned: true,
        };
        assert!(decrypt(&keys, &item).is_err());
    }

    #[test]
    fn unknown_entry_format_fails() {
        let mut plain = encode_entry(&text_entry("hello")).unwrap();
        plain[0] = 2;
        assert!(decode_entry(&plain).is_err());
        assert!(decode_entry(&[]).is_err());
        assert!(decode_entry(&[ENTRY_FORMAT, 0, 0, 40, 1]).is_err());
    }
}
