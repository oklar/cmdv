use arboard::Clipboard;

use crate::crypto;
use crate::db::{Database, EntryType, NewEntry};
use crate::image as img;
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
        hash_key: &[u8; 32],
        max_entry_size: usize,
    ) -> Result<Option<String>, String> {
        if sensitive::flags::is_clipboard_concealed() {
            return Ok(None);
        }

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

            if db
                .entry_exists_by_hash(&content_hash)
                .map_err(|e| e.to_string())?
            {
                self.last_text_hash = Some(content_hash);
                return Ok(None);
            }

            let is_sensitive = sensitive::detect::is_sensitive(&text);

            let entry = NewEntry {
                content: text.as_bytes().to_vec(),
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

        if let Ok(image_data) = clipboard.get_image() {
            let rgba = &image_data.bytes;
            if rgba.is_empty() {
                return Ok(None);
            }

            let content_hash = crypto::hash::keyed_hash(hash_key, rgba);

            if self.last_image_hash.as_deref() == Some(&content_hash) {
                return Ok(None);
            }

            if db
                .entry_exists_by_hash(&content_hash)
                .map_err(|e| e.to_string())?
            {
                self.last_image_hash = Some(content_hash);
                return Ok(None);
            }

            let (width, height) = (image_data.width as u32, image_data.height as u32);

            let webp_bytes = img::rgba_to_webp(rgba, width, height, 80.0)?;

            if webp_bytes.len() > max_entry_size {
                return Ok(None);
            }

            let entry = NewEntry {
                content: webp_bytes.clone(),
                content_type: EntryType::Image,
                content_hash: content_hash.clone(),
                size_bytes: webp_bytes.len() as i64,
                is_favorite: false,
                is_sensitive: false,
                source_app,
            };

            let id = db.insert_entry(&entry).map_err(|e| e.to_string())?;
            self.last_image_hash = Some(content_hash);

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
