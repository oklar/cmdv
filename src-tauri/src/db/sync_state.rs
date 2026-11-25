//! Local sync state: the outbox of changes waiting for upload, deletes from the last 30 days,
//! and sync metadata. Nothing is queued until this device has set up sync (it holds the
//! account's key bundle), so the outbox stays empty for devices that don't sync.

use rusqlite::types::{FromSql, ToSql};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::entries::ClipboardEntry;
use crate::sync::item::EntryPayload;

pub const META_KEY_BUNDLE: &str = "key_bundle";
pub const META_KEY_VERSION: &str = "key_version";
pub const META_GENERATION_ID: &str = "generation_id";
pub const META_LAST_SEQ: &str = "last_seq";
/// The hash key from the account's bundle, once it differs from the phrase's own (after a
/// rotation). Content hashes depend on it, so it outlives a logout.
pub const META_HASH_KEY: &str = "hash_key";

const SYNC_SET_UP: &str = "EXISTS (SELECT 1 FROM sync_meta WHERE key = 'key_bundle')";

pub struct OutboxRow {
    pub id: i64,
    pub content_hash: Vec<u8>,
    pub is_delete: bool,
}

pub fn meta<T: FromSql>(conn: &Connection, key: &str) -> rusqlite::Result<Option<T>> {
    conn.query_row(
        "SELECT value FROM sync_meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
}

pub fn set_meta<T: ToSql>(conn: &Connection, key: &str, value: T) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Queues the entry with this hash for upload, unless it is local-only or sync isn't set up.
/// A newer change to an entry replaces any older one still waiting.
pub fn queue_upsert(conn: &Connection, content_hash: &[u8]) -> rusqlite::Result<()> {
    let queued = conn.execute(
        &format!(
            "INSERT INTO sync_outbox (content_hash, kind)
             SELECT content_hash, 'entry' FROM entries
             WHERE content_hash = ?1 AND local_only = 0 AND {SYNC_SET_UP} LIMIT 1"
        ),
        params![content_hash],
    )?;
    if queued > 0 {
        drop_older_rows(conn, content_hash)?;
    }
    Ok(())
}

pub fn queue_upsert_by_id(conn: &Connection, entry_id: &str) -> rusqlite::Result<()> {
    let hash: Option<Vec<u8>> = conn
        .query_row(
            "SELECT content_hash FROM entries WHERE id = ?1",
            params![entry_id],
            |row| row.get(0),
        )
        .optional()?;
    match hash {
        Some(hash) => queue_upsert(conn, &hash),
        None => Ok(()),
    }
}

/// Queues a delete for an entry that was just removed, and remembers it for 30 days so it can
/// be sent again if the server is restored from a backup.
pub fn queue_delete(
    conn: &Connection,
    content_hash: &[u8],
    local_only: bool,
) -> rusqlite::Result<()> {
    if local_only || !sync_set_up(conn)? {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO sync_outbox (content_hash, kind) VALUES (?1, 'delete')",
        params![content_hash],
    )?;
    drop_older_rows(conn, content_hash)?;
    remember_delete(conn, content_hash)
}

/// Queues every syncable entry, for this device's first sync.
pub fn queue_all_entries(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_outbox (content_hash, kind)
         SELECT content_hash, 'entry' FROM entries WHERE local_only = 0 ORDER BY last_used_at",
        [],
    )?;
    keep_newest_row_per_entry(conn)
}

/// Sends the deletes from the last 30 days again, after the server was restored. Content that
/// was copied again since is left alone.
pub fn queue_recent_deletes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_outbox (content_hash, kind)
         SELECT content_hash, 'delete' FROM sync_recent_deletes
         WHERE content_hash NOT IN (SELECT content_hash FROM entries)",
        [],
    )?;
    keep_newest_row_per_entry(conn)
}

pub fn outbox_rows(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<OutboxRow>> {
    let mut stmt =
        conn.prepare("SELECT id, content_hash, kind FROM sync_outbox ORDER BY id LIMIT ?1")?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(OutboxRow {
                id: row.get(0)?,
                content_hash: row.get(1)?,
                is_delete: row.get::<_, String>(2)? == "delete",
            })
        })?
        .collect();
    rows
}

pub fn outbox_hashes(conn: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut stmt = conn.prepare("SELECT DISTINCT content_hash FROM sync_outbox")?;
    let hashes = stmt.query_map([], |row| row.get(0))?.collect();
    hashes
}

pub fn remove_outbox_rows(conn: &Connection, ids: &[i64]) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("DELETE FROM sync_outbox WHERE id = ?1")?;
    for id in ids {
        stmt.execute(params![id])?;
    }
    Ok(())
}

/// The entry to upload for this hash, or None if it is gone or local-only.
pub fn entry_for_upload(
    conn: &Connection,
    content_hash: &[u8],
) -> rusqlite::Result<Option<ClipboardEntry>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM entries WHERE content_hash = ?1 AND local_only = 0 LIMIT 1",
            params![content_hash],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => super::entries::get_entry(conn, &id),
        None => Ok(None),
    }
}

