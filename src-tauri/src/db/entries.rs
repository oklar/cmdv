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

/// What the entry list is narrowed to. `Link` is not a stored type: links are
/// text entries whose whole content is a single http(s) URL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntryFilter {
    Text,
    Image,
    Link,
}

impl EntryFilter {
    pub fn from_str(s: &str) -> Self {
        match s {
            "image" => EntryFilter::Image,
            "link" => EntryFilter::Link,
            _ => EntryFilter::Text,
        }
    }

    fn sql(&self) -> &'static str {
        match self {
            EntryFilter::Text => " AND content_type = 'text'",
            EntryFilter::Image => " AND content_type = 'image'",
            EntryFilter::Link => LINK_SQL,
        }
    }
}

/// A text entry that, once surrounding whitespace (space, tab, LF, CR) is
/// trimmed, starts with http:// or https:// and has no whitespace inside it.
/// LIKE ignores ASCII case, so HTTPS:// matches too.
const LINK_SQL: &str = " AND content_type = 'text'
     AND (TRIM(CAST(content AS TEXT), char(32, 9, 10, 13)) LIKE 'http://_%'
       OR TRIM(CAST(content AS TEXT), char(32, 9, 10, 13)) LIKE 'https://_%')
     AND TRIM(CAST(content AS TEXT), char(32, 9, 10, 13))
         NOT GLOB '*[' || char(32, 9, 10, 13) || ']*'";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: Vec<u8>,
    pub content_type: EntryType,
    pub content_hash: Vec<u8>,
    pub last_used_at: String,
    pub is_favorite: bool,
    pub size_bytes: i64,
    /// The copying app asked that this content never leave the device, so it is never synced.
    pub local_only: bool,
}

pub struct NewEntry {
    pub content: Vec<u8>,
    pub content_type: EntryType,
    pub content_hash: Vec<u8>,
    pub size_bytes: i64,
    pub is_favorite: bool,
    pub local_only: bool,
}

pub fn insert_entry(conn: &Connection, entry: &NewEntry) -> Result<String, rusqlite::Error> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO entries (id, content, content_type, content_hash, is_favorite, size_bytes, local_only)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            entry.content,
            entry.content_type.as_str(),
            entry.content_hash,
            entry.is_favorite as i32,
            entry.size_bytes,
            entry.local_only,
        ],
    )?;
    Ok(id)
}

pub fn get_entry(conn: &Connection, id: &str) -> Result<Option<ClipboardEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, content_hash, last_used_at, is_favorite, size_bytes, local_only
         FROM entries WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], row_to_entry)?;
    Ok(rows.next().transpose()?)
}

pub fn get_entries(
    conn: &Connection,
    limit: usize,
    offset: usize,
    filter: Option<EntryFilter>,
    favorites_only: bool,
) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
    let mut sql = String::from(
        "SELECT id, content, content_type, content_hash, last_used_at, is_favorite, size_bytes, local_only
         FROM entries WHERE 1=1",
    );

    if let Some(filter) = filter {
        sql.push_str(filter.sql());
    }
    if favorites_only {
        sql.push_str(" AND is_favorite = 1");
    }
    sql.push_str(" ORDER BY last_used_at DESC LIMIT ?1 OFFSET ?2");

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
        "SELECT id, content, content_type, content_hash, last_used_at, is_favorite, size_bytes, local_only
         FROM entries
         WHERE content_type = 'text' AND CAST(content AS TEXT) LIKE ?1 ESCAPE '\\'
         ORDER BY last_used_at DESC
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
         ORDER BY last_used_at ASC",
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

