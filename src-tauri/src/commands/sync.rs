use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use zeroize::Zeroize;

use crate::commands::account_auth;
use crate::commands::secure_paste::api_base_url;
use crate::crypto::keys::VaultState;
use crate::db::settings::SettingsDb;
use crate::db::Database;
use crate::storage::keychain::KeychainStore;
use crate::sync::blob::SyncEntry;

const MAX_SNAPSHOT_BYTES: i64 = 50 * 1024 * 1024;
const DEBOUNCE: Duration = Duration::from_secs(1);
const MAX_RETRIES: usize = 3;

const LAST_ETAG_KEY: &str = "sync_last_etag";
const LAST_SYNCED_KEY: &str = "sync_last_synced_at";
const SNAPSHOT_FILE: &str = "cmdv.sync.tmp";
const PULL_FILE: &str = "cmdv.pull.tmp";

/// Cached server-side entitlement (paid/Admin). Cloud sync is not a user preference — the server
/// decides. `/sync/status` returns 200 when entitled, 403 when not. We cache the answer so the
/// auto-sync loop doesn't hammer the API for users who cannot sync.
const ENT_UNKNOWN: u8 = 0;
const ENT_YES: u8 = 1;
const ENT_NO: u8 = 2;

/// Set once the debounce loop is spawned, so mutation paths can nudge a sync without threading
/// the scheduler handle through every call site.
static SYNC_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

pub struct SyncScheduler {
    notify: Arc<Notify>,
    running: Arc<AtomicBool>,
    entitled: AtomicU8,
    last_error: Mutex<Option<String>>,
}

impl SyncScheduler {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            running: Arc::new(AtomicBool::new(false)),
            entitled: AtomicU8::new(ENT_UNKNOWN),
            last_error: Mutex::new(None),
        }
    }
}

impl Default for SyncScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Clone)]
pub struct SyncStatusView {
    /// `Some(true)` paid/entitled, `Some(false)` not entitled, `None` not yet checked.
    pub entitled: Option<bool>,
    pub syncing: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Deserialize)]
struct StatusResp {
    #[serde(rename = "hasRemote")]
    has_remote: bool,
    #[serde(rename = "currentEtag")]
    current_etag: Option<String>,
}

#[derive(Deserialize)]
struct PullResp {
    #[serde(rename = "downloadToken")]
    download_token: String,
    #[serde(rename = "workerUrl")]
    worker_url: String,
    #[serde(rename = "currentEtag")]
    current_etag: Option<String>,
}

#[derive(Deserialize)]
struct IntentResp {
    #[serde(rename = "uploadToken")]
    upload_token: String,
    #[serde(rename = "workerUrl")]
    worker_url: String,
}

#[derive(Deserialize)]
struct UploadResp {
    etag: String,
}

/// Nudge the debounced auto-sync loop. No-op until the scheduler is spawned or if nothing is
/// listening yet (Notify stores a single permit for the next wait).
pub fn trigger_sync() {
    if let Some(notify) = SYNC_NOTIFY.get() {
        notify.notify_one();
    }
}

/// Spawn the debounce + single-flight auto-sync loop. Coalesces bursts of mutations into one
/// sync ~1s after the last change. Auto-sync always runs for entitled (paid) users; it is skipped
/// only once the server has told us the user is not entitled, to avoid hammering the API.
pub fn spawn_scheduler(app: AppHandle, scheduler: Arc<SyncScheduler>) {
    let _ = SYNC_NOTIFY.set(scheduler.notify.clone());

    tauri::async_runtime::spawn(async move {
        loop {
            scheduler.notify.notified().await;

            // Debounce: keep resetting the timer while changes keep arriving.
            loop {
                tokio::select! {
                    _ = scheduler.notify.notified() => continue,
                    _ = tokio::time::sleep(DEBOUNCE) => break,
                }
            }

            if scheduler.entitled.load(Ordering::SeqCst) == ENT_NO {
                continue;
            }

            if let Err(e) = run_sync(app.clone(), scheduler.clone()).await {
                log::warn!("Auto-sync failed: {e}");
            }
        }
    });
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<SyncStatusView, String> {
    let scheduler = app.state::<Arc<SyncScheduler>>().inner().clone();
    run_sync(app.clone(), scheduler.clone()).await?;
    Ok(build_view(&app, &scheduler, false))
}

#[tauri::command]
pub fn get_sync_status(app: AppHandle) -> Result<SyncStatusView, String> {
    let scheduler = app.state::<Arc<SyncScheduler>>().inner().clone();
    let syncing = scheduler.running.load(Ordering::SeqCst);
    Ok(build_view(&app, &scheduler, syncing))
}

struct RunGuard(Arc<AtomicBool>);

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub async fn run_sync(app: AppHandle, scheduler: Arc<SyncScheduler>) -> Result<(), String> {
    if scheduler
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Sync already in progress.".into());
    }
    let _guard = RunGuard(scheduler.running.clone());

    {
        let vault = app.state::<Arc<VaultState>>();
        if vault.keys.lock().map_err(|_| "Lock poisoned")?.is_none() {
            return Err("Vault is locked.".into());
        }
    }
    if KeychainStore::new().load_account_refresh_token()?.is_none() {
        // Logged out: forget entitlement so a later login re-checks against the server.
        scheduler.entitled.store(ENT_UNKNOWN, Ordering::SeqCst);
        return Err("Not logged in.".into());
    }

    set_error(&scheduler, None)?;
    emit(&app, &scheduler, true);

    let result = do_sync(&app, &scheduler).await;

    match &result {
        Ok(()) => {
            persist_last_synced(&app);
            set_error(&scheduler, None)?;
        }
        Err(e) => set_error(&scheduler, Some(e.clone()))?,
    }
    emit(&app, &scheduler, false);
    result
}

