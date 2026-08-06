//! Two-phase incremental sync to SQLite (langkurt pattern).
//!
//! Phase A (new, forward): if a last_message_id exists, fetch messages after it.
//! Phase B (backward, history): resume from oldest_message_id, paginate backward.
//! Persist max/min cursors via `sync_state`.

use anyhow::Result;
use discord_core::client::ApiClient;
use discord_core::config;
use discord_db::db as ddb;
use discord_db::MessageRow;

/// Sync one channel into SQLite. Returns message count written.
pub async fn sync_channel(
    client: &mut ApiClient,
    channel_id: &str,
    limit: usize,
) -> Result<usize> {
    let db_path = config::db_path()?;
    let conn = ddb::open(db_path.to_str().unwrap_or("discord.db"))?;

    // Ensure the channel exists in the DB first (foreign-key requirement).
    // Upsert channel with a placeholder name; the archive query joins on it.
    ddb::upsert_channel(&conn, channel_id, None, channel_id, 0, None, None)?;

    let (last_id, oldest_id) = ddb::get_sync_state(&conn, channel_id)?;
    let mut total = 0usize;

    // Phase B (history backward) — always runs to backfill.
    let before = if oldest_id.is_empty() { None } else { oldest_id.parse().ok() };
    let msgs = client.fetch_messages(channel_id, limit, before, None).await?;
    for m in &msgs {
        ddb::upsert_message(&conn, &row_from_msg(m, channel_id))?;
    }
    total += msgs.len();

    // Phase A (new, forward) — only when we have a last cursor.
    if !last_id.is_empty() {
        let after: Option<u64> = last_id.parse().ok();
        let new_msgs = client.fetch_messages(channel_id, limit, None, after).await?;
        for m in &new_msgs {
            ddb::upsert_message(&conn, &row_from_msg(m, channel_id))?;
        }
        total += new_msgs.len();
    }

    // Compute new cursors: newest = max id seen, oldest = min id seen.
    let newest = msgs
        .iter()
        .map(|m| m.message_id.clone())
        .max()
        .unwrap_or_default();
    let oldest = msgs
        .iter()
        .map(|m| m.message_id.clone())
        .min()
        .unwrap_or_default();
    if !newest.is_empty() {
        ddb::update_sync_state(&conn, channel_id, &newest, &oldest)?;
    }
    Ok(total)
}

/// Convert a core Message to a db MessageRow.
fn row_from_msg(m: &discord_core::types::Message, channel_id: &str) -> MessageRow {
    MessageRow {
        id: m.message_id.clone(),
        channel_id: channel_id.to_string(),
        guild_id: m.guild_id.clone(),
        author_id: m.author_id.clone().unwrap_or_default(),
        author_name: m.author.clone(),
        content: m.content.clone(),
        timestamp: m.timestamp.clone(),
        edited: false,
        reaction_count: 0,
    }
}
