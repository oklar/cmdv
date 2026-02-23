use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SyncState {
    pub is_syncing: bool,
    pub last_sync_at: Option<String>,
    pub syncs_remaining: Option<i32>,
    pub error: Option<String>,
}

// Sync commands will be implemented in Phase 4 when cloud sync is built