async fn do_sync(app: &AppHandle, scheduler: &SyncScheduler) -> Result<(), String> {
    let db = app.state::<Arc<Database>>().inner().clone();
    let vault = app.state::<Arc<VaultState>>().inner().clone();
    let settings_db = app.state::<Arc<SettingsDb>>().inner().clone();

    let api = api_base_url();

    let res = account_auth::authed_get(app, &format!("{api}/sync/status")).await?;
    if res.status() == reqwest::StatusCode::FORBIDDEN {
        scheduler.entitled.store(ENT_NO, Ordering::SeqCst);
        return Err("Cloud sync is a paid feature.".into());
    }
    if !res.status().is_success() {
        return Err(format!("Sync status failed ({}).", res.status()));
    }
    scheduler.entitled.store(ENT_YES, Ordering::SeqCst);
    let status: StatusResp = res.json().await.map_err(|e| e.to_string())?;

    let last_etag = settings_db.get_value(LAST_ETAG_KEY);

    let base_etag = if status.has_remote && status.current_etag.as_deref() != last_etag.as_deref() {
        pull_and_merge(app, &db, &vault, &settings_db).await?
    } else {
        status.current_etag
    };

    push_with_retries(app, &db, &vault, &settings_db, base_etag).await
}

async fn pull_and_merge(
    app: &AppHandle,
    db: &Database,
    vault: &VaultState,
    settings_db: &SettingsDb,
) -> Result<Option<String>, String> {
    let api = api_base_url();
    let res = account_auth::authed_post_json(app, &format!("{api}/sync/pull"), &()).await?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if res.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("Cloud sync is a paid feature.".into());
    }
    if !res.status().is_success() {
        return Err(format!("Pull failed ({}).", res.status()));
    }
    let pull: PullResp = res.json().await.map_err(|e| e.to_string())?;

    let mut token = pull.download_token;
    let download = account_auth::http()
        .get(format!("{}/download", pull.worker_url.trim_end_matches('/')))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"));
    token.zeroize();
    let download = download?;

    if !download.status().is_success() {
        return Err(format!("Download failed ({}).", download.status()));
    }
    let bytes = download.bytes().await.map_err(|e| e.to_string())?;

    merge_downloaded(app, db, vault, settings_db, &bytes)?;
    Ok(pull.current_etag)
}

fn merge_downloaded(
    app: &AppHandle,
    db: &Database,
    vault: &VaultState,
    settings_db: &SettingsDb,
    bytes: &[u8],
) -> Result<(), String> {
    let mut db_key = db_key(vault)?;
    let path = snapshot_dir(app)?.join(PULL_FILE);
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;

    let remote_entries = {
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        let mut hex_key = hex::encode(db_key);
        let mut pragma = format!("PRAGMA key = \"x'{}'\";", hex_key);
        hex_key.zeroize();
        let keyed = conn.execute_batch(&pragma);
        pragma.zeroize();
        keyed.map_err(|e| format!("Failed to open pulled snapshot: {}", e))?;
        crate::db::entries::get_all_entries(&conn).map_err(|e| e.to_string())?
    };
    let _ = std::fs::remove_file(&path);
    db_key.zeroize();

    let remote_sync: Vec<SyncEntry> = remote_entries.iter().map(SyncEntry::from).collect();
    let local = db.get_all_entries().map_err(|e| e.to_string())?;
    let merged = crate::sync::conflict::merge_entries(&local, &remote_sync);

    for entry in &merged {
        if !db
            .entry_exists_by_hash(&entry.content_hash)
            .map_err(|e| e.to_string())?
        {
            let new_entry = crate::db::NewEntry {
                content: entry.content.clone(),
                content_type: crate::db::EntryType::from_str(&entry.content_type),
                content_hash: entry.content_hash.clone(),
                size_bytes: entry.size_bytes,
                is_favorite: entry.is_favorite,
            };
            db.insert_entry(&new_entry).map_err(|e| e.to_string())?;
        }
    }

    let max_total = settings_db.get_settings().max_total_size_bytes;
    crate::commands::vault::enforce_storage_limit(db, max_total);
    Ok(())
}

