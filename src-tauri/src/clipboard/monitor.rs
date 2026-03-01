use arboard::Clipboard;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::crypto;
use crate::db::{Database, EntryType, NewEntry};
use crate::sensitive;

use super::source;

pub struct ClipboardMonitor {
    last_text_hash: Option<Vec<u8>>,
    last_image_hash: Option<Vec<u8>>,
    excluded_apps: Vec<String>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            last_text_hash: None,
            last_image_hash: None,
            excluded_apps: Vec::new(),
        }
    }

    pub fn with_excluded_apps(mut self, apps: Vec<String>) -> Self {
        self.excluded_apps = apps;
        self
    }

    pub fn poll_once(
        &mut self,
        db: &Database,
        encryption_key: &[u8; 32],
        hash_key: &[u8; 32],
        max_entry_size: usize,
    ) -> Result<Option<String>, String> {
        // Skip if clipboard is marked as concealed by the OS
        if sensitive::flags::is_clipboard_concealed() {
            return Ok(None);
        }

        // Skip if the source app is in the exclude list
        let source_app = source::get_foreground_app();
        if let Some(ref app) = source_app {
            if source::is_excluded_with_custom(app, &self.excluded_apps) {
                return Ok(None);
            }
        }

        let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

        if let Ok(text) = clipboard.get_text() {
            if text.is_empty() {
                return Ok(None);
            }
            if text.len() > max_entry_size {
                return Ok(None);
            }

            let content_hash = crypto::hash::keyed_hash(hash_key, text.as_bytes());

            if self.last_text_hash.as_deref() == Some(&content_hash) {
                return Ok(None);
            }

            if db.entry_exists_by_hash(&content_hash).map_err(|e| e.to_string())? {
                self.last_text_hash = Some(content_hash);
                return Ok(None);
            }

            let is_sensitive = sensitive::detect::is_sensitive(&text);
            let (nonce, ciphertext) =
                crypto::encrypt::encrypt(encryption_key, text.as_bytes()).map_err(|e| e.to_string())?;

            let entry = NewEntry {
                encrypted_payload: ciphertext,
                nonce,
                content_type: EntryType::Text,
                content_hash: content_hash.clone(),
                size_bytes: text.len() as i64,
                is_favorite: false,
                is_sensitive,
                source_app,
            };

            let id = db.insert_entry(&entry).map_err(|e| e.to_string())?;
            self.last_text_hash = Some(content_hash);

            return Ok(Some(id));
        }

        Ok(None)
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}
