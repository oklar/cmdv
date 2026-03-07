use crate::crypto::keys::VaultState;
use crate::db::{Database, EntryType};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
pub struct EntryView {
    pub id: String,
    pub content_type: String,
    pub created_at: String,
    pub is_favorite: bool,
    pub is_sensitive: bool,
    pub size_bytes: i64,
    pub source_app: Option<String>,
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
            if s.len() > 200 {
                s[..200].to_string()
            } else {
                s
            }
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
    vault.keys.lock().map_err(|_| "Lock poisoned")?
        .as_ref().ok_or("Vault is locked")?;

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
                created_at: e.created_at,
                is_favorite: e.is_favorite,
                is_sensitive: e.is_sensitive,
                size_bytes: e.size_bytes,
                source_app: e.source_app,
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
    vault.keys.lock().map_err(|_| "Lock poisoned")?
        .as_ref().ok_or("Vault is locked")?;

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
                created_at: e.created_at,
                is_favorite: e.is_favorite,
                is_sensitive: e.is_sensitive,
                size_bytes: e.size_bytes,
                source_app: e.source_app,
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
