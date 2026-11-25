//! One sync run: set up or check the account's key, upload the outbox, then fetch what other
//! devices changed. It needs only the database, the recovery phrase's master key and a client,
//! so a test can run two devices in one process.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::crypto::keys::MasterKey;
use crate::crypto::sync_keys::{ItemKeys, KeyBundle};
use crate::db::sync_state::{
    self, META_GENERATION_ID, META_KEY_BUNDLE, META_KEY_VERSION, META_LAST_SEQ,
};
use crate::db::Database;
use crate::sync::client::{
    ApiError, KeyState, SyncClient, DAILY_QUOTA_TYPE, PINNED_LIMIT_TYPE, STALE_KEY_VERSION_TYPE,
};
use crate::sync::item::{self, EntryPayload, SyncItem};

/// The server's per-item limit (5 MiB + 4 KiB). Bigger entries stay on this device.
const MAX_ITEM_BYTES: usize = 5 * 1024 * 1024 + 4 * 1024;
/// The server takes 8 MiB per request, base64 included. The margin covers the JSON around items.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024 - 64 * 1024;
const PAGE_ITEMS: u32 = 500;
const OUTBOX_READ: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SyncMode {
    /// Upload the outbox only, after a change on this device.
    Upload,
    /// Upload, then fetch other devices' changes.
    Full,
}

#[derive(Default, Debug)]
pub struct SyncReport {
    pub uploaded: usize,
    pub applied: usize,
    /// Items that didn't decrypt with this account's keys, so were ignored.
    pub skipped: usize,
    /// The server refused pins past its limit, and those entries were un-pinned here.
    pub pinned_limit: bool,
    /// The server was restored or started over, so this device sent its recent deletes again
    /// and fetched from the start.
    pub restored: bool,
}

#[derive(Debug)]
pub enum SyncError {
    NotEntitled,
    DifferentPhrase,
    KeyChanged,
    QuotaReached,
    RateLimited,
    ServerChanged,
    Auth(String),
    Network(String),
    Server(String),
    Local(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::NotEntitled => write!(f, "Cloud sync is a paid feature."),
            SyncError::DifferentPhrase => {
                write!(f, "This account syncs with a different recovery phrase.")
            }
            SyncError::KeyChanged => {
                write!(f, "The account's sync key changed on another device.")
            }
            SyncError::QuotaReached => {
                write!(f, "Daily sync limit reached. Sync continues tomorrow.")
            }
            SyncError::RateLimited => write!(f, "Too many sync requests. Try again in a minute."),
            SyncError::ServerChanged => {
                write!(
                    f,
                    "The server changed during sync. It catches up on the next sync."
                )
            }
            SyncError::Auth(message) => write!(f, "{message}"),
            SyncError::Network(message) => write!(f, "Network error: {message}"),
            SyncError::Server(message) => write!(f, "Sync failed: {message}"),
            SyncError::Local(message) => write!(f, "Sync failed: {message}"),
        }
    }
}

impl From<rusqlite::Error> for SyncError {
    fn from(e: rusqlite::Error) -> Self {
        SyncError::Local(e.to_string())
    }
}

impl From<ApiError> for SyncError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Network(message) => SyncError::Network(message),
            ApiError::Auth(message) => SyncError::Auth(message),
            ApiError::Invalid(message) => SyncError::Server(message),
            ApiError::Status { status: 403, .. } => SyncError::NotEntitled,
            ApiError::Status {
                status: 429,
                problem_type,
            } if problem_type.as_deref() == Some(DAILY_QUOTA_TYPE) => SyncError::QuotaReached,
            ApiError::Status { status: 429, .. } => SyncError::RateLimited,
            ApiError::Status {
                status: 409,
                problem_type,
            } if problem_type.as_deref() == Some(STALE_KEY_VERSION_TYPE) => SyncError::KeyChanged,
            ApiError::Status { status, .. } => {
                SyncError::Server(format!("server answered {status}"))
            }
        }
    }
}

pub struct SyncEngine {
    db: Arc<Database>,
    client: SyncClient,
}