/// The server refused a pin. Un-pins the entry here and queues it again.
pub fn unpin_and_requeue(conn: &Connection, content_hash: &[u8]) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE entries SET is_favorite = 0 WHERE content_hash = ?1",
        params![content_hash],
    )?;
    queue_upsert(conn, content_hash)
}

pub fn entries_without_sync_id(conn: &Connection) -> rusqlite::Result<Vec<(String, Vec<u8>)>> {
    let mut stmt = conn
        .prepare("SELECT id, content_hash FROM entries WHERE sync_id IS NULL AND local_only = 0")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect();
    rows
}

pub fn set_sync_id(conn: &Connection, entry_id: &str, sync_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE entries SET sync_id = ?2 WHERE id = ?1",
        params![entry_id, sync_id],
    )?;
    Ok(())
}

/// Stores an entry from another device. Returns whether anything changed. Local-only entries
/// are never touched; with `insert_only`, existing entries aren't either.
pub fn apply_entry(
    conn: &Connection,
    sync_id: &str,
    entry: &EntryPayload,
    insert_only: bool,
) -> rusqlite::Result<bool> {
    let existing: Option<(String, bool)> = conn
        .query_row(
            "SELECT id, local_only FROM entries WHERE content_hash = ?1 LIMIT 1",
            params![entry.content_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    match existing {
        Some((_, true)) => Ok(false),
        Some((id, false)) if insert_only => {
            set_sync_id(conn, &id, sync_id)?;
            Ok(false)
        }
        Some((id, false)) => {
            conn.execute(
                "UPDATE entries SET is_favorite = ?2, last_used_at = ?3, sync_id = ?4 WHERE id = ?1",
                params![id, entry.is_favorite, entry.last_used_at, sync_id],
            )?;
            Ok(true)
        }
        None => {
            conn.execute(
                "INSERT INTO entries (id, content, content_type, content_hash, last_used_at,
                    is_favorite, size_bytes, local_only, sync_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
                params![
                    Uuid::new_v4().to_string(),
                    entry.content,
                    entry.content_type.as_str(),
                    entry.content_hash,
                    entry.last_used_at,
                    entry.is_favorite,
                    entry.content.len() as i64,
                    sync_id,
                ],
            )?;
            Ok(true)
        }
    }
}

/// Deletes the entry another device deleted. Returns whether there was one.
pub fn apply_delete(conn: &Connection, sync_id: &str) -> rusqlite::Result<bool> {
    let hashes: Vec<Vec<u8>> = {
        let mut stmt =
            conn.prepare("SELECT content_hash FROM entries WHERE sync_id = ?1 AND local_only = 0")?;
        let hashes = stmt
            .query_map(params![sync_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        hashes
    };
    for hash in &hashes {
        conn.execute(
            "DELETE FROM entries WHERE content_hash = ?1 AND local_only = 0",
            params![hash],
        )?;
        remember_delete(conn, hash)?;
    }
    Ok(!hashes.is_empty())
}

pub fn outbox_last_id(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COALESCE(MAX(id), 0) FROM sync_outbox", [], |row| {
        row.get(0)
    })
}

pub fn remove_outbox_through(conn: &Connection, last_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sync_outbox WHERE id <= ?1", params![last_id])?;
    Ok(())
}

/// Content hashes of the entries that sync, oldest first.
pub fn syncable_entry_hashes(conn: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut stmt = conn.prepare(
        "SELECT content_hash FROM entries WHERE local_only = 0
         GROUP BY content_hash ORDER BY MIN(last_used_at)",
    )?;
    let hashes = stmt.query_map([], |row| row.get(0))?.collect();
    hashes
}

/// Deletes from the last 30 days of content that isn't here any more.
pub fn recent_delete_hashes(conn: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut stmt = conn.prepare(
        "SELECT content_hash FROM sync_recent_deletes
         WHERE content_hash NOT IN (SELECT content_hash FROM entries)",
    )?;
    let hashes = stmt.query_map([], |row| row.get(0))?.collect();
    hashes
}

pub fn clear_sync_ids(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("UPDATE entries SET sync_id = NULL", [])?;
    Ok(())
}

/// Hashes text entries again under the account's hash key. Images keep their hash, since it
/// was taken over pixels that aren't kept, so an image may show up twice once.
pub fn rehash_text_entries(conn: &Connection, hash_key: &[u8; 32]) -> rusqlite::Result<()> {
    let texts: Vec<(String, Vec<u8>)> = {
        let mut stmt =
            conn.prepare("SELECT id, content FROM entries WHERE content_type = 'text'")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows
    };
    let mut update =
        conn.prepare("UPDATE entries SET content_hash = ?2, sync_id = NULL WHERE id = ?1")?;
    for (id, content) in texts {
        update.execute(params![
            id,
            crate::crypto::hash::keyed_hash(hash_key, &content)
        ])?;
    }
    Ok(())
}

pub fn prune_recent_deletes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM sync_recent_deletes WHERE deleted_at < datetime('now', '-30 days')",
        [],
    )?;
    Ok(())
}