async fn push_with_retries(
    app: &AppHandle,
    db: &Database,
    vault: &VaultState,
    settings_db: &SettingsDb,
    mut base_etag: Option<String>,
) -> Result<(), String> {
    let api = api_base_url();

    for _ in 0..MAX_RETRIES {
        let (bytes, size, snapshot_path) = build_snapshot(app, db, vault)?;

        if size > MAX_SNAPSHOT_BYTES {
            let _ = std::fs::remove_file(&snapshot_path);
            return Err("Too large to sync (50MB max) - remove some favorites.".into());
        }

        let intent_body = serde_json::json!({ "baseEtag": base_etag, "size": size });
        let res = account_auth::authed_post_json(app, &format!("{api}/sync/push/intent"), &intent_body).await?;
        let code = res.status();

        if code == reqwest::StatusCode::CONFLICT {
            let _ = std::fs::remove_file(&snapshot_path);
            base_etag = pull_and_merge(app, db, vault, settings_db).await?;
            continue;
        }
        if code == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
            let _ = std::fs::remove_file(&snapshot_path);
            return Err("Too large to sync (50MB max) - remove some favorites.".into());
        }
        if code == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let _ = std::fs::remove_file(&snapshot_path);
            return Err("Daily sync limit reached.".into());
        }
        if code == reqwest::StatusCode::FORBIDDEN {
            let _ = std::fs::remove_file(&snapshot_path);
            return Err("Cloud sync is a paid feature.".into());
        }
        if !code.is_success() {
            let _ = std::fs::remove_file(&snapshot_path);
            return Err(format!("Sync intent failed ({code})."));
        }

        let intent: IntentResp = res.json().await.map_err(|e| e.to_string())?;

        let mut token = intent.upload_token;
        let upload = account_auth::http()
            .put(format!("{}/upload", intent.worker_url.trim_end_matches('/')))
            .bearer_auth(&token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("Network error: {e}"));
        token.zeroize();
        let _ = std::fs::remove_file(&snapshot_path);
        let upload = upload?;

        if upload.status() == reqwest::StatusCode::PRECONDITION_FAILED {
            base_etag = pull_and_merge(app, db, vault, settings_db).await?;
            continue;
        }
        if !upload.status().is_success() {
            return Err(format!("Upload failed ({}).", upload.status()));
        }
        let uploaded: UploadResp = upload.json().await.map_err(|e| e.to_string())?;

        let commit_body = serde_json::json!({ "etag": uploaded.etag, "size": size });
        let commit = account_auth::authed_post_json(app, &format!("{api}/sync/push/commit"), &commit_body).await?;
        if !commit.status().is_success() {
            return Err(format!("Commit failed ({}).", commit.status()));
        }

        settings_db.set_value(LAST_ETAG_KEY, &normalize_etag(&uploaded.etag))?;
        return Ok(());
    }

    Err("Sync conflict - please try again.".into())
}

/// Snapshot the live DB and read it into memory. Returns (bytes, size, path-to-clean-up). The
/// caller must remove the snapshot file after the upload completes.
fn build_snapshot(
    app: &AppHandle,
    db: &Database,
    vault: &VaultState,
) -> Result<(Vec<u8>, i64, PathBuf), String> {
    let mut key = db_key(vault)?;
    let path = snapshot_dir(app)?.join(SNAPSHOT_FILE);
    let backup_result = db.backup_to_encrypted(&path, &key);
    key.zeroize();
    backup_result?;

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let size = bytes.len() as i64;
    Ok((bytes, size, path))
}

fn db_key(vault: &VaultState) -> Result<[u8; 32], String> {
    let guard = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    let keys = guard.as_ref().ok_or("Vault is locked")?;
    Ok(keys.db_key)
}

fn snapshot_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn normalize_etag(etag: &str) -> String {
    etag.trim().trim_matches('"').to_string()
}

fn set_error(scheduler: &SyncScheduler, error: Option<String>) -> Result<(), String> {
    *scheduler.last_error.lock().map_err(|_| "Lock poisoned")? = error;
    Ok(())
}

fn persist_last_synced(app: &AppHandle) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default();
    let _ = app.state::<Arc<SettingsDb>>().set_value(LAST_SYNCED_KEY, &millis);
}