impl SyncEngine {
    pub fn new(db: Arc<Database>, client: SyncClient) -> Self {
        Self { db, client }
    }

    pub async fn run(&self, master: &MasterKey, mode: SyncMode) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        let (keys, server) = self.ensure_key(master).await?;

        let stored_generation: Option<String> = self
            .db
            .with_conn(|c| sync_state::meta(c, META_GENERATION_ID))?;
        let mut last_seq: i64 = self
            .db
            .with_conn(|c| sync_state::meta(c, META_LAST_SEQ))?
            .unwrap_or(0);
        let first = stored_generation.is_none();
        // A restored file can keep its old generation id, but its latest seq then falls behind
        // what this device has already seen.
        let restored = !first
            && (stored_generation.as_deref() != Some(server.generation_id.as_str())
                || server.latest_seq < last_seq);
        report.restored = restored;

        self.db.with_conn(|c| {
            sync_state::prune_recent_deletes(c)?;
            for (entry_id, hash) in sync_state::entries_without_sync_id(c)? {
                sync_state::set_sync_id(c, &entry_id, &item::item_id(&keys, &hash))?;
            }
            if first {
                sync_state::queue_all_entries(c)?;
            }
            if restored {
                sync_state::queue_recent_deletes(c)?;
            }
            Ok(())
        })?;

        if first || restored {
            last_seq = 0;
        }
        self.upload(
            &keys,
            &server.generation_id,
            &mut last_seq,
            !restored,
            &mut report,
        )
        .await?;

        if mode == SyncMode::Full || first || restored {
            self.fetch(
                &keys,
                &server.generation_id,
                last_seq,
                restored,
                &mut report,
            )
            .await?;
        }

