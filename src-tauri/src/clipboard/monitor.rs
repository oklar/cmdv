use arboard::Clipboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::crypto;
use crate::db::{Database, EntryType, NewEntry};
use crate::sensitive;

pub struct ClipboardMonitor {
    running: AtomicBool,
    last_text_hash: Option<Vec<u8>>,
    last_image_hash: Option<Vec<u8>>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            last_text_hash: None,
            last_image_hash: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn start(
        &mut self,
        _db: std::sync::Arc<Database>,
        encryption_key: &[u8; 32],
        hash_key: &[u8; 32],
        poll_interval: Duration,
        _max_entry_size: usize,
        _max_total_size: usize,
    ) -> Result<(), String> {
        if self.is_running() {
            return Err("Monitor already running".into());
        }
        self.running.store(true, Ordering::Relaxed);

        let _enc_key = *encryption_key;
        let _h_key = *hash_key;
        let _running = &self.running as *const AtomicBool;

        log::info!(
            "Clipboard monitor started (poll interval: {:?})",
            poll_interval
        );

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        log::info!("Clipboard monitor stopped");
    }

    pub fn poll_once(
        &mut self,
        db: &Database,
        encryption_key: &[u8; 32],
        hash_key: &[u8; 32],
        max_entry_size: usize,
    ) -> Result<Option<String>, String> {
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
                source_app: source::get_foreground_app(),
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

use super::source;
