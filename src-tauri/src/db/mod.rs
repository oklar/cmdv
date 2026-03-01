pub mod entries;
pub mod schema;
pub mod settings;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub use entries::{ClipboardEntry, EntryType, NewEntry};

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open the database file without applying an encryption key or initializing the schema.
    /// Call `set_encryption_key` after vault unlock to make the DB usable.
    pub fn open_encrypted(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Apply the SQLCipher encryption key and initialize the schema.
    /// Must be called exactly once after vault unlock.
    pub fn set_encryption_key(&self, key: &[u8; 32]) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let hex_key = hex::encode(key);
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key))
            .map_err(|e| format!("Failed to set DB encryption key: {}", e))?;
        schema::initialize(&conn)
            .map_err(|e| format!("Failed to initialize schema: {}", e))?;
        Ok(())
    }

    /// Open an unencrypted in-memory database with immediate schema init (for tests).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        schema::initialize(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_entry(&self, entry: &NewEntry) -> Result<String, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::insert_entry(&conn, entry)
    }

    pub fn get_entry(&self, id: &str) -> Result<Option<ClipboardEntry>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::get_entry(&conn, id)
    }

    pub fn get_entries(
        &self,
        limit: usize,
        offset: usize,
        entry_type: Option<EntryType>,
        favorites_only: bool,
    ) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::get_entries(&conn, limit, offset, entry_type, favorites_only)
    }

    pub fn search_entries(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::search_entries(&conn, query, limit)
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::toggle_favorite(&conn, id)
    }

    pub fn delete_entry(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::delete_entry(&conn, id)
    }

    pub fn entry_exists_by_hash(&self, hash: &[u8]) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::entry_exists_by_hash(&conn, hash)
    }

    pub fn get_total_size(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::get_total_size(&conn)
    }

    pub fn get_entry_count(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::get_entry_count(&conn)
    }

    pub fn prune_oldest_non_favorites(
        &self,
        bytes_to_free: i64,
    ) -> Result<usize, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::prune_oldest_non_favorites(&conn, bytes_to_free)
    }

    pub fn delete_expired_sensitive(&self, max_age_secs: i64) -> Result<usize, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::delete_expired_sensitive(&conn, max_age_secs)
    }

    pub fn get_all_entries(&self) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        entries::get_all_entries(&conn)
    }
}
