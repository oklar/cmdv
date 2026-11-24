use rusqlite::Connection;

/// Applied in order; `PRAGMA user_version` counts the ones a database has. Version 1 is the
/// schema from before versioning, written with IF NOT EXISTS so an existing version-0 database
/// takes it as a no-op.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS entries (
        id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        content_type TEXT NOT NULL DEFAULT 'text',
        content_hash BLOB NOT NULL,
        last_used_at TEXT NOT NULL DEFAULT (datetime('now')),
        is_favorite INTEGER NOT NULL DEFAULT 0,
        size_bytes INTEGER NOT NULL DEFAULT 0
    );

    CREATE INDEX IF NOT EXISTS idx_entries_last_used_at ON entries(last_used_at);
    CREATE INDEX IF NOT EXISTS idx_entries_content_hash ON entries(content_hash);
    CREATE INDEX IF NOT EXISTS idx_entries_is_favorite ON entries(is_favorite);
    CREATE INDEX IF NOT EXISTS idx_entries_content_type ON entries(content_type);",
    // Cloud sync. Outbox rows and recent deletes keep the content hash rather than the sync id,
    // because a phrase rotation changes every id.
    "ALTER TABLE entries ADD COLUMN local_only INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE entries ADD COLUMN sync_id TEXT;
    CREATE INDEX idx_entries_sync_id ON entries(sync_id);

    CREATE TABLE sync_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        content_hash BLOB NOT NULL,
        kind TEXT NOT NULL,
        queued_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE sync_recent_deletes (
        content_hash BLOB PRIMARY KEY,
        deleted_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE sync_meta (
        key TEXT PRIMARY KEY,
        value BLOB
    ) WITHOUT ROWID;",
];

pub fn initialize(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    migrate(conn)
}

fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version as usize > MIGRATIONS.len() {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some("This vault was opened by a newer version of cmdv. Update cmdv to open it.".into()),
        ));
    }

    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as u32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_version(conn: &Connection) -> usize {
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap() as usize
    }

    #[test]
    fn schema_creates_without_error() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        assert_eq!(user_version(&conn), MIGRATIONS.len());
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        initialize(&conn).unwrap();
        assert_eq!(user_version(&conn), MIGRATIONS.len());
    }

    #[test]
    fn version_zero_database_keeps_its_entries() {
        let conn = Connection::open_in_memory().unwrap();
        // A database from before versioning: today's tables, user_version still 0.
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute(
            "INSERT INTO entries (id, content, content_type, content_hash, is_favorite, size_bytes)
             VALUES ('a', x'68656c6c6f', 'text', x'0102', 1, 5)",
            [],
        )
        .unwrap();
        assert_eq!(user_version(&conn), 0);

        initialize(&conn).unwrap();

        assert_eq!(user_version(&conn), 2);
        let (content, favorite, local_only, sync_id): (Vec<u8>, bool, bool, Option<String>) = conn
            .query_row(
                "SELECT content, is_favorite, local_only, sync_id FROM entries WHERE id = 'a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(content, b"hello");
        assert!(favorite);
        assert!(!local_only);
        assert_eq!(sync_id, None);
        for table in ["sync_outbox", "sync_recent_deletes", "sync_meta"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn newer_database_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", MIGRATIONS.len() as u32 + 1).unwrap();
        assert!(initialize(&conn).is_err());
    }
}