fn build_view(app: &AppHandle, scheduler: &SyncScheduler, syncing: bool) -> SyncStatusView {
    let last_synced_at = app.state::<Arc<SettingsDb>>().get_value(LAST_SYNCED_KEY);
    let last_error = scheduler
        .last_error
        .lock()
        .ok()
        .and_then(|guard| guard.clone());

    let entitled = match scheduler.entitled.load(Ordering::SeqCst) {
        ENT_YES => Some(true),
        ENT_NO => Some(false),
        _ => None,
    };

    SyncStatusView {
        entitled,
        syncing,
        last_synced_at,
        last_error,
    }
}

fn emit(app: &AppHandle, scheduler: &SyncScheduler, syncing: bool) {
    let _ = app.emit("sync-status", build_view(app, scheduler, syncing));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{EntryType, NewEntry};
    use std::path::Path;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cmdv-sync-test-{}-{name}", uuid::Uuid::new_v4()))
    }

    fn keyed_db(path: &Path, key: &[u8; 32]) -> Database {
        let db = Database::open_encrypted(path).unwrap();
        db.set_encryption_key(key).unwrap();
        db
    }

    fn open_snapshot(path: &Path, key: &[u8; 32]) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex::encode(key)))
            .unwrap();
        conn
    }

    fn entry(content: &[u8], hash: Vec<u8>) -> NewEntry {
        NewEntry {
            content: content.to_vec(),
            content_type: EntryType::Text,
            content_hash: hash,
            size_bytes: content.len() as i64,
            is_favorite: false,
        }
    }

    #[test]
    fn snapshot_roundtrips_through_encrypted_backup() {
        let key = [42u8; 32];
        let src_path = temp_path("src.db");
        let snap_path = temp_path("snap.db");

        let db = keyed_db(&src_path, &key);
        db.insert_entry(&entry(b"hello", vec![1, 2, 3])).unwrap();
        db.insert_entry(&entry(b"world", vec![4, 5, 6])).unwrap();

        let size = db.backup_to_encrypted(&snap_path, &key).unwrap();
        assert!(size > 0);

        let conn = open_snapshot(&snap_path, &key);
        let entries = crate::db::entries::get_all_entries(&conn).unwrap();
        assert_eq!(entries.len(), 2);
        drop(conn);

        db.close();
        let _ = std::fs::remove_file(&src_path);
        let _ = std::fs::remove_file(&snap_path);
    }

    #[test]
    fn merge_unions_remote_snapshot_into_local() {
        let key = [7u8; 32];
        let local_path = temp_path("local.db");
        let remote_path = temp_path("remote.db");
        let snap_path = temp_path("remote-snap.db");

        let local = keyed_db(&local_path, &key);
        local.insert_entry(&entry(b"local-only", vec![1, 1, 1])).unwrap();
        let shared = entry(b"shared", vec![2, 2, 2]);
        local.insert_entry(&shared).unwrap();

        let remote = keyed_db(&remote_path, &key);
        remote.insert_entry(&entry(b"shared", vec![2, 2, 2])).unwrap();
        remote.insert_entry(&entry(b"remote-only", vec![3, 3, 3])).unwrap();

        remote.backup_to_encrypted(&snap_path, &key).unwrap();

        let conn = open_snapshot(&snap_path, &key);
        let remote_entries = crate::db::entries::get_all_entries(&conn).unwrap();
        drop(conn);

        let remote_sync: Vec<SyncEntry> = remote_entries.iter().map(SyncEntry::from).collect();
        let local_entries = local.get_all_entries().unwrap();
        let merged = crate::sync::conflict::merge_entries(&local_entries, &remote_sync);

        for e in &merged {
            if !local.entry_exists_by_hash(&e.content_hash).unwrap() {
                local
                    .insert_entry(&NewEntry {
                        content: e.content.clone(),
                        content_type: EntryType::from_str(&e.content_type),
                        content_hash: e.content_hash.clone(),
                        size_bytes: e.size_bytes,
                        is_favorite: e.is_favorite,
                    })
                    .unwrap();
            }
        }

        // local-only + shared + remote-only = 3 (shared not duplicated by hash)
        assert_eq!(local.get_entry_count().unwrap(), 3);

        local.close();
        remote.close();
        let _ = std::fs::remove_file(&local_path);
        let _ = std::fs::remove_file(&remote_path);
        let _ = std::fs::remove_file(&snap_path);
    }

    #[test]
    fn normalize_etag_strips_quotes_and_whitespace() {
        assert_eq!(normalize_etag("\"abc123\""), "abc123");
        assert_eq!(normalize_etag("  \"abc\"  "), "abc");
        assert_eq!(normalize_etag("plain"), "plain");
    }
}
