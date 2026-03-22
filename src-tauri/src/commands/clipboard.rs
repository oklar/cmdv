use crate::crypto::keys::VaultState;
use crate::db::{Database, EntryType};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
pub struct EntryView {
    pub id: String,
    pub content_type: String,
    pub last_used_at: String,
    pub is_favorite: bool,
    pub size_bytes: i64,
    pub preview: Option<String>,
}

#[derive(Serialize)]
pub struct StatsView {
    pub total_entries: i64,
    pub total_size_bytes: i64,
    pub max_size_bytes: i64,
}

fn make_preview(content: &[u8], content_type: &EntryType) -> Option<String> {
    match content_type {
        EntryType::Text => String::from_utf8(content.to_vec()).ok().map(|s| {
            s.chars().take(200).collect::<String>()
        }),
        EntryType::Image => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(content);
            Some(format!("data:image/webp;base64,{}", b64))
        }
    }
}

#[tauri::command]
pub fn get_entries(
    db: State<'_, Arc<Database>>,
    vault: State<'_, Arc<VaultState>>,
    limit: Option<usize>,
    offset: Option<usize>,
    content_type: Option<String>,
    favorites_only: Option<bool>,
) -> Result<Vec<EntryView>, String> {
    vault
        .keys
        .lock()
        .map_err(|_| "Lock poisoned")?
        .as_ref()
        .ok_or("Vault is locked")?;

    let entry_type = content_type.map(|t| EntryType::from_str(&t));
    let entries = db
        .get_entries(
            limit.unwrap_or(50),
            offset.unwrap_or(0),
            entry_type,
            favorites_only.unwrap_or(false),
        )
        .map_err(|e| e.to_string())?;

    Ok(entries
        .into_iter()
        .map(|e| {
            let preview = make_preview(&e.content, &e.content_type);
            EntryView {
                id: e.id,
                content_type: e.content_type.as_str().to_string(),
                last_used_at: e.last_used_at,
                is_favorite: e.is_favorite,
                size_bytes: e.size_bytes,
                preview,
            }
        })
        .collect())
}

#[tauri::command]
pub fn search_entries(
    db: State<'_, Arc<Database>>,
    vault: State<'_, Arc<VaultState>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<EntryView>, String> {
    vault
        .keys
        .lock()
        .map_err(|_| "Lock poisoned")?
        .as_ref()
        .ok_or("Vault is locked")?;

    let entries = db
        .search_entries(&query, limit.unwrap_or(20))
        .map_err(|e| e.to_string())?;

    Ok(entries
        .into_iter()
        .map(|e| {
            let preview = make_preview(&e.content, &e.content_type);
            EntryView {
                id: e.id,
                content_type: e.content_type.as_str().to_string(),
                last_used_at: e.last_used_at,
                is_favorite: e.is_favorite,
                size_bytes: e.size_bytes,
                preview,
            }
        })
        .collect())
}

#[tauri::command]
pub fn toggle_favorite(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.toggle_favorite(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_entry(db: State<'_, Arc<Database>>, id: String) -> Result<(), String> {
    db.delete_entry(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_all_entries(db: State<'_, Arc<Database>>) -> Result<(), String> {
    db.wipe_all().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_stats(db: State<'_, Arc<Database>>) -> Result<StatsView, String> {
    let total_entries = db.get_entry_count().map_err(|e| e.to_string())?;
    let total_size_bytes = db.get_total_size().map_err(|e| e.to_string())?;
    Ok(StatsView {
        total_entries,
        total_size_bytes,
        max_size_bytes: 50 * 1024 * 1024,
    })
}

#[tauri::command]
pub fn copy_entry_to_clipboard(
    id: String,
    db: State<'_, Arc<Database>>,
    vault: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    vault
        .keys
        .lock()
        .map_err(|_| "Lock poisoned")?
        .as_ref()
        .ok_or("Vault is locked")?;

    let entry = db
        .get_entry(&id)
        .map_err(|e| e.to_string())?
        .ok_or("Entry not found")?;

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;

    match entry.content_type {
        EntryType::Text => {
            let text = String::from_utf8(entry.content).map_err(|e| e.to_string())?;
            clipboard.set_text(text).map_err(|e| e.to_string())?;
        }
        EntryType::Image => {
            let (rgba, width, height) =
                crate::image::decode_to_rgba(&entry.content).map_err(|e| e.to_string())?;
            let img_data = arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: std::borrow::Cow::Owned(rgba),
            };
            clipboard.set_image(img_data).map_err(|e| e.to_string())?;
        }
    }

    db.touch_entry(&id).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_preview_truncates_utf8_at_char_boundary() {
        // Text where byte 200 would split multi-byte 'ệ' (bytes 199..202) - previously panicked
        let text = "Latviski Lietuviškai македонски Melayu Norsk Polski Português Româna Pyccкий Српски Slovenčina Slovenščina Español Svenska ไทย Türkçe Українська Tiếng Việt Lorem Ipsum";
        let content = text.as_bytes();
        let preview = make_preview(content, &EntryType::Text);
        assert!(preview.is_some());
        let p = preview.unwrap();
        assert!(p.chars().count() <= 200);
        assert!(p.is_char_boundary(p.len()) || p.len() == 0);
    }

    #[test]
    fn make_preview_short_text_unchanged() {
        let text = "hello";
        let content = text.as_bytes();
        let preview = make_preview(content, &EntryType::Text);
        assert_eq!(preview.as_deref(), Some("hello"));
    }

    #[test]
    fn make_preview_long_ascii_truncates_to_200_chars() {
        let text = "a".repeat(300);
        let content = text.as_bytes();
        let preview = make_preview(content, &EntryType::Text);
        assert_eq!(preview.as_ref().map(|s| s.len()), Some(200));
    }
}
