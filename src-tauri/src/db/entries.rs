use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Text,
    Image,
}

impl EntryType {
    pub fn as_str(&self) -> &str {
        match self {
            EntryType::Text => "text",
            EntryType::Image => "image",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "image" => EntryType::Image,
            _ => EntryType::Text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: Vec<u8>,
    pub content_type: EntryType,
    pub content_hash: Vec<u8>,
    pub created_at: String,
    pub is_favorite: bool,
    pub is_sensitive: bool,
    pub size_bytes: i64,
    pub source_app: Option<String>,
}

pub struct NewEntry {
    pub content: Vec<u8>,
    pub content_type: EntryType,
    pub content_hash: Vec<u8>,
    pub size_bytes: i64,
    pub is_favorite: bool,
    pub is_sensitive: bool,
    pub source_app: Option<String>,
}

pub fn insert_entry(conn: &Connection, entry: &NewEntry) -> Result<String, rusqlite::Error> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO entries (id, content, content_type, content_hash, is_favorite, is_sensitive, size_bytes, source_app)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            entry.content,
            entry.content_type.as_str(),
            entry.content_hash,
            entry.is_favorite as i32,
            entry.is_sensitive as i32,
            entry.size_bytes,
            entry.source_app,
        ],
    )?;
    Ok(id)
}

pub fn get_entry(conn: &Connection, id: &str) -> Result<Option<ClipboardEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, content_hash, created_at, is_favorite, is_sensitive, size_bytes, source_app
         FROM entries WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], row_to_entry)?;
    Ok(rows.next().transpose()?)
}

