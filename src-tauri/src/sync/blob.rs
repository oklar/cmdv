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
    pub encrypted_payload: Vec<u8>,
    pub nonce: Vec<u8>,
    pub content_type: String,
    pub content_hash: Vec<u8>,
    pub created_at: String,
    pub is_favorite: bool,
    pub is_sensitive: bool,
    pub size_bytes: i64,
}

impl From<&ClipboardEntry> for SyncEntry {
    fn from(e: &ClipboardEntry) -> Self {
        Self {
            id: e.id.clone(),
            encrypted_payload: e.encrypted_payload.clone(),
            nonce: e.nonce.clone(),
            content_type: e.content_type.as_str().to_string(),
            content_hash: e.content_hash.clone(),
            created_at: e.created_at.clone(),
            is_favorite: e.is_favorite,
            is_sensitive: e.is_sensitive,
            size_bytes: e.size_bytes,
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
