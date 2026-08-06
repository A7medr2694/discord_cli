//! SQLite archive: schema, WAL, upserts, FTS5 search, sync state.
//!
//! Schema ported from langkurt `storage/db.go` (MIT, `.tmp/`) + jackwener
//! `db.py` (Apache-2.0). Verified in plan §6.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

/// Open the database at `path`, apply schema, return the connection.
/// WAL + foreign_keys; single-writer semantics.
pub fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(path).context("open sqlite")?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .context("pragmas")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Apply schema migrations (idempotent).
fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS guilds (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, icon TEXT
        );
        CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY, guild_id TEXT REFERENCES guilds(id),
            name TEXT NOT NULL, type INTEGER NOT NULL DEFAULT 0,
            topic TEXT, parent_id TEXT
        );
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            channel_id TEXT NOT NULL REFERENCES channels(id),
            guild_id TEXT,
            author_id TEXT NOT NULL, author_name TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp DATETIME NOT NULL,
            edited INTEGER NOT NULL DEFAULT 0,
            reaction_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id);
        CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_messages_author ON messages(author_id);

        CREATE TABLE IF NOT EXISTS sync_state (
            channel_id TEXT PRIMARY KEY,
            last_message_id TEXT,
            oldest_message_id TEXT,
            synced_at DATETIME
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content, author_name,
            content='messages', content_rowid='rowid',
            tokenize='unicode61 remove_diacritics 1'
        );
        CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, content, author_name) VALUES (new.rowid, new.content, new.author_name);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content, author_name) VALUES('delete', old.rowid, old.content, old.author_name);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content, author_name) VALUES('delete', old.rowid, old.content, old.author_name);
            INSERT INTO messages_fts(rowid, content, author_name) VALUES (new.rowid, new.content, new.author_name);
        END;
        "#,
    )
    .context("schema migration")?;
    Ok(())
}

/// Upsert a message (INSERT OR REPLACE). Returns whether a new row was inserted.
pub fn upsert_message(
    conn: &Connection,
    msg: &crate::MessageRow,
) -> Result<bool> {
    let changed = conn
        .execute(
            r#"
            INSERT OR REPLACE INTO messages
                (id, channel_id, guild_id, author_id, author_name, content, timestamp, edited, reaction_count)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                msg.id,
                msg.channel_id,
                msg.guild_id,
                msg.author_id,
                msg.author_name,
                msg.content,
                msg.timestamp,
                msg.edited,
                msg.reaction_count,
            ],
        )
        .context("upsert message")?;
    Ok(changed > 0)
}

/// Upsert a guild (INSERT OR REPLACE).
pub fn upsert_guild(conn: &Connection, id: &str, name: &str, icon: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO guilds (id, name, icon) VALUES (?1, ?2, ?3)",
        params![id, name, icon],
    )
    .context("upsert guild")?;
    Ok(())
}

/// Upsert a channel (INSERT OR REPLACE).
pub fn upsert_channel(
    conn: &Connection,
    id: &str,
    guild_id: Option<&str>,
    name: &str,
    channel_type: u8,
    topic: Option<&str>,
    parent_id: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO channels (id, guild_id, name, type, topic, parent_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, guild_id, name, channel_type, topic, parent_id],
    )
    .context("upsert channel")?;
    Ok(())
}

/// Read sync state for a channel. Returns (last_message_id, oldest_message_id).
pub fn get_sync_state(conn: &Connection, channel_id: &str) -> Result<(String, String)> {
    let mut stmt = conn
        .prepare("SELECT COALESCE(last_message_id,''), COALESCE(oldest_message_id,'') FROM sync_state WHERE channel_id = ?1")
        .context("prepare sync_state")?;
    let mut rows = stmt.query(params![channel_id]).context("query sync_state")?;
    if let Some(row) = rows.next().context("next sync_state")? {
        let last: String = row.get(0)?;
        let oldest: String = row.get(1)?;
        Ok((last, oldest))
    } else {
        Ok((String::new(), String::new()))
    }
}

