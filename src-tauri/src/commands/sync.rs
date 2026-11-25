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
    /// Another device changed the recovery phrase; uploads wait until the new one is entered.
    needs_new_phrase: AtomicBool,
    last_error: Mutex<Option<String>>,
    /// Held by a sync run and by a phrase change, so they never overlap.
    exclusive: tokio::sync::Mutex<()>,
}

impl SyncScheduler {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            fetch_requested: AtomicBool::new(false),
            running: AtomicBool::new(false),
            entitled: AtomicU8::new(ENT_UNKNOWN),
            needs_new_phrase: AtomicBool::new(false),
            last_error: Mutex::new(None),
            exclusive: tokio::sync::Mutex::new(()),
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
    /// Another device changed the recovery phrase; this one needs the new phrase to sync.
    pub needs_new_phrase: bool,
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
            if mode == SyncMode::Upload
                && (scheduler.entitled.load(Ordering::SeqCst) == ENT_NO
                    || scheduler.needs_new_phrase.load(Ordering::SeqCst))
            {
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

    let _exclusive = scheduler.exclusive.lock().await;
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
            scheduler.needs_new_phrase.store(false, Ordering::SeqCst);
            if let Some(hash_key) = &report.new_hash_key {
                use_hash_key(app, hash_key);
            }
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
            match e {
                SyncError::NotEntitled => scheduler.entitled.store(ENT_NO, Ordering::SeqCst),
                SyncError::KeyVersionBehind => {
                    scheduler.needs_new_phrase.store(true, Ordering::SeqCst)
                }
                _ => {}
            }
            log::warn!("Sync failed: {e}");
            Some(e.to_string())
        }
    };
    set_error(scheduler, error);

    scheduler.running.store(false, Ordering::SeqCst);
    emit(app, scheduler, false);
}

/// Finishes a phrase change started with `begin_phrase_change`, once the user has saved the
/// new phrase. The server gets every entry again under the new phrase first. Then this device
/// moves to it. If the app stops in between, this device asks for the saved phrase like the
/// others do.
#[tauri::command]
pub async fn finish_phrase_change(app: AppHandle) -> Result<(), String> {
    let scheduler = app.state::<Arc<SyncScheduler>>().inner().clone();
    let vault = app.state::<Arc<VaultState>>().inner().clone();
    let _exclusive = scheduler.exclusive.lock().await;

    let new_master = vault
        .pending_phrase
        .lock()
        .map_err(|_| "Lock poisoned")?
        .as_ref()
        .map(|master| MasterKey::from_bytes(*master.as_bytes()))
        .ok_or("No phrase change was started.")?;
    let mut hash_key = current_hash_key(&vault)?;

    let engine = engine(&app);
    let result = async {
        let rotation = engine
            .rotate(&new_master, hash_key)
            .await
            .map_err(|e| e.to_string())?;
        switch_phrase(&app, &new_master, &hash_key)?;
        engine
            .finish_rotation(&rotation, &hash_key)
            .map_err(|e| e.to_string())
    }
    .await;
    hash_key.zeroize();
    result?;

    *vault.pending_phrase.lock().map_err(|_| "Lock poisoned")? = None;
    scheduler.needs_new_phrase.store(false, Ordering::SeqCst);
    set_error(&scheduler, None);
    emit(&app, &scheduler, false);
    // Changes made while the phrase changed are still queued.
    trigger_sync();
    Ok(())
}

/// Moves this device to a phrase changed on another device: checks it opens the account's
/// key, then re-encrypts the database under it. The next sync does the restore steps.
#[tauri::command]
pub async fn enter_new_phrase(app: AppHandle, mnemonic_words: String) -> Result<(), String> {
    let new_master = crate::commands::vault::master_from_phrase(&mnemonic_words)?;
    let scheduler = app.state::<Arc<SyncScheduler>>().inner().clone();
    let vault = app.state::<Arc<VaultState>>().inner().clone();
    let _exclusive = scheduler.exclusive.lock().await;

    let engine = engine(&app);
    let check = engine
        .open_with_phrase(&new_master)
        .await
        .map_err(|e| e.to_string())?;
    let mut previous_hash_key = current_hash_key(&vault)?;
    let result = switch_phrase(&app, &new_master, check.hash_key()).and_then(|()| {
        engine
            .adopt_phrase(&check, &previous_hash_key)
            .map_err(|e| e.to_string())
    });
    let hash_key_changed = &previous_hash_key != check.hash_key();
    previous_hash_key.zeroize();
    result?;
    if hash_key_changed {
        use_hash_key(&app, check.hash_key());
    }

    scheduler.needs_new_phrase.store(false, Ordering::SeqCst);
    set_error(&scheduler, None);
    emit(&app, &scheduler, false);
    request_fetch();
    Ok(())
}

/// Moves this device to `master`'s phrase: re-encrypts the database under its key, then saves
/// it in the keychain. The hash key is stored first, since it doesn't depend on the phrase.
fn switch_phrase(app: &AppHandle, master: &MasterKey, hash_key: &[u8; 32]) -> Result<(), String> {
    let db = app.state::<Arc<Database>>();
    db.store_hash_key(hash_key).map_err(|e| e.to_string())?;

    let mut db_key = master.derive_db_key();
    let switched = db
        .rekey(&db_key)
        .and_then(|()| KeychainStore::new().save_seed(master.as_bytes()));
    if switched.is_ok() {
        if let Ok(mut keys) = app.state::<Arc<VaultState>>().keys.lock() {
            if let Some(keys) = keys.as_mut() {
                keys.db_key = db_key;
                keys.hash_key = *hash_key;
            }
        }
    }
    db_key.zeroize();
    switched
}

/// Starts hashing new copies with `hash_key`, as the account's other devices do.
fn use_hash_key(app: &AppHandle, hash_key: &[u8; 32]) {
    let vault = app.state::<Arc<VaultState>>();
    if let Ok(mut keys) = vault.keys.lock() {
        if let Some(keys) = keys.as_mut() {
            keys.hash_key = *hash_key;
        }
    }
    let db = app.state::<Arc<Database>>().inner().clone();
    let settings_db = app.state::<Arc<SettingsDb>>().inner().clone();
    crate::commands::vault::start_monitoring(&vault, &db, &settings_db);
}

fn current_hash_key(vault: &VaultState) -> Result<[u8; 32], String> {
    let keys = vault.keys.lock().map_err(|_| "Lock poisoned")?;
    Ok(keys.as_ref().ok_or("Vault is locked")?.hash_key)
}

fn engine(app: &AppHandle) -> SyncEngine {
    let db = app.state::<Arc<Database>>().inner().clone();
    SyncEngine::new(
        db,
        SyncClient::new(api_base_url(), Arc::new(AppTokens(app.clone()))),
    )
}

fn set_error(scheduler: &SyncScheduler, error: Option<String>) {
    if let Ok(mut last_error) = scheduler.last_error.lock() {
        *last_error = error;
    }
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
        needs_new_phrase: scheduler.needs_new_phrase.load(Ordering::SeqCst),
    }
}

fn emit(app: &AppHandle, scheduler: &SyncScheduler, syncing: bool) {
    let _ = app.emit("sync-status", build_view(app, scheduler, syncing));
}