        if first || restored {
            self.db.with_conn(|c| {
                sync_state::set_meta(c, META_GENERATION_ID, server.generation_id.as_str())
            })?;
        }
        Ok(report)
    }

    /// Opens the account's key bundle, storing a new one if the account has none yet.
    async fn ensure_key(&self, master: &MasterKey) -> Result<(ItemKeys, KeyState), SyncError> {
        let mut server = self.client.get_key().await?;
        if server.bundle.is_none() {
            // A device that synced before offers the bundle it has, so a server that lost the
            // account's file keeps the same keys.
            let cached: Option<Vec<u8>> = self
                .db
                .with_conn(|c| sync_state::meta(c, META_KEY_BUNDLE))?;
            let sealed = match cached {
                Some(sealed) => sealed,
                None => KeyBundle::generate(1, master.derive_hash_key())
                    .seal(master)
                    .map_err(SyncError::Local)?,
            };
            server = self.client.put_key(&sealed).await?;
        }

        let sealed = server
            .bundle
            .clone()
            .ok_or_else(|| SyncError::Server("no sync key".into()))?;
        let bundle = KeyBundle::open(master, &sealed).map_err(|_| SyncError::DifferentPhrase)?;
        if bundle.version() != server.key_version {
            return Err(SyncError::KeyChanged);
        }

        self.db.with_conn(|c| {
            sync_state::set_meta(c, META_KEY_BUNDLE, &sealed)?;
            sync_state::set_meta(c, META_KEY_VERSION, server.key_version)
        })?;
        Ok((bundle.item_keys(), server))
    }

    /// Uploads the outbox in order. With `advance`, moves `last_seq` past this device's own
    /// uploads when nothing else was saved in between, so it doesn't download them again.
    async fn upload(
        &self,
        keys: &ItemKeys,
        generation: &str,
        last_seq: &mut i64,
        advance: bool,
        report: &mut SyncReport,
    ) -> Result<(), SyncError> {
        loop {
            let rows = self
                .db
                .with_conn(|c| sync_state::outbox_rows(c, OUTBOX_READ))?;
            if rows.is_empty() {
                return Ok(());
            }

            let mut batch: Vec<SyncItem> = Vec::new();
            let mut sent: Vec<(i64, Vec<u8>)> = Vec::new();
            let mut dropped: Vec<i64> = Vec::new();
            let mut body_bytes = 0;

            for row in rows {
                let item = if row.is_delete {
                    item::encrypt_delete(keys, &row.content_hash)
                } else {
                    match self
                        .db
                        .with_conn(|c| sync_state::entry_for_upload(c, &row.content_hash))?
                    {
                        Some(entry) => item::encrypt_entry(
                            keys,
                            &EntryPayload {
                                content_type: entry.content_type,
                                content: entry.content,
                                content_hash: entry.content_hash,
                                is_favorite: entry.is_favorite,
                                last_used_at: entry.last_used_at,
                            },
                        ),
                        None => {
                            dropped.push(row.id);
                            continue;
                        }
                    }
                }
                .map_err(SyncError::Local)?;

                if item.ciphertext.len() > MAX_ITEM_BYTES {
                    log::warn!("Sync: an entry over the server's size limit stays on this device");
                    dropped.push(row.id);
                    continue;
                }
                let item_bytes = item.ciphertext.len().div_ceil(3) * 4 + 256;
                if !batch.is_empty() && body_bytes + item_bytes > MAX_BODY_BYTES {
                    break;
                }
                body_bytes += item_bytes;
                sent.push((row.id, row.content_hash));
                batch.push(item);
            }

            if !dropped.is_empty() {
                self.db
                    .with_conn(|c| sync_state::remove_outbox_rows(c, &dropped))?;
            }
            if batch.is_empty() {
                continue;
            }

            match self.client.post_items(&batch).await {
                Ok(saved) => {
                    if saved.generation_id != generation {
                        return Err(SyncError::ServerChanged);
                    }
                    let ids: Vec<i64> = sent.iter().map(|(id, _)| *id).collect();
                    self.db
                        .with_conn(|c| sync_state::remove_outbox_rows(c, &ids))?;
                    report.uploaded += batch.len();

                    if advance && saved.seqs.first() == Some(&(*last_seq + 1)) {
                        *last_seq = saved.latest_seq;
                        self.db
                            .with_conn(|c| sync_state::set_meta(c, META_LAST_SEQ, *last_seq))?;
                    }
                }
                Err(ApiError::Status {
                    status: 413,
                    problem_type,
                }) if problem_type.as_deref() == Some(PINNED_LIMIT_TYPE) => {
                    report.pinned_limit = true;
                    self.db.with_conn(|c| {
                        for ((_, hash), item) in sent.iter().zip(&batch) {
                            if item.pinned {
                                sync_state::unpin_and_requeue(c, hash)?;
                            }
                        }
                        Ok(())
                    })?;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Applies other devices' changes after `after`, skipping entries with a newer change
    /// waiting in the outbox. With `insert_only`, entries already here are kept as they are.
    async fn fetch(
        &self,
        keys: &ItemKeys,
        generation: &str,
        mut after: i64,
        insert_only: bool,
        report: &mut SyncReport,
    ) -> Result<(), SyncError> {
        let waiting: HashSet<String> = self
            .db
            .with_conn(sync_state::outbox_hashes)?
            .iter()
            .map(|hash| item::item_id(keys, hash))
            .collect();

        loop {
            let page = self.client.get_changes(after, PAGE_ITEMS).await?;
            if page.generation_id != generation {
                return Err(SyncError::ServerChanged);
            }

            for change in &page.items {
                if waiting.contains(&change.item.id) {
                    continue;
                }
                let applied = match item::decrypt(keys, &change.item) {
                    Ok(Some(entry)) => self.db.with_conn(|c| {
                        sync_state::apply_entry(c, &change.item.id, &entry, insert_only)
                    })?,
                    Ok(None) => self
                        .db
                        .with_conn(|c| sync_state::apply_delete(c, &change.item.id))?,
                    Err(e) => {
                        log::warn!("Sync: skipped an item that didn't decrypt: {e}");
                        report.skipped += 1;
                        false
                    }
                };
                if applied {
                    report.applied += 1;
                }
            }

            after = match page.items.last() {
                Some(last) if page.has_more => last.seq,
                _ => page.latest_seq,
            };
            self.db
                .with_conn(|c| sync_state::set_meta(c, META_LAST_SEQ, after))?;
            if !page.has_more || page.items.is_empty() {
                return Ok(());
            }
        }
    }
}
