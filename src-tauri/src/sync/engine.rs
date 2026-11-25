//! One sync run: set up or check the account's key, upload the outbox, then fetch what other
//! devices changed. It needs only the database, the recovery phrase's master key and a client,
//! so a test can run two devices in one process.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::crypto::keys::MasterKey;
use crate::crypto::sync_keys::{ItemKeys, KeyBundle};
use crate::db::sync_state::{
    self, META_GENERATION_ID, META_HASH_KEY, META_KEY_BUNDLE, META_KEY_VERSION, META_LAST_SEQ,
};
use crate::db::{ClipboardEntry, Database};
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
    /// The account's hash key, when it differs from the one this device used, as on a device
    /// set up with a phrase from after a rotation. Text entries were hashed again with it.
    pub new_hash_key: Option<[u8; 32]>,
}

#[derive(Debug)]
pub enum SyncError {
    NotEntitled,
    NotSetUp,
    DifferentPhrase,
    /// Another device changed the recovery phrase. This one needs the new phrase to sync.
    KeyVersionBehind,
    /// A phrase entered on this device doesn't open the account's key.
    WrongPhrase,
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
            SyncError::NotSetUp => {
                write!(f, "Sync this device once before changing the recovery phrase.")
            }
            SyncError::DifferentPhrase => {
                write!(f, "This account syncs with a different recovery phrase.")
            }
            SyncError::KeyVersionBehind => write!(
                f,
                "The recovery phrase was changed on another device. Enter the new phrase to keep syncing."
            ),
            SyncError::WrongPhrase => {
                write!(f, "That phrase doesn't open this account's sync key.")
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

/// A phrase rotation the server has committed. `finish_rotation` records it on this device.
pub struct Rotation {
    sealed: Vec<u8>,
    key: KeyState,
    outbox_mark: i64,
}

/// The account's key, opened with a phrase entered on this device.
pub struct PhraseCheck {
    sealed: Vec<u8>,
    key_version: u32,
    hash_key: [u8; 32],
}

impl PhraseCheck {
    pub fn hash_key(&self) -> &[u8; 32] {
        &self.hash_key
    }
}

impl Drop for PhraseCheck {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.hash_key);
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

    /// Uploads every synced entry, and the deletes from the last 30 days, encrypted under a new
    /// phrase and the next key version, and makes them the account's items on the server.
    /// Other devices then need the new phrase. The caller moves this device to the new phrase,
    /// then calls `finish_rotation`.
    pub async fn rotate(
        &self,
        new_master: &MasterKey,
        hash_key: [u8; 32],
    ) -> Result<Rotation, SyncError> {
        let current = self.client.get_key().await?;
        let cached_version: Option<u32> = self
            .db
            .with_conn(|c| sync_state::meta(c, META_KEY_VERSION))?;
        match cached_version {
            None => return Err(SyncError::NotSetUp),
            Some(version) if version != current.key_version => {
                return Err(SyncError::KeyVersionBehind)
            }
            Some(_) => {}
        }

        let bundle = KeyBundle::generate(current.key_version + 1, hash_key);
        let sealed = bundle.seal(new_master).map_err(SyncError::Local)?;
        let keys = bundle.item_keys();
        let outbox_mark = self.db.with_conn(sync_state::outbox_last_id)?;

        self.client.start_rotation().await?;
        let committed = match self.upload_for_rotation(&keys).await {
            Ok(()) => self
                .client
                .commit_rotation(&sealed)
                .await
                .map_err(SyncError::from),
            Err(e) => Err(e),
        };
        match committed {
            Ok(key) => Ok(Rotation {
                sealed,
                key,
                outbox_mark,
            }),
            Err(e) => {
                let _ = self.client.abort_rotation().await;
                Err(e)
            }
        }
    }

    /// Records a committed rotation, once this device uses the new phrase. Changes queued
    /// before the rotation started were part of it.
    pub fn finish_rotation(
        &self,
        rotation: &Rotation,
        hash_key: &[u8; 32],
    ) -> Result<(), SyncError> {
        self.db.with_conn(|c| {
            sync_state::set_meta(c, META_KEY_BUNDLE, &rotation.sealed)?;
            sync_state::set_meta(c, META_KEY_VERSION, rotation.key.key_version)?;
            sync_state::set_meta(c, META_GENERATION_ID, rotation.key.generation_id.as_str())?;
            sync_state::set_meta(c, META_LAST_SEQ, rotation.key.latest_seq)?;
            sync_state::set_meta(c, META_HASH_KEY, hash_key.as_slice())?;
            sync_state::clear_sync_ids(c)?;
            sync_state::remove_outbox_through(c, rotation.outbox_mark)
        })?;
        Ok(())
    }

    /// Opens the account's key with a phrase entered on this device, after another device
    /// changed it. Nothing changes here until `adopt_phrase`.
    pub async fn open_with_phrase(&self, master: &MasterKey) -> Result<PhraseCheck, SyncError> {
        let server = self.client.get_key().await?;
        let sealed = server
            .bundle
            .ok_or_else(|| SyncError::Server("no sync key".into()))?;
        let bundle = KeyBundle::open(master, &sealed).map_err(|_| SyncError::WrongPhrase)?;
        Ok(PhraseCheck {
            sealed,
            key_version: server.key_version,
            hash_key: *bundle.hash_key(),
        })
    }

    /// Records the key opened with a new phrase, once this device uses that phrase. The next
    /// run finds the new generation id and does the restore steps.
    pub fn adopt_phrase(
        &self,
        check: &PhraseCheck,
        previous_hash_key: &[u8; 32],
    ) -> Result<(), SyncError> {
        self.db.with_conn(|c| {
            sync_state::set_meta(c, META_KEY_BUNDLE, &check.sealed)?;
            sync_state::set_meta(c, META_KEY_VERSION, check.key_version)?;
            sync_state::set_meta(c, META_HASH_KEY, check.hash_key.as_slice())?;
            sync_state::clear_sync_ids(c)?;
            if previous_hash_key != &check.hash_key {
                sync_state::rehash_text_entries(c, &check.hash_key)?;
            }
            Ok(())
        })?;
        Ok(())
    }

    pub async fn run(&self, master: &MasterKey, mode: SyncMode) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        let (keys, server) = self.ensure_key(master, &mut report).await?;

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
    async fn ensure_key(
        &self,
        master: &MasterKey,
        report: &mut SyncReport,
    ) -> Result<(ItemKeys, KeyState), SyncError> {
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
        let cached_version: Option<u32> = self
            .db
            .with_conn(|c| sync_state::meta(c, META_KEY_VERSION))?;
        let bundle = match KeyBundle::open(master, &sealed) {
            Ok(bundle) => bundle,
            Err(_) if cached_version.is_some_and(|v| server.key_version > v) => {
                return Err(SyncError::KeyVersionBehind)
            }
            Err(_) => return Err(SyncError::DifferentPhrase),
        };
        if bundle.version() != server.key_version {
            return Err(SyncError::KeyChanged);
        }

        let current_hash_key = match self.db.stored_hash_key()? {
            Some(stored) => stored,
            None => master.derive_hash_key(),
        };
        let hash_key_changed = bundle.hash_key() != &current_hash_key;
        self.db.with_conn(|c| {
            sync_state::set_meta(c, META_KEY_BUNDLE, &sealed)?;
            sync_state::set_meta(c, META_KEY_VERSION, server.key_version)?;
            sync_state::set_meta(c, META_HASH_KEY, bundle.hash_key().as_slice())?;
            // Item ids come from the key, so a new key version means new ids.
            if cached_version != Some(server.key_version) {
                sync_state::clear_sync_ids(c)?;
            }
            if hash_key_changed {
                sync_state::rehash_text_entries(c, bundle.hash_key())?;
            }
            Ok(())
        })?;
        if hash_key_changed {
            report.new_hash_key = Some(*bundle.hash_key());
        }
        Ok((bundle.item_keys(), server))
    }

    /// Uploads to the rotation's staging area: every synced entry, oldest first, then deletes
    /// from the last 30 days of content that isn't here any more.
    async fn upload_for_rotation(&self, keys: &ItemKeys) -> Result<(), SyncError> {
        let (entries, deletes) = self.db.with_conn(|c| {
            Ok((
                sync_state::syncable_entry_hashes(c)?,
                sync_state::recent_delete_hashes(c)?,
            ))
        })?;

        let mut batch: Vec<SyncItem> = Vec::new();
        let mut body_bytes = 0;
        let rows = entries
            .into_iter()
            .map(|hash| (hash, false))
            .chain(deletes.into_iter().map(|hash| (hash, true)));
        for (hash, is_delete) in rows {
            let item = if is_delete {
                item::encrypt_delete(keys, &hash)
            } else {
                match self
                    .db
                    .with_conn(|c| sync_state::entry_for_upload(c, &hash))?
                {
                    Some(entry) => item::encrypt_entry(keys, &payload(entry)),
                    None => continue,
                }
            }
            .map_err(SyncError::Local)?;
            if item.ciphertext.len() > MAX_ITEM_BYTES {
                continue;
            }

            let item_bytes = wire_bytes(&item);
            if !batch.is_empty() && body_bytes + item_bytes > MAX_BODY_BYTES {
                self.client.post_rotation_items(&batch).await?;
                batch.clear();
                body_bytes = 0;
            }
            body_bytes += item_bytes;
            batch.push(item);
        }
        if !batch.is_empty() {
            self.client.post_rotation_items(&batch).await?;
        }
        Ok(())
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
                        Some(entry) => item::encrypt_entry(keys, &payload(entry)),
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
                let item_bytes = wire_bytes(&item);
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

fn payload(entry: ClipboardEntry) -> EntryPayload {
    EntryPayload {
        content_type: entry.content_type,
        content: entry.content,
        content_hash: entry.content_hash,
        is_favorite: entry.is_favorite,
        last_used_at: entry.last_used_at,
    }
}

/// An item's size in the request body: base64 ciphertext plus room for its other fields.
fn wire_bytes(item: &SyncItem) -> usize {
    item.ciphertext.len().div_ceil(3) * 4 + 256
}