pub fn get_entries(
    conn: &Connection,
    limit: usize,
    offset: usize,
    entry_type: Option<EntryType>,
    favorites_only: bool,
) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
    let mut sql = String::from(
        "SELECT id, content, content_type, content_hash, created_at, is_favorite, is_sensitive, size_bytes, source_app
         FROM entries WHERE 1=1",
    );

    if let Some(ref t) = entry_type {
        sql.push_str(&format!(" AND content_type = '{}'", t.as_str()));
    }
    if favorites_only {
        sql.push_str(" AND is_favorite = 1");
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ?1 OFFSET ?2");

    let mut stmt = conn.prepare(&sql)?;
    let entries = stmt
        .query_map(params![limit as i64, offset as i64], row_to_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

pub fn search_entries(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, content_hash, created_at, is_favorite, is_sensitive, size_bytes, source_app
         FROM entries
         WHERE content_type = 'text' AND CAST(content AS TEXT) LIKE ?1 ESCAPE '\\'
         ORDER BY created_at DESC
         LIMIT ?2",
    )?;
    let entries = stmt
        .query_map(params![pattern, limit as i64], row_to_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

pub fn toggle_favorite(conn: &Connection, id: &str) -> Result<bool, rusqlite::Error> {
    conn.execute(
        "UPDATE entries SET is_favorite = NOT is_favorite WHERE id = ?1",
        params![id],
    )?;
    let new_state: bool = conn.query_row(
        "SELECT is_favorite FROM entries WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(new_state)
}

pub fn delete_entry(conn: &Connection, id: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn entry_exists_by_hash(conn: &Connection, hash: &[u8]) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE content_hash = ?1",
        params![hash],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn get_total_size(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(SUM(size_bytes), 0) FROM entries",
        [],
        |row| row.get(0),
    )
}

pub fn get_entry_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
}

pub fn prune_oldest_non_favorites(
    conn: &Connection,
    bytes_to_free: i64,
) -> Result<usize, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, size_bytes FROM entries
         WHERE is_favorite = 0
         ORDER BY created_at ASC",
    )?;
    let candidates: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut freed: i64 = 0;
    let mut deleted = 0;

    for (id, size) in &candidates {
        if freed >= bytes_to_free {
            break;
        }
        delete_entry(conn, id)?;
        freed += size;
        deleted += 1;
    }

    Ok(deleted)
}

pub fn delete_expired_sensitive(
    conn: &Connection,
    max_age_secs: i64,
) -> Result<usize, rusqlite::Error> {
    let deleted = conn.execute(
        "DELETE FROM entries WHERE is_sensitive = 1 AND is_favorite = 0
         AND datetime(created_at, '+' || ?1 || ' seconds') < datetime('now')",
        params![max_age_secs],
    )?;
    Ok(deleted)
}

pub fn get_all_entries(conn: &Connection) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, content_hash, created_at, is_favorite, is_sensitive, size_bytes, source_app
         FROM entries ORDER BY created_at ASC",
    )?;
    let entries = stmt
        .query_map([], row_to_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

fn row_to_entry(row: &rusqlite::Row) -> Result<ClipboardEntry, rusqlite::Error> {
    let content_type_str: String = row.get(2)?;
    Ok(ClipboardEntry {
        id: row.get(0)?,
        content: row.get(1)?,
        content_type: EntryType::from_str(&content_type_str),
        content_hash: row.get(3)?,
        created_at: row.get(4)?,
        is_favorite: row.get::<_, i32>(5)? != 0,
        is_sensitive: row.get::<_, i32>(6)? != 0,
        size_bytes: row.get(7)?,
        source_app: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::initialize(&conn).unwrap();
        conn
    }

    fn sample_entry() -> NewEntry {
        NewEntry {
            content: b"hello world".to_vec(),
            content_type: EntryType::Text,
            content_hash: vec![7, 8, 9],
            size_bytes: 100,
            is_favorite: false,
            is_sensitive: false,
            source_app: None,
        }
    }

    #[test]
    fn insert_and_get_entry() {
        let conn = setup_db();
        let entry = sample_entry();
        let id = insert_entry(&conn, &entry).unwrap();
        let retrieved = get_entry(&conn, &id).unwrap().unwrap();
        assert_eq!(retrieved.content, entry.content);
        assert_eq!(retrieved.size_bytes, 100);
    }

    #[test]
    fn get_entries_pagination() {
        let conn = setup_db();
        for _ in 0..5 {
            insert_entry(&conn, &sample_entry()).unwrap();
        }
        let page1 = get_entries(&conn, 3, 0, None, false).unwrap();
        let page2 = get_entries(&conn, 3, 3, None, false).unwrap();
        assert_eq!(page1.len(), 3);
        assert_eq!(page2.len(), 2);
    }

    #[test]
    fn toggle_favorite_works() {
        let conn = setup_db();
        let id = insert_entry(&conn, &sample_entry()).unwrap();
        assert!(!get_entry(&conn, &id).unwrap().unwrap().is_favorite);
        toggle_favorite(&conn, &id).unwrap();
        assert!(get_entry(&conn, &id).unwrap().unwrap().is_favorite);
        toggle_favorite(&conn, &id).unwrap();
        assert!(!get_entry(&conn, &id).unwrap().unwrap().is_favorite);
    }

    #[test]
    fn delete_entry_works() {
        let conn = setup_db();
        let id = insert_entry(&conn, &sample_entry()).unwrap();
        assert!(get_entry(&conn, &id).unwrap().is_some());
        delete_entry(&conn, &id).unwrap();
        assert!(get_entry(&conn, &id).unwrap().is_none());
    }

    #[test]
    fn hash_deduplication() {
        let conn = setup_db();
        let entry = sample_entry();
        insert_entry(&conn, &entry).unwrap();
        assert!(entry_exists_by_hash(&conn, &entry.content_hash).unwrap());
        assert!(!entry_exists_by_hash(&conn, &[99, 99, 99]).unwrap());
    }

    #[test]
    fn total_size_tracking() {
        let conn = setup_db();
        assert_eq!(get_total_size(&conn).unwrap(), 0);
        insert_entry(&conn, &sample_entry()).unwrap();
        assert_eq!(get_total_size(&conn).unwrap(), 100);
    }

    #[test]
    fn prune_respects_favorites() {
        let conn = setup_db();
        let mut fav = sample_entry();
        fav.is_favorite = true;
        fav.size_bytes = 500;
        insert_entry(&conn, &fav).unwrap();

        insert_entry(&conn, &NewEntry {
            content_hash: vec![10, 11, 12],
            ..sample_entry()
        }).unwrap();

        let pruned = prune_oldest_non_favorites(&conn, 200).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(get_entry_count(&conn).unwrap(), 1);
    }

    #[test]
    fn search_finds_matching_text() {
        let conn = setup_db();
        insert_entry(&conn, &NewEntry {
            content: b"the quick brown fox".to_vec(),
            content_hash: vec![1, 2, 3],
            ..sample_entry()
        }).unwrap();
        insert_entry(&conn, &NewEntry {
            content: b"lazy dog".to_vec(),
            content_hash: vec![4, 5, 6],
            ..sample_entry()
        }).unwrap();

        let results = search_entries(&conn, "quick", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, b"the quick brown fox");
    }

    #[test]
    fn search_escapes_like_wildcards() {
        let conn = setup_db();
        insert_entry(&conn, &NewEntry {
            content: b"100% done".to_vec(),
            content_hash: vec![20, 21, 22],
            ..sample_entry()
        }).unwrap();
        insert_entry(&conn, &NewEntry {
            content: b"nothing here".to_vec(),
            content_hash: vec![30, 31, 32],
            ..sample_entry()
        }).unwrap();

        let results = search_entries(&conn, "%", 10).unwrap();
        assert_eq!(results.len(), 1, "literal % should not match all rows");

        let results = search_entries(&conn, "_", 10).unwrap();
        assert_eq!(results.len(), 0, "literal _ should not act as wildcard");
    }
}
