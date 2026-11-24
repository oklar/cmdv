use crate::crypto::encrypt;
use crate::db::{ClipboardEntry, Database};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SyncBlob {
    pub version: u32,
    pub entries: Vec<SyncEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SyncEntry {
    pub id: String,
    pub content: Vec<u8>,
    pub content_type: String,
    pub content_hash: Vec<u8>,
    pub last_used_at: String,
    pub is_favorite: bool,
    pub size_bytes: i64,
    /// Kept through backup and restore so restored entries don't become syncable.
    /// Backups made before the flag existed have none, which reads as false.
    #[serde(default)]
    pub local_only: bool,
}

impl From<&ClipboardEntry> for SyncEntry {
    fn from(e: &ClipboardEntry) -> Self {
        Self {
            id: e.id.clone(),
            content: e.content.clone(),
            content_type: e.content_type.as_str().to_string(),
            content_hash: e.content_hash.clone(),
            last_used_at: e.last_used_at.clone(),
            is_favorite: e.is_favorite,
            size_bytes: e.size_bytes,
            local_only: e.local_only,
        }
    }
}

pub fn export_to_blob(db: &Database, blob_key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let entries = db.get_all_entries().map_err(|e| e.to_string())?;
    let sync_entries: Vec<SyncEntry> = entries.iter().map(SyncEntry::from).collect();
    let blob = SyncBlob {
        version: 1,
        entries: sync_entries,
    };
    let json = serde_json::to_vec(&blob).map_err(|e| e.to_string())?;
    encrypt::encrypt_blob(blob_key, &json)
}

pub fn decrypt_blob(blob_key: &[u8; 32], encrypted: &[u8]) -> Result<SyncBlob, String> {
    let json = encrypt::decrypt_blob(blob_key, encrypted)?;
    serde_json::from_slice(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{EntryType, NewEntry};

    #[test]
    fn backup_keeps_local_only() {
        let db = Database::open_in_memory().unwrap();
        for (hash, local_only) in [(1u8, true), (2u8, false)] {
            db.insert_entry(&NewEntry {
                content: vec![hash],
                content_type: EntryType::Text,
                content_hash: vec![hash],
                size_bytes: 1,
                is_favorite: false,
                local_only,
            })
            .unwrap();
        }

        let key = [9u8; 32];
        let blob = decrypt_blob(&key, &export_to_blob(&db, &key).unwrap()).unwrap();
        let mut flags: Vec<(Vec<u8>, bool)> = blob
            .entries
            .iter()
            .map(|e| (e.content_hash.clone(), e.local_only))
            .collect();
        flags.sort();
        assert_eq!(flags, vec![(vec![1], true), (vec![2], false)]);
    }

    #[test]
    fn backup_from_before_the_flag_imports_as_syncable() {
        let key = [9u8; 32];
        let json = br#"{"version":1,"entries":[{"id":"a","content":[104,105],"content_type":"text",
            "content_hash":[1],"last_used_at":"2026-01-01 00:00:00","is_favorite":false,"size_bytes":2}]}"#;
        let encrypted = encrypt::encrypt_blob(&key, json).unwrap();
        let blob = decrypt_blob(&key, &encrypted).unwrap();
        assert!(!blob.entries[0].local_only);
    }
}
