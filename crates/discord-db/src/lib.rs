//! discord-db: SQLite archive for Discord messages.

pub mod db;

/// A row in the `messages` table.
#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub author_id: String,
    pub author_name: String,
    pub content: String,
    /// UTC RFC3339 string.
    pub timestamp: String,
    pub edited: bool,
    pub reaction_count: u32,
}

/// A full-text search hit (FTS5 join result).
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub channel_id: String,
    pub channel_name: String,
    pub guild_name: String,
    pub author_name: String,
    pub content: String,
    pub timestamp: String,
    pub rank: f64,
}
