pub mod entries;
pub mod schema;
pub mod settings;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub use entries::{ClipboardEntry, EntryType, NewEntry};

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

    pub fn insert_entry(&self, entry: &NewEntry) -> Result<String, rusqlite::Error> {
        let guard = self.conn()?;
        entries::insert_entry(guard.as_ref().unwrap(), entry)
    }

    pub fn get_entry(&self, id: &str) -> Result<Option<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_entry(guard.as_ref().unwrap(), id)
    }

    pub fn get_entries(
        &self,
        limit: usize,
        offset: usize,
        entry_type: Option<EntryType>,
        favorites_only: bool,
    ) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_entries(
            guard.as_ref().unwrap(),
            limit,
            offset,
            entry_type,
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
        let guard = self.conn()?;
        entries::toggle_favorite(guard.as_ref().unwrap(), id)
    }

    pub fn delete_entry(&self, id: &str) -> Result<(), rusqlite::Error> {
        let guard = self.conn()?;
        entries::delete_entry(guard.as_ref().unwrap(), id)
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
        let guard = self.conn()?;
        entries::touch_entry(guard.as_ref().unwrap(), id)
    }

    pub fn touch_entry_by_hash(&self, hash: &[u8]) -> Result<bool, rusqlite::Error> {
        let guard = self.conn()?;
        entries::touch_entry_by_hash(guard.as_ref().unwrap(), hash)
    }

    pub fn get_all_entries(&self) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let guard = self.conn()?;
        entries::get_all_entries(guard.as_ref().unwrap())
    }

    pub fn wipe_all(&self) -> Result<(), rusqlite::Error> {
        let guard = self.conn()?;
        guard
            .as_ref()
            .unwrap()
            .execute_batch("DELETE FROM entries;")?;
        Ok(())
    }

    /// Produce a consistent, encrypted snapshot of the live DB at `dst_path` using the SQLite
    /// online backup API. The destination is keyed with `key` (same SQLCipher key), so the
    /// resulting file is a self-contained encrypted `cmdv.db`. Returns the snapshot byte size.
    ///
    /// The backup API copies committed pages, sidestepping WAL/checkpoint concerns.
    pub fn backup_to_encrypted(&self, dst_path: &Path, key: &[u8; 32]) -> Result<u64, String> {
        use zeroize::Zeroize;

        // Overwrite any stale snapshot so backup starts from an empty destination.
        let _ = std::fs::remove_file(dst_path);

        let guard = self.conn().map_err(|e| e.to_string())?;
        let src = guard.as_ref().unwrap();

        let mut dst = Connection::open(dst_path).map_err(|e| e.to_string())?;
        let mut hex_key = hex::encode(key);
        let mut pragma = format!("PRAGMA key = \"x'{}'\";", hex_key);
        hex_key.zeroize();
        let key_result = dst.execute_batch(&pragma);
        pragma.zeroize();
        key_result.map_err(|e| format!("Failed to key snapshot: {}", e))?;

        {
            let backup =
                rusqlite::backup::Backup::new(src, &mut dst).map_err(|e| e.to_string())?;
            backup
                .run_to_completion(100, std::time::Duration::from_millis(0), None)
                .map_err(|e| format!("Backup failed: {}", e))?;
        }

        dst.close().map_err(|(_, e)| e.to_string())?;

        std::fs::metadata(dst_path)
            .map(|m| m.len())
            .map_err(|e| e.to_string())
    }
}