/// Forgets the account this device synced with, for a logout. The hash key stays, since the
/// entries' content hashes were made with it.
pub fn clear(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DELETE FROM sync_meta WHERE key <> 'hash_key';
         DELETE FROM sync_outbox;
         DELETE FROM sync_recent_deletes;
         UPDATE entries SET sync_id = NULL;",
    )
}

fn sync_set_up(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(&format!("SELECT {SYNC_SET_UP}"), [], |row| row.get(0))
}

fn remember_delete(conn: &Connection, content_hash: &[u8]) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_recent_deletes (content_hash, deleted_at) VALUES (?1, datetime('now'))
         ON CONFLICT (content_hash) DO UPDATE SET deleted_at = excluded.deleted_at",
        params![content_hash],
    )?;
    Ok(())
}

fn drop_older_rows(conn: &Connection, content_hash: &[u8]) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM sync_outbox WHERE content_hash = ?1 AND id < ?2",
        params![content_hash, conn.last_insert_rowid()],
    )?;
    Ok(())
}

fn keep_newest_row_per_entry(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM sync_outbox WHERE id NOT IN (SELECT MAX(id) FROM sync_outbox GROUP BY content_hash)",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::entries::{self, EntryType, NewEntry};
    use crate::db::schema;

    fn db(set_up: bool) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::initialize(&conn).unwrap();
        if set_up {
            set_meta(&conn, META_KEY_BUNDLE, vec![1u8]).unwrap();
        }
        conn
    }

    fn insert(conn: &Connection, hash: u8, local_only: bool) -> String {
        entries::insert_entry(
            conn,
            &NewEntry {
                content: vec![hash],
                content_type: EntryType::Text,
                content_hash: vec![hash],
                size_bytes: 1,
                is_favorite: false,
                local_only,
            },
        )
        .unwrap()
    }

    fn outbox(conn: &Connection) -> Vec<(Vec<u8>, bool)> {
        outbox_rows(conn, 100)
            .unwrap()
            .into_iter()
            .map(|row| (row.content_hash, row.is_delete))
            .collect()
    }

    #[test]
    fn nothing_is_queued_before_sync_is_set_up() {
        let conn = db(false);
        insert(&conn, 1, false);
        queue_upsert(&conn, &[1]).unwrap();
        queue_delete(&conn, &[1], false).unwrap();
        assert!(outbox(&conn).is_empty());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM sync_recent_deletes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn local_only_entries_are_never_queued() {
        let conn = db(true);
        insert(&conn, 1, true);
        queue_upsert(&conn, &[1]).unwrap();
        queue_delete(&conn, &[1], true).unwrap();
        queue_all_entries(&conn).unwrap();
        assert!(outbox(&conn).is_empty());
    }

    #[test]
    fn a_newer_change_replaces_an_older_one_and_moves_to_the_end() {
        let conn = db(true);
        insert(&conn, 1, false);
        insert(&conn, 2, false);
        queue_upsert(&conn, &[1]).unwrap();
        queue_upsert(&conn, &[2]).unwrap();
        queue_upsert(&conn, &[1]).unwrap();
        assert_eq!(outbox(&conn), vec![(vec![2], false), (vec![1], false)]);

        queue_delete(&conn, &[2], false).unwrap();
        assert_eq!(outbox(&conn), vec![(vec![1], false), (vec![2], true)]);
    }

    #[test]
    fn recent_deletes_are_requeued_unless_copied_again() {
        let conn = db(true);
        queue_delete(&conn, &[1], false).unwrap();
        queue_delete(&conn, &[2], false).unwrap();
        conn.execute("DELETE FROM sync_outbox", []).unwrap();
        insert(&conn, 2, false);

        queue_recent_deletes(&conn).unwrap();
        assert_eq!(outbox(&conn), vec![(vec![1], true)]);
    }

    #[test]
    fn applying_never_touches_local_only_entries() {
        let conn = db(true);
        let id = insert(&conn, 1, true);
        let payload = EntryPayload {
            content_type: EntryType::Text,
            content: vec![1],
            content_hash: vec![1],
            is_favorite: true,
            last_used_at: "2026-01-01 00:00:00".into(),
        };
        assert!(!apply_entry(&conn, "remote-id", &payload, false).unwrap());
        conn.execute("UPDATE entries SET sync_id = 'remote-id'", [])
            .unwrap();
        assert!(!apply_delete(&conn, "remote-id").unwrap());

        let entry = entries::get_entry(&conn, &id).unwrap().unwrap();
        assert!(entry.local_only);
        assert!(!entry.is_favorite);
    }

    #[test]
    fn insert_only_keeps_existing_entries() {
        let conn = db(true);
        let id = insert(&conn, 1, false);
        let payload = EntryPayload {
            content_type: EntryType::Text,
            content: vec![1],
            content_hash: vec![1],
            is_favorite: true,
            last_used_at: "2026-01-01 00:00:00".into(),
        };
        assert!(!apply_entry(&conn, "remote-id", &payload, true).unwrap());
        assert!(!entries::get_entry(&conn, &id).unwrap().unwrap().is_favorite);

        assert!(apply_entry(&conn, "remote-id", &payload, false).unwrap());
        assert!(entries::get_entry(&conn, &id).unwrap().unwrap().is_favorite);
    }
}
