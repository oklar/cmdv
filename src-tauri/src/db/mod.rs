pub mod entries;
pub mod schema;
pub mod settings;
pub mod sync_state;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zeroize::Zeroize;

pub use entries::{ClipboardEntry, EntryFilter, EntryType, NewEntry};

pub struct Database {
    conn: Mutex<Option<Connection>>,
    /// The file behind `conn`, so a failed key attempt can start over on a fresh connection.
    path: Option<PathBuf>,
}

impl Database {
    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Option<Connection>>, rusqlite::Error> {
        let guard = self.conn.lock().unwrap();
        if guard.is_none() {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
                Some("Database closed".into()),
            ));
        }
        Ok(guard)
    }

    /// Open the database file without applying an encryption key or initializing the schema.
    /// Call `set_encryption_key` after vault unlock to make the DB usable.
    pub fn open_encrypted(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Mutex::new(Some(conn)),
            path: Some(path.to_path_buf()),
        })
    }

    /// Apply the SQLCipher encryption key and initialize the schema.
    /// Call once after vault unlock. If the key doesn't open the file, the connection is
    /// replaced, so another key can be tried.
    pub fn set_encryption_key(&self, key: &[u8; 32]) -> Result<(), String> {
        let mut guard = self.conn().map_err(|e| e.to_string())?;
        let result = Self::apply_key(guard.as_ref().unwrap(), key);
        if result.is_err() {
            if let Some(path) = &self.path {
                *guard = Some(Connection::open(path).map_err(|e| e.to_string())?);
            }
        }
        result
    }

    fn apply_key(conn: &Connection, key: &[u8; 32]) -> Result<(), String> {
        let mut hex_key = hex::encode(key);
        let mut pragma = format!("PRAGMA key = \"x'{}'\";", hex_key);
        hex_key.zeroize();
        let keyed = conn.execute_batch(&pragma);
        pragma.zeroize();
        keyed.map_err(|e| format!("Failed to set DB encryption key: {}", e))?;
        schema::initialize(conn).map_err(|e| format!("Failed to initialize schema: {}", e))
    }

    /// Re-encrypts the open database under `key`, for a new recovery phrase.
    pub fn rekey(&self, key: &[u8; 32]) -> Result<(), String> {
        let guard = self.conn().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().unwrap();
        // SQLCipher can't rekey a database in WAL mode.
        conn.execute_batch("PRAGMA journal_mode=DELETE;")
            .map_err(|e| e.to_string())?;
        let mut hex_key = hex::encode(key);
        let mut pragma = format!("PRAGMA rekey = \"x'{}'\";", hex_key);
        hex_key.zeroize();
        let rekeyed = conn.execute_batch(&pragma);
        pragma.zeroize();
        rekeyed.map_err(|e| format!("Failed to re-encrypt the database: {}", e))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| e.to_string())
    }

    pub fn store_hash_key(&self, hash_key: &[u8; 32]) -> Result<(), rusqlite::Error> {
        self.with_conn(|c| sync_state::set_meta(c, sync_state::META_HASH_KEY, hash_key.as_slice()))
    }

    /// The hash key kept after a phrase rotation. Without one, the phrase's own is used.
    pub fn stored_hash_key(&self) -> Result<Option<[u8; 32]>, rusqlite::Error> {
        let stored: Option<Vec<u8>> =
            self.with_conn(|c| sync_state::meta(c, sync_state::META_HASH_KEY))?;
        Ok(stored.and_then(|mut bytes| {
            let key = <[u8; 32]>::try_from(bytes.as_slice()).ok();
            bytes.zeroize();
            key
        }))
    }

    /// Open an unencrypted in-memory database with immediate schema init (for tests).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        schema::initialize(&conn)?;
        Ok(Self {
            conn: Mutex::new(Some(conn)),
            path: None,
        })
    }

    /// Drop the underlying connection, releasing the file lock.
    pub fn close(&self) {
        let mut guard = self.conn.lock().unwrap();
        guard.take();
    }

    pub(crate) fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, rusqlite::Error> {
        let guard = self.conn()?;
        f(guard.as_ref().unwrap())
    }

    /// Runs `f` in one transaction. The user-facing changes below use it to change an entry and
    /// queue that change for sync together.
    fn in_transaction<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, rusqlite::Error> {
        let guard = self.conn()?;
        let tx = guard.as_ref().unwrap().unchecked_transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn insert_entry(&self, entry: &NewEntry) -> Result<String, rusqlite::Error> {
        self.in_transaction(|conn| {
            let id = entries::insert_entry(conn, entry)?;
            sync_state::queue_upsert(conn, &entry.content_hash)?;
            Ok(id)
        })
    }

    pub fn get_entry(&self, id: &str) -> Result<Option<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_entry(guard.as_ref().unwrap(), id)
    }

    pub fn get_entries(
        &self,
        limit: usize,
        offset: usize,
        filter: Option<EntryFilter>,
        favorites_only: bool,
    ) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_entries(
            guard.as_ref().unwrap(),
            limit,
            offset,
            filter,
            favorites_only,
        )
    }

    pub fn search_entries(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::search_entries(guard.as_ref().unwrap(), query, limit)
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, rusqlite::Error> {
        self.in_transaction(|conn| {
            let favorite = entries::toggle_favorite(conn, id)?;
            sync_state::queue_upsert_by_id(conn, id)?;
            Ok(favorite)
        })
    }

    pub fn delete_entry(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.in_transaction(|conn| {
            if let Some(entry) = entries::get_entry(conn, id)? {
                entries::delete_entry(conn, id)?;
                sync_state::queue_delete(conn, &entry.content_hash, entry.local_only)?;
            }
            Ok(())
        })
    }

    pub fn entry_exists_by_hash(&self, hash: &[u8]) -> Result<bool, rusqlite::Error> {
        let guard = self.conn()?;
        entries::entry_exists_by_hash(guard.as_ref().unwrap(), hash)
    }

    pub fn get_total_size(&self) -> Result<i64, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_total_size(guard.as_ref().unwrap())
    }

    pub fn get_entry_count(&self) -> Result<i64, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_entry_count(guard.as_ref().unwrap())
    }

    pub fn prune_oldest_non_favorites(&self, bytes_to_free: i64) -> Result<usize, rusqlite::Error> {
        let guard = self.conn()?;
        entries::prune_oldest_non_favorites(guard.as_ref().unwrap(), bytes_to_free)
    }

    pub fn touch_entry(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.in_transaction(|conn| {
            entries::touch_entry(conn, id)?;
            sync_state::queue_upsert_by_id(conn, id)
        })
    }

    pub fn touch_entry_by_hash(&self, hash: &[u8]) -> Result<bool, rusqlite::Error> {
        self.in_transaction(|conn| {
            let touched = entries::touch_entry_by_hash(conn, hash)?;
            if touched {
                sync_state::queue_upsert(conn, hash)?;
            }
            Ok(touched)
        })
    }

    pub fn get_all_entries(&self) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_all_entries(guard.as_ref().unwrap())
    }

    /// Deletes every entry and queues a delete for each synced one.
    pub fn wipe_all(&self) -> Result<(), rusqlite::Error> {
        self.in_transaction(|conn| {
            for entry in entries::get_all_entries(conn)? {
                sync_state::queue_delete(conn, &entry.content_hash, entry.local_only)?;
            }
            conn.execute_batch("DELETE FROM entries;")
        })
    }

    /// Forgets the synced account: its key, position, outbox and recent deletes.
    pub fn clear_sync_state(&self) -> Result<(), rusqlite::Error> {
        self.with_conn(sync_state::clear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!("cmdv-rekey-test-{}.db", uuid::Uuid::new_v4()))
    }

    fn remove(path: &Path) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn rekeyed_file_opens_with_the_new_key_only() {
        let path = temp_db_path();
        let (old_key, new_key) = ([1u8; 32], [2u8; 32]);

        let db = Database::open_encrypted(&path).unwrap();
        db.set_encryption_key(&old_key).unwrap();
        db.insert_entry(&NewEntry {
            content: b"kept".to_vec(),
            content_type: EntryType::Text,
            content_hash: vec![1],
            size_bytes: 4,
            is_favorite: false,
            local_only: false,
        })
        .unwrap();
        db.rekey(&new_key).unwrap();
        db.close();

        let with_old = Database::open_encrypted(&path).unwrap();
        assert!(with_old.set_encryption_key(&old_key).is_err());
        // The failed attempt left a fresh connection, so the right key still works on it.
        with_old.set_encryption_key(&new_key).unwrap();
        assert_eq!(with_old.get_all_entries().unwrap()[0].content, b"kept");
        with_old.close();

        remove(&path);
    }
}
