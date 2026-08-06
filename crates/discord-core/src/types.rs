//! Serde types for Discord entities (Guild, Channel, Message, User, DM).
//!
//! Field shapes match jackwener `_parse_message` / `list_guilds` /
//! `list_channels` (Apache-2.0, `.tmp/`) and famasya `MessageItem` (MIT).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Discord snowflake ID (u64). String comparisons are used for cursors.
pub type Snowflake = u64;

/// Convert a snowflake to UTC datetime.
/// `ms = (id >> 22) + DISCORD_EPOCH` (jackwener client.py).
pub fn snowflake_to_datetime(id: Snowflake) -> DateTime<Utc> {
    const DISCORD_EPOCH: u64 = 1420070400000;
    let ms = (id >> 22) + DISCORD_EPOCH;
    DateTime::from_timestamp_millis(ms as i64).unwrap_or_default()
}

/// Convert a datetime to a snowflake (for `after` cursor).
/// `ms = ts_ms - DISCORD_EPOCH; snowflake = ms << 22`.
pub fn datetime_to_snowflake(dt: DateTime<Utc>) -> Snowflake {
    const DISCORD_EPOCH: u64 = 1420070400000;
    let ms = dt.timestamp_millis() as u64;
    (ms.saturating_sub(DISCORD_EPOCH)) << 22
}

/// A guild the user has joined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<bool>,
}

/// A channel within a guild (or DM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    /// 0=text, 2=voice, 5=announcement, 15=forum, 16=media
    #[serde(rename = "type")]
    pub channel_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
}

impl Channel {
    /// True for text-like channels (text/announcement/forum).
    pub fn is_text_like(&self) -> bool {
        matches!(self.channel_type, 0 | 5 | 15)
    }
}

/// A message as parsed for output (famasya `MessageItem`-shaped).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: String,
    pub channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    pub author: String,
    pub timestamp: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<String>>,
}

/// Current user profile (`GET /users/@me`). Field shapes match the
/// Discord REST response (mirrors discord-user-rs `MeResponse`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub mfa_enabled: bool,
    #[serde(default)]
    pub premium_type: u32,
}

/// A guild member (jackwener list_members shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub bot: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Guild info with counts (jackwener get_guild_info shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_count: Option<u32>,
}

/// A DM or group-DM channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmChannel {
    pub id: String,
    /// Human label: user#disc for DMs, joined tags for group DMs.
    pub label: String,
    #[serde(rename = "type")]
    pub channel_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_count: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snowflake_roundtrip() {
        // Discord epoch is 2015-01-01; a known id.
        let dt = snowflake_to_datetime(123456789012345678);
        // Just verify it parses to a plausible date and round-trips approx.
        let back = datetime_to_snowflake(dt);
        // (id >> 22) << 22 loses low 22 bits — compare upper bits only.
        assert_eq!(back >> 22, 123456789012345678u64 >> 22);
    }

    #[test]
    fn channel_text_like() {
        let text = Channel { id: "1".into(), name: "g".into(), guild_id: None, channel_type: 0, topic: None, parent_id: None, position: None };
        let forum = Channel { id: "2".into(), name: "f".into(), guild_id: None, channel_type: 15, topic: None, parent_id: None, position: None };
        let voice = Channel { id: "3".into(), name: "v".into(), guild_id: None, channel_type: 2, topic: None, parent_id: None, position: None };
        assert!(text.is_text_like());
        assert!(forum.is_text_like());
        assert!(!voice.is_text_like());
    }

    #[test]
    fn message_serializes_agent_friendly() {
        let m = Message {
            message_id: "1".into(),
            channel_id: "c".into(),
            guild_id: Some("g".into()),
            author_id: Some("u".into()),
            author: "alice".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            content: "hello".into(),
            attachments: None,
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["message_id"], "1");
        assert_eq!(v["content"], "hello");
    }
}
