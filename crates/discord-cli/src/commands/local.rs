//! Local offline queries over the SQLite archive (plan §9).
//! `search`, `recent`, `stats`, `top` — top-level (not under `dc`).

use std::process::ExitCode;

use discord_core::config;
use discord_core::output::{self, Format, exit};
use discord_db::db as ddb;

/// `search <KEYWORD>` — FTS5 search of local archive.
pub fn cmd_search(query: &str, channel: Option<&str>, limit: usize, format: Format) -> ExitCode {
    let db_path = match config::db_path() {
        Ok(p) => p,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    // Basic per-channel filter is applied post-hoc (query is FTS5-native).
    match ddb::search_messages(&conn, query, limit as i64) {
        Ok(mut hits) => {
            if let Some(ch) = channel {
                hits.retain(|h| h.channel_name.to_lowercase().contains(&ch.to_lowercase()));
            }
            let _ = output::emit(&hits, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    }
}

/// `recent [-c CH] [--hours N]` — newest stored messages.
pub fn cmd_recent(
    channel: Option<&str>,
    hours: Option<i64>,
    limit: usize,
    format: Format,
) -> ExitCode {
    let db_path = match config::db_path() {
        Ok(p) => p,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    match ddb::recent_messages(&conn, channel, hours, limit as i64) {
        Ok(hits) => {
            let _ = output::emit(&hits, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    }
}

/// `stats` — per-channel message counts.
pub fn cmd_stats(format: Format) -> ExitCode {
    let db_path = match config::db_path() {
        Ok(p) => p,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    match ddb::channel_stats(&conn) {
        Ok(stats) => {
            let _ = output::emit(&stats, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    }
}

/// `export <CHANNEL> [-f text|json] [-o FILE]` — export stored messages.
pub fn cmd_export(
    channel: &str,
    as_json: bool,
    output: Option<&str>,
    format: Format,
) -> ExitCode {
    let db_path = match config::db_path() {
        Ok(p) => p,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let channel_id = channel.to_string();
    match ddb::channel_messages(&conn, &channel_id, 1_000_000) {
        Ok(rows) => {
            let text = if as_json {
                serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".into())
            } else {
                rows.iter()
                    .map(|r| format!("[{}] {}: {}", r.timestamp, r.author_name, r.content))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            if let Some(path) = output {
                match std::fs::write(path, &text) {
                    Ok(_) => {
                        let data = serde_json::json!({ "exported": true, "file": path, "messages": rows.len() });
                        let _ = output::emit(&data, format);
                        ExitCode::from(exit::OK)
                    }
                    Err(e) => {
                        ExitCode::from(output::emit_error("IOError", &e.to_string(), exit::ERROR))
                    }
                }
            } else {
                println!("{}", text);
                ExitCode::from(exit::OK)
            }
        }
        Err(e) => ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    }
}

/// `purge <CHANNEL> [-y]` — delete stored messages for a channel.
pub fn cmd_purge(channel: &str, yes: bool, format: Format) -> ExitCode {
    if !yes {
        eprintln!("This will delete stored messages for channel \"{channel}\". Add -y to proceed.");
        return ExitCode::from(exit::USAGE);
    }
    let db_path = match config::db_path() {
        Ok(p) => p,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    match ddb::purge_channel(&conn, channel) {
        Ok(n) => {
            let data = serde_json::json!({ "purged": true, "channel": channel, "messages_deleted": n });
            let _ = output::emit(&data, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    }
}

/// `top [-c CH]` — top senders.
pub fn cmd_top(channel: Option<&str>, limit: usize, format: Format) -> ExitCode {
    let db_path = match config::db_path() {
        Ok(p) => p,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => return ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    };
    match ddb::top_senders(&conn, channel, limit as i64) {
        Ok(senders) => {
            let _ = output::emit(&senders, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("DbError", &e.to_string(), exit::ERROR)),
    }
}
