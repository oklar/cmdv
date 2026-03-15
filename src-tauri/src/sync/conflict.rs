use crate::db::ClipboardEntry;
use crate::sync::blob::SyncEntry;
use std::collections::HashMap;

pub fn merge_entries(local: &[ClipboardEntry], remote: &[SyncEntry]) -> Vec<SyncEntry> {
    let mut merged: HashMap<String, SyncEntry> = HashMap::new();

    for entry in remote {
        merged.insert(entry.id.clone(), (*entry).clone());
    }

    for entry in local {
        let sync_entry = SyncEntry::from(entry);
        merged
            .entry(entry.id.clone())
            .and_modify(|existing| {
                if entry.is_favorite && !existing.is_favorite {
                    *existing = sync_entry.clone();
                }
                if entry.last_used_at > existing.last_used_at {
                    let was_fav = existing.is_favorite;
                    *existing = sync_entry.clone();
                    if was_fav {
                        existing.is_favorite = true;
                    }
                }
            })
            .or_insert(sync_entry);
    }

    let mut result: Vec<SyncEntry> = merged.into_values().collect();
    result.sort_by(|a, b| a.last_used_at.cmp(&b.last_used_at));
    result
}