pub fn touch_entry(conn: &Connection, id: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE entries SET last_used_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn touch_entry_by_hash(conn: &Connection, hash: &[u8]) -> Result<bool, rusqlite::Error> {
    let updated = conn.execute(
        "UPDATE entries SET last_used_at = datetime('now') WHERE content_hash = ?1",
        params![hash],
    )?;
    Ok(updated > 0)
}

pub fn get_all_entries(conn: &Connection) -> Result<Vec<ClipboardEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, content_hash, last_used_at, is_favorite, size_bytes, local_only
         FROM entries ORDER BY last_used_at ASC",
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
        last_used_at: row.get(4)?,
        is_favorite: row.get::<_, i32>(5)? != 0,
        size_bytes: row.get(6)?,
        local_only: row.get(7)?,
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
            local_only: false,
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
        assert!(!retrieved.local_only);
    }

    #[test]
    fn local_only_is_stored() {
        let conn = setup_db();
        let id = insert_entry(
            &conn,
            &NewEntry {
                local_only: true,
                ..sample_entry()
            },
        )
        .unwrap();
        assert!(get_entry(&conn, &id).unwrap().unwrap().local_only);
        assert!(get_all_entries(&conn).unwrap()[0].local_only);
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

    fn insert_text(conn: &Connection, content: &str, hash: u8) -> String {
        insert_entry(
            conn,
            &NewEntry {
                content: content.as_bytes().to_vec(),
                content_hash: vec![hash],
                ..sample_entry()
            },
        )
        .unwrap()
    }

    fn contents(entries: Vec<ClipboardEntry>) -> Vec<String> {
        let mut out: Vec<String> = entries
            .into_iter()
            .map(|e| String::from_utf8(e.content).unwrap())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn type_filters_match_stored_type() {
        let conn = setup_db();
        insert_text(&conn, "plain text", 1);
        insert_entry(
            &conn,
            &NewEntry {
                content: vec![0xff, 0xd8],
                content_type: EntryType::Image,
                content_hash: vec![2],
                ..sample_entry()
            },
        )
        .unwrap();

        let text = get_entries(&conn, 50, 0, Some(EntryFilter::Text), false).unwrap();
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].content_type, EntryType::Text);

        let images = get_entries(&conn, 50, 0, Some(EntryFilter::Image), false).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].content_type, EntryType::Image);
    }

    #[test]
    fn link_filter_matches_only_whole_url_entries() {
        let conn = setup_db();
        // Links.
        insert_text(&conn, "https://github.com/oklar/cmdv", 1);
        insert_text(&conn, "http://localhost:5173/?q=a%20b#top", 2);
        insert_text(&conn, "  https://example.com/path\r\n", 3);
        insert_text(&conn, "HTTPS://EXAMPLE.COM", 4);
        // Not links.
        insert_text(&conn, "see https://example.com for details", 5);
        insert_text(&conn, "https://example.com and more", 6);
        insert_text(&conn, "https://a.example\nhttps://b.example", 7);
        insert_text(&conn, "https://", 8);
        insert_text(&conn, "ftp://example.com/file", 9);
        insert_text(&conn, "httpsfoo", 10);
        insert_text(&conn, "plain text", 11);
        insert_entry(
            &conn,
            &NewEntry {
                content: b"https://example.com/image-bytes".to_vec(),
                content_type: EntryType::Image,
                content_hash: vec![12],
                ..sample_entry()
            },
        )
        .unwrap();

        let links = get_entries(&conn, 50, 0, Some(EntryFilter::Link), false).unwrap();
        assert_eq!(
            contents(links),
            vec![
                "  https://example.com/path\r\n",
                "HTTPS://EXAMPLE.COM",
                "http://localhost:5173/?q=a%20b#top",
                "https://github.com/oklar/cmdv",
            ]
        );
    }

    #[test]
    fn link_filter_combines_with_favorites() {
        let conn = setup_db();
        let fav = insert_text(&conn, "https://example.com/fav", 1);
        insert_text(&conn, "https://example.com/other", 2);
        let fav_text = insert_text(&conn, "favorite note", 3);
        toggle_favorite(&conn, &fav).unwrap();
        toggle_favorite(&conn, &fav_text).unwrap();

        let links = get_entries(&conn, 50, 0, Some(EntryFilter::Link), true).unwrap();
        assert_eq!(contents(links), vec!["https://example.com/fav"]);
    }

    #[test]
    fn unknown_filter_name_falls_back_to_text() {
        assert_eq!(EntryFilter::from_str("image"), EntryFilter::Image);
        assert_eq!(EntryFilter::from_str("link"), EntryFilter::Link);
        assert_eq!(EntryFilter::from_str("text"), EntryFilter::Text);
        assert_eq!(EntryFilter::from_str("whatever"), EntryFilter::Text);
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

        insert_entry(
            &conn,
            &NewEntry {
                content_hash: vec![10, 11, 12],
                ..sample_entry()
            },
        )
        .unwrap();

        let pruned = prune_oldest_non_favorites(&conn, 200).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(get_entry_count(&conn).unwrap(), 1);
    }

    #[test]
    fn search_finds_matching_text() {
        let conn = setup_db();
        insert_entry(
            &conn,
            &NewEntry {
                content: b"the quick brown fox".to_vec(),
                content_hash: vec![1, 2, 3],
                ..sample_entry()
            },
        )
        .unwrap();
        insert_entry(
            &conn,
            &NewEntry {
                content: b"lazy dog".to_vec(),
                content_hash: vec![4, 5, 6],
                ..sample_entry()
            },
        )
        .unwrap();

        let results = search_entries(&conn, "quick", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, b"the quick brown fox");
    }

    #[test]
    fn search_escapes_like_wildcards() {
        let conn = setup_db();
        insert_entry(
            &conn,
            &NewEntry {
                content: b"100% done".to_vec(),
                content_hash: vec![20, 21, 22],
                ..sample_entry()
            },
        )
        .unwrap();
        insert_entry(
            &conn,
            &NewEntry {
                content: b"nothing here".to_vec(),
                content_hash: vec![30, 31, 32],
                ..sample_entry()
            },
        )
        .unwrap();

        let results = search_entries(&conn, "%", 10).unwrap();
        assert_eq!(results.len(), 1, "literal % should not match all rows");

        let results = search_entries(&conn, "_", 10).unwrap();
        assert_eq!(results.len(), 0, "literal _ should not act as wildcard");
    }
}