/// Update sync state with max/min cursor semantics (langkurt).
pub fn update_sync_state(
    conn: &Connection,
    channel_id: &str,
    newest_id: &str,
    oldest_id: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO sync_state (channel_id, last_message_id, oldest_message_id, synced_at)
        VALUES (?1, ?2, ?3, datetime('now'))
        ON CONFLICT(channel_id) DO UPDATE SET
            last_message_id = CASE WHEN excluded.last_message_id > last_message_id
                                   THEN excluded.last_message_id ELSE last_message_id END,
            oldest_message_id = CASE WHEN oldest_message_id='' OR excluded.oldest_message_id < oldest_message_id
                                     THEN excluded.oldest_message_id ELSE oldest_message_id END,
            synced_at = excluded.synced_at
        "#,
        params![channel_id, newest_id, oldest_id],
    )
    .context("update sync_state")?;
    Ok(())
}

/// FTS5 full-text search over stored messages (langkurt SQL, bind verbatim).
pub fn search_messages(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> Result<Vec<crate::SearchHit>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT m.id, m.channel_id, c.name AS channel_name,
                   COALESCE(g.name,'DM') AS guild_name, m.author_name, m.content,
                   m.timestamp, rank
            FROM messages_fts
            JOIN messages m ON messages_fts.rowid = m.rowid
            JOIN channels c ON m.channel_id = c.id
            LEFT JOIN guilds g ON m.guild_id = g.id
            WHERE messages_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
        )
        .context("prepare search")?;
    let rows = stmt
        .query_map(params![query, limit], |row| {
            Ok(crate::SearchHit {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                channel_name: row.get(2)?,
                guild_name: row.get(3)?,
                author_name: row.get(4)?,
                content: row.get(5)?,
                timestamp: row.get(6)?,
                rank: row.get(7)?,
            })
        })
        .context("query search")?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.context("search row")?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn schema_migrates_clean() {
        let conn = temp_db();
        // Verify key tables exist.
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for t in ["messages", "channels", "guilds", "sync_state", "messages_fts"] {
            assert!(tables.contains(&t.to_string()), "missing table {t}: {tables:?}");
        }
    }

    #[test]
    fn upsert_message_dedups() {
        let conn = temp_db();
        upsert_guild(&conn, "g1", "Test Guild", None).unwrap();
        upsert_channel(&conn, "c1", Some("g1"), "general", 0, None, None).unwrap();
        let msg = crate::MessageRow {
            id: "1".into(),
            channel_id: "c1".into(),
            guild_id: Some("g1".into()),
            author_id: "u1".into(),
            author_name: "alice".into(),
            content: "hello".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            edited: false,
            reaction_count: 0,
        };
        upsert_message(&conn, &msg).unwrap();
        // Insert same id again — should replace, count still 1.
        upsert_message(&conn, &msg).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn fts_search_after_insert() {
        let conn = temp_db();
        upsert_guild(&conn, "g1", "Test Guild", None).unwrap();
        upsert_channel(&conn, "c1", Some("g1"), "general", 0, None, None).unwrap();
        let msg = crate::MessageRow {
            id: "1".into(),
            channel_id: "c1".into(),
            guild_id: Some("g1".into()),
            author_id: "u1".into(),
            author_name: "alice".into(),
            content: "the quick brown fox".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            edited: false,
            reaction_count: 0,
        };
        upsert_message(&conn, &msg).unwrap();
        let hits = search_messages(&conn, "quick", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].channel_name, "general");
        assert_eq!(hits[0].guild_name, "Test Guild");
    }

    #[test]
    fn sync_state_max_min_cursors() {
        let conn = temp_db();
        update_sync_state(&conn, "c1", "500", "100").unwrap();
        // newer last, older oldest
        update_sync_state(&conn, "c1", "700", "050").unwrap();
        let (last, oldest) = get_sync_state(&conn, "c1").unwrap();
        assert_eq!(last, "700");
        assert_eq!(oldest, "050");
    }
}
