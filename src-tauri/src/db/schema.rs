use rusqlite::Connection;

pub fn initialize(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
            id TEXT PRIMARY KEY,
            content BLOB NOT NULL,
            content_type TEXT NOT NULL DEFAULT 'text',
            content_hash BLOB NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            is_favorite INTEGER NOT NULL DEFAULT 0,
            is_sensitive INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            source_app TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_entries_created_at ON entries(created_at);
        CREATE INDEX IF NOT EXISTS idx_entries_content_hash ON entries(content_hash);
        CREATE INDEX IF NOT EXISTS idx_entries_is_favorite ON entries(is_favorite);
        CREATE INDEX IF NOT EXISTS idx_entries_content_type ON entries(content_type);
        CREATE INDEX IF NOT EXISTS idx_entries_is_sensitive ON entries(is_sensitive);",
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_without_error() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        initialize(&conn).unwrap();
    }
}
