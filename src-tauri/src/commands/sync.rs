use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use zeroize::Zeroize;

use crate::commands::account_auth;
use crate::commands::secure_paste::api_base_url;
use crate::crypto::keys::{MasterKey, VaultState};
use crate::db::settings::SettingsDb;
use crate::db::Database;
use crate::storage::keychain::KeychainStore;
use crate::storage::SeedStore;
use crate::sync::client::{AccessTokenSource, SyncClient, TokenFuture};
use crate::sync::engine::{SyncEngine, SyncError, SyncMode};

const DEBOUNCE: Duration = Duration::from_secs(1);
const LAST_SYNCED_KEY: &str = "sync_last_synced_at";
const PINNED_LIMIT_MESSAGE: &str = "Pinned entries can't add up to more than 50 MB.";

/// Whether the server lets this account sync (Admin for now). Cached so changes on a device
/// that can't sync don't each call the API.
const ENT_UNKNOWN: u8 = 0;
const ENT_YES: u8 = 1;
const ENT_NO: u8 = 2;

static SCHEDULER: OnceLock<Arc<SyncScheduler>> = OnceLock::new();

pub struct SyncScheduler {
    notify: Notify,
    fetch_requested: AtomicBool,
    running: AtomicBool,
    entitled: AtomicU8,
    last_error: Mutex<Option<String>>,
}

impl SyncScheduler {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            fetch_requested: AtomicBool::new(false),
            running: AtomicBool::new(false),
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
    /// `Some(true)` can sync, `Some(false)` can't, `None` not checked yet.
    pub entitled: Option<bool>,
    pub syncing: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

/// Uploads this device's changes after a short pause, so a burst goes in one request.
pub fn trigger_sync() {
    if let Some(scheduler) = SCHEDULER.get() {
        scheduler.notify.notify_one();
    }
}

/// Uploads, then fetches other devices' changes. Used at unlock, when the window opens and
/// from "Sync now".
pub fn request_fetch() {
    if let Some(scheduler) = SCHEDULER.get() {
        scheduler.fetch_requested.store(true, Ordering::SeqCst);
        scheduler.notify.notify_one();
    }
}

/// Runs every sync from one loop, so two never overlap. A request that arrives during a run
/// leaves a permit, and the loop runs again after.
pub fn spawn_scheduler(app: AppHandle, scheduler: Arc<SyncScheduler>) {
    let _ = SCHEDULER.set(scheduler.clone());

    tauri::async_runtime::spawn(async move {
        loop {
            scheduler.notify.notified().await;

            if !scheduler.fetch_requested.load(Ordering::SeqCst) {
                loop {
                    tokio::select! {
                        _ = scheduler.notify.notified() => {
                            if scheduler.fetch_requested.load(Ordering::SeqCst) {
                                break;
                            }
                        }
                        _ = tokio::time::sleep(DEBOUNCE) => break,
                    }
                }
            }

            let mode = if scheduler.fetch_requested.swap(false, Ordering::SeqCst) {
                SyncMode::Full
            } else {
                SyncMode::Upload
            };
            if mode == SyncMode::Upload && scheduler.entitled.load(Ordering::SeqCst) == ENT_NO {
                continue;
            }
            run_sync(&app, &scheduler, mode).await;
        }
    });
}

#[tauri::command]
pub fn sync_now(app: AppHandle) -> SyncStatusView {
    request_fetch();
    let scheduler = app.state::<Arc<SyncScheduler>>().inner().clone();
    build_view(&app, &scheduler, true)
}

#[tauri::command]
pub fn sync_on_window_open() {
    request_fetch();
}

#[tauri::command]
pub fn get_sync_status(app: AppHandle) -> SyncStatusView {
    let scheduler = app.state::<Arc<SyncScheduler>>().inner().clone();
    let syncing = scheduler.running.load(Ordering::SeqCst);
    build_view(&app, &scheduler, syncing)
}

async fn run_sync(app: &AppHandle, scheduler: &SyncScheduler, mode: SyncMode) {
    let unlocked = app
        .state::<Arc<VaultState>>()
        .keys
        .lock()
        .map(|keys| keys.is_some())
        .unwrap_or(false);
    if !unlocked {
        return;
    }
    if !matches!(
        KeychainStore::new().load_account_refresh_token(),
        Ok(Some(_))
    ) {
        // Logged out: forget entitlement so a later login checks with the server again.
        scheduler.entitled.store(ENT_UNKNOWN, Ordering::SeqCst);
        return;
    }

    scheduler.running.store(true, Ordering::SeqCst);
    emit(app, scheduler, true);

    let db = app.state::<Arc<Database>>().inner().clone();
    let result = match master_key() {
        Ok(master) => {
            let client = SyncClient::new(api_base_url(), Arc::new(AppTokens(app.clone())));
            SyncEngine::new(db.clone(), client).run(&master, mode).await
        }
        Err(e) => Err(SyncError::Local(e)),
    };

    let error = match result {
        Ok(report) => {
            scheduler.entitled.store(ENT_YES, Ordering::SeqCst);
            if report.applied > 0 {
                let max_total = app
                    .state::<Arc<SettingsDb>>()
                    .get_settings()
                    .max_total_size_bytes;
                crate::commands::vault::enforce_storage_limit(&db, max_total);
            }
            persist_last_synced(app);
            report
                .pinned_limit
                .then(|| PINNED_LIMIT_MESSAGE.to_string())
        }
        Err(e) => {
            if matches!(e, SyncError::NotEntitled) {
                scheduler.entitled.store(ENT_NO, Ordering::SeqCst);
            }
            log::warn!("Sync failed: {e}");
            Some(e.to_string())
        }
    };
    if let Ok(mut last_error) = scheduler.last_error.lock() {
        *last_error = error;
    }

    scheduler.running.store(false, Ordering::SeqCst);
    emit(app, scheduler, false);
}

fn master_key() -> Result<MasterKey, String> {
    let mut seed = KeychainStore::new().load_seed()?;
    if seed.len() < 32 {
        seed.zeroize();
        return Err("The recovery key in the keychain is damaged.".into());
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&seed[..32]);
    seed.zeroize();
    let master = MasterKey::from_bytes(bytes);
    bytes.zeroize();
    Ok(master)
}

struct AppTokens(AppHandle);

impl AccessTokenSource for AppTokens {
    fn access_token(&self, refresh: bool) -> TokenFuture<'_> {
        Box::pin(async move {
            if refresh {
                account_auth::refresh_session(&self.0).await
            } else {
                account_auth::ensure_access_token(&self.0).await
            }
        })
    }
}

fn persist_last_synced(app: &AppHandle) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default();
    let _ = app
        .state::<Arc<SettingsDb>>()
        .set_value(LAST_SYNCED_KEY, &millis);
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
