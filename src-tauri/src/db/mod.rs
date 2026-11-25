pub mod entries;
pub mod schema;
pub mod settings;
pub mod sync_state;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub use entries::{ClipboardEntry, EntryFilter, EntryType, NewEntry};

pub struct Database {
    conn: Mutex<Option<Connection>>,
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
        })
    }

    /// Apply the SQLCipher encryption key and initialize the schema.
    /// Must be called exactly once after vault unlock.
    pub fn set_encryption_key(&self, key: &[u8; 32]) -> Result<(), String> {
        use zeroize::Zeroize;
        let guard = self.conn().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().unwrap();
        let mut hex_key = hex::encode(key);
        let mut pragma = format!("PRAGMA key = \"x'{}'\";", hex_key);
        hex_key.zeroize();
        conn.execute_batch(&pragma)
            .map_err(|e| format!("Failed to set DB encryption key: {}", e))?;
        pragma.zeroize();
        schema::initialize(conn).map_err(|e| format!("Failed to initialize schema: {}", e))?;
        Ok(())
    }

    /// Open an unencrypted in-memory database with immediate schema init (for tests).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        schema::initialize(&conn)?;
        Ok(Self {
            conn: Mutex::new(Some(conn)),
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
