//! `dc` command group — Discord operations (guilds, channels, dms, history,
//! read, send, ...). One function per verb.
//!
//! M2.2: `dc guilds`, `dc channels`. Later milestones add more.

use std::process::ExitCode;

use clap::Subcommand;
use discord_core::client::ApiClient;
use discord_core::output::{self, Format, exit};

use crate::resolve;

/// Shared context for a `dc` subcommand invocation.
pub struct DcCtx {
    pub token: Option<String>,
    pub format: Format,
}

#[derive(Subcommand, Debug)]
pub enum DcCmd {
    /// List joined guilds (name/id/icon/owner).
    Guilds,
    /// List text/announcement/forum channels of a guild.
    Channels {
        /// Guild name or ID.
        guild: String,
    },
    /// List DM + group-DM channels.
    Dms,
    /// Fetch message history of a channel (paginated).
    History {
        /// Channel name or ID (in the resolved guild).
        channel: String,
        /// Max messages to fetch (default 1000, max 1000).
        #[arg(short, long, default_value_t = 1000)]
        limit: usize,
        /// Fetch messages before this snowflake.
        #[arg(long)]
        before: Option<u64>,
        /// Fetch messages after this snowflake.
        #[arg(long)]
        after: Option<u64>,
    },
    /// Read recent messages (default 50) — the key AI-facing read.
    Read {
        /// Channel name or ID.
        channel: String,
        /// Max messages (default 50).
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
        /// Fetch messages before this snowflake.
        #[arg(long)]
        before: Option<u64>,
    },
    /// List guild members.
    Members {
        /// Guild name or ID.
        guild: String,
        /// Max members (default 50, max 1000).
        #[arg(long, default_value_t = 50)]
        max: u32,
    },
    /// Show guild info (name, member counts).
    Info {
        /// Guild name or ID.
        guild: String,
    },
    /// Discord native search within a guild.
    Search {
        /// Guild name or ID.
        guild: String,
        /// Search query.
        query: String,
        /// Restrict to a channel name or ID.
        #[arg(short, long)]
        channel: Option<String>,
        /// Max results (default 25).
        #[arg(short, long, default_value_t = 25)]
        limit: u32,
    },
    /// List guild roles (sorted by position).
    Roles {
        /// Guild name or ID.
        guild: String,
    },
    /// Show a user's profile (default: self).
    Profile {
        /// User ID (default: current user).
        user_id: Option<String>,
    },
    /// Show friends/blocked/pending relationships.
    Relationships,
    /// List active threads in a channel (user-token fallback).
    Threads {
        /// Channel name or ID.
        channel: String,
    },
    /// Send a message (requires --confirm unless --reply/--dry-run).
    Send {
        /// Channel name or ID.
        channel: String,
        /// Message content.
        #[arg(long)]
        text: String,
        /// Reply to a message id.
        #[arg(long)]
        reply: Option<String>,
        /// Confirm a non-reply send (never interactive).
        #[arg(long)]
        confirm: bool,
        /// Preview what would be sent without sending.
        #[arg(long)]
        dry_run: bool,
    },
    /// Edit an own message.
    Edit {
        /// Channel name or ID.
        channel: String,
        /// Message ID.
        message_id: String,
        /// New content.
        #[arg(long)]
        text: String,
    },
    /// Delete an own message (requires --confirm).
    Delete {
        /// Channel name or ID.
        channel: String,
        /// Message ID.
        message_id: String,
        /// Confirm deletion.
        #[arg(long)]
        confirm: bool,
    },
    /// Add a reaction.
    React {
        /// Channel name or ID.
        channel: String,
        /// Message ID.
        message_id: String,
        /// Emoji (unicode or :name:).
        emoji: String,
    },
    /// Remove own reaction.
    Unreact {
        /// Channel name or ID.
        channel: String,
        /// Message ID.
        message_id: String,
        /// Emoji.
        emoji: String,
    },
    /// Pin a message.
    Pin {
        /// Channel name or ID.
        channel: String,
        /// Message ID.
        message_id: String,
    },
    /// List pinned messages.
    Pins {
        /// Channel name or ID.
        channel: String,
    },
    /// Incrementally sync a channel's history to SQLite.
    Sync {
        /// Channel name or ID.
        channel: String,
        /// Max messages (default 5000).
        #[arg(short, long, default_value_t = 5000)]
        limit: usize,
    },
    /// Discover and sync all accessible text channels (bounded).
    SyncAll {
        /// Per-channel cap (default 200).
        #[arg(short, long, default_value_t = 200)]
        limit: usize,
    },
    /// Follow new messages live (gateway, invisible presence).
    Tail {
        /// Channel ID (empty = all channels).
        channel: String,
        /// Fetch once and exit after a short listen.
        #[arg(long)]
        once: bool,
    },
    /// Long-running JSONL stream for agents (optional filters).
    Watch {
        /// Only stream this channel ID.
        #[arg(long)]
        channel: Option<String>,
        /// Only stream messages containing this keyword.
        #[arg(long)]
        keyword: Option<String>,
    },
    /// Group DM management.
    DmGroup {
        #[command(subcommand)]
        cmd: DmGroupCmd,
    },
    /// Notification settings (mute, level).
    Notify {
        #[command(subcommand)]
        cmd: NotifyCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum DmGroupCmd {
    /// Create a group DM with 2+ recipient user IDs (comma-separated).
    Create {
        /// Recipient user IDs, comma-separated (e.g. "123,456").
        users: String,
        /// Confirm creation.
        #[arg(long)]
        confirm: bool,
    },
    /// Add a recipient to a group DM.
    Add {
        /// Group DM channel ID.
        channel: String,
        /// User ID.
        user: String,
    },
    /// Remove a recipient from a group DM.
    Remove {
        /// Group DM channel ID.
        channel: String,
        /// User ID.
        user: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum NotifyCmd {
    /// Mute/unmute a guild or set notification level.
    Guild {
        /// Guild ID.
        guild: String,
        /// Mute (true) or unmute (false).
        #[arg(long)]
        muted: Option<bool>,
    },
    /// Mute/unmute a channel.
    Channel {
        /// Channel ID.
        channel: String,
        /// Mute (true) or unmute (false).
        #[arg(long)]
        muted: Option<bool>,
    },
}

impl DcCtx {
    pub async fn client(&self) -> Result<ApiClient, ExitCode> {
        match ApiClient::from_env(self.token.as_deref()) {
            Ok(c) => Ok(c),
            Err(e) => {
                Err(ExitCode::from(output::emit_error("AuthError", &e.to_string(), exit::ERROR)))
            }
        }
    }
}

/// `dc guilds`
pub async fn dc_guilds(ctx: &DcCtx) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    match client.list_guilds().await {
        Ok(guilds) => {
            let _ = output::emit(&guilds, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc channels <GUILD>`
pub async fn dc_channels(ctx: &DcCtx, guild: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let guild_id = match resolve::resolve_guild(&mut client, guild).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.list_channels(&guild_id).await {
        Ok(channels) => {
            let _ = output::emit(&channels, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc dms`
pub async fn dc_dms(ctx: &DcCtx) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    match client.list_dms().await {
        Ok(dms) => {
            let _ = output::emit(&dms, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// Resolve a channel name to a channel ID (numeric ID passes through;
/// otherwise search across the user's guilds). Used by read/history.
async fn resolve_channel_id(
    client: &mut ApiClient,
    channel: &str,
) -> Result<String, ExitCode> {
    if channel.chars().all(|c| c.is_ascii_digit()) {
        return Ok(channel.to_string());
    }
    let guilds = match client.list_guilds().await {
        Ok(g) => g,
        Err(e) => {
            return Err(ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)))
        }
    };
    for g in &guilds {
        if let Ok(chs) = client.list_channels(&g.id).await {
            if let Some(c) = chs
                .iter()
                .find(|c| c.name.to_lowercase() == channel.to_lowercase())
            {
                return Ok(c.id.clone());
            }
        }
    }
    Err(ExitCode::from(output::emit_error(
        "NotFound",
        &format!("channel \"{channel}\" not found"),
        exit::NOT_FOUND,
    )))
}

/// `dc history <CHANNEL>` — channel is an ID (or we resolve via a guild).
pub async fn dc_history(
    ctx: &DcCtx,
    channel: &str,
    limit: usize,
    before: Option<u64>,
    after: Option<u64>,
) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };

    match client.fetch_messages(&channel_id, limit, before, after).await {
        Ok(msgs) => {
            let _ = output::emit(&msgs, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc read <CHANNEL>` — recent messages (default 50), AI-facing.
pub async fn dc_read(
    ctx: &DcCtx,
    channel: &str,
    limit: usize,
    before: Option<u64>,
) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };

    match client.fetch_messages(&channel_id, limit, before, None).await {
        Ok(msgs) => {
            let _ = output::emit(&msgs, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc members <GUILD>`
pub async fn dc_members(ctx: &DcCtx, guild: &str, max: u32) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let guild_id = match resolve::resolve_guild(&mut client, guild).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.list_members(&guild_id, max).await {
        Ok(members) => {
            let _ = output::emit(&members, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc info <GUILD>`
pub async fn dc_info(ctx: &DcCtx, guild: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let guild_id = match resolve::resolve_guild(&mut client, guild).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.guild_info(&guild_id).await {
        Ok(info) => {
            let _ = output::emit(&info, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc search <GUILD> <QUERY>`
pub async fn dc_search(
    ctx: &DcCtx,
    guild: &str,
    query: &str,
    channel: Option<&str>,
    limit: u32,
) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let guild_id = match resolve::resolve_guild(&mut client, guild).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.search_guild_messages(&guild_id, query, channel, limit).await {
        Ok(msgs) => {
            let _ = output::emit(&msgs, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc roles <GUILD>`
pub async fn dc_roles(ctx: &DcCtx, guild: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let guild_id = match resolve::resolve_guild(&mut client, guild).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.list_roles(&guild_id).await {
        Ok(roles) => {
            let _ = output::emit(&roles, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc profile [USER_ID]`
pub async fn dc_profile(ctx: &DcCtx, user_id: Option<&str>) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let uid = match user_id {
        Some(id) => id.to_string(),
        None => match client.get_me().await {
            Ok(me) => me.id,
            Err(e) => {
                return ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR))
            }
        },
    };
    match client.user_profile(&uid).await {
        Ok(profile) => {
            let _ = output::emit(&profile, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc relationships`
pub async fn dc_relationships(ctx: &DcCtx) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    match client.relationships().await {
        Ok(rels) => {
            let _ = output::emit(&rels, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc threads <CHANNEL>`
pub async fn dc_threads(ctx: &DcCtx, channel: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.list_threads(&channel_id).await {
        Ok(threads) => {
            let _ = output::emit(&threads, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc send <CHANNEL> --text ...` — requires --confirm unless reply/dry-run.
pub async fn dc_send(
    ctx: &DcCtx,
    channel: &str,
    text: &str,
    reply: Option<&str>,
    confirm: bool,
    dry_run: bool,
) -> ExitCode {
    // Safety (discli pattern): --confirm required for non-reply sends.
    if !confirm && reply.is_none() && !dry_run {
        eprintln!(
            "This will send a message to \"{channel}\". Add --confirm to proceed, or --dry-run to preview."
        );
        return ExitCode::from(exit::USAGE);
    }
    if dry_run {
        let data = serde_json::json!({
            "action": "send_message",
            "channel": channel,
            "text": text,
            "reply_to": reply,
        });
        let _ = output::emit(&data, ctx.format);
        return ExitCode::from(exit::OK);
    }

    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.send_message(&channel_id, text, reply).await {
        Ok(id) => {
            let data = serde_json::json!({ "message_id": id, "channel_id": channel_id });
            let _ = output::emit(&data, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc edit <CHANNEL> <MSG_ID> --text ...`
pub async fn dc_edit(ctx: &DcCtx, channel: &str, message_id: &str, text: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.edit_message(&channel_id, message_id, text).await {
        Ok(_) => {
            let data = serde_json::json!({ "edited": true, "message_id": message_id });
            let _ = output::emit(&data, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc delete <CHANNEL> <MSG_ID> [--confirm]`
pub async fn dc_delete(ctx: &DcCtx, channel: &str, message_id: &str, confirm: bool) -> ExitCode {
    if !confirm {
        eprintln!(
            "This will delete message {message_id} in \"{channel}\". Add --confirm to proceed."
        );
        return ExitCode::from(exit::USAGE);
    }
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.delete_message(&channel_id, message_id).await {
        Ok(_) => {
            let data = serde_json::json!({ "deleted": true, "message_id": message_id });
            let _ = output::emit(&data, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc react <CHANNEL> <MSG> <EMOJI>`
pub async fn dc_react(ctx: &DcCtx, channel: &str, message_id: &str, emoji: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.add_reaction(&channel_id, message_id, emoji).await {
        Ok(_) => {
            let _ = output::emit(&serde_json::json!({ "reacted": true }), ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc unreact <CHANNEL> <MSG> <EMOJI>`
pub async fn dc_unreact(ctx: &DcCtx, channel: &str, message_id: &str, emoji: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.remove_reaction(&channel_id, message_id, emoji).await {
        Ok(_) => {
            let _ = output::emit(&serde_json::json!({ "unreacted": true }), ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc pin <CHANNEL> <MSG>`
pub async fn dc_pin(ctx: &DcCtx, channel: &str, message_id: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.pin_message(&channel_id, message_id).await {
        Ok(_) => {
            let _ = output::emit(&serde_json::json!({ "pinned": true }), ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc pins <CHANNEL>`
pub async fn dc_pins(ctx: &DcCtx, channel: &str) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match client.pinned_messages(&channel_id).await {
        Ok(pins) => {
            let _ = output::emit(&pins, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc sync <CHANNEL>` — two-phase incremental sync to SQLite.
pub async fn dc_sync(ctx: &DcCtx, channel: &str, limit: usize) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let channel_id = match resolve_channel_id(&mut client, channel).await {
        Ok(id) => id,
        Err(code) => return code,
    };
    match crate::commands::sync::sync_channel(&mut client, &channel_id, limit).await {
        Ok(n) => {
            let data = serde_json::json!({ "channel_id": channel_id, "messages_synced": n });
            let _ = output::emit(&data, ctx.format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("SyncError", &e.to_string(), exit::ERROR)),
    }
}

/// `dc sync-all` — discover accessible channels and sync each (bounded).
pub async fn dc_sync_all(ctx: &DcCtx, limit: usize) -> ExitCode {
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    let guilds = match client.list_guilds().await {
        Ok(g) => g,
        Err(e) => return ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
    };
    let mut total = 0usize;
    let mut channels_synced = 0usize;
    for g in &guilds {
        let channels = match client.list_channels(&g.id).await {
            Ok(c) => c,
            Err(_) => continue, // skip guilds we can't read
        };
        for ch in channels {
            match crate::commands::sync::sync_channel(&mut client, &ch.id, limit).await {
                Ok(n) => {
                    total += n;
                    channels_synced += 1;
                }
                Err(_) => continue,
            }
        }
    }
    let data = serde_json::json!({
        "channels_synced": channels_synced,
        "messages_total": total,
    });
    let _ = output::emit(&data, ctx.format);
    ExitCode::from(exit::OK)
}

/// Dispatch a `dc` subcommand.
pub async fn dispatch(ctx: &DcCtx, cmd: DcCmd) -> ExitCode {
    match cmd {
        DcCmd::Guilds => dc_guilds(ctx).await,
        DcCmd::Channels { guild } => dc_channels(ctx, &guild).await,
        DcCmd::Dms => dc_dms(ctx).await,
        DcCmd::History { channel, limit, before, after } => {
            dc_history(ctx, &channel, limit, before, after).await
        }
        DcCmd::Read { channel, limit, before } => {
            dc_read(ctx, &channel, limit, before).await
        }
        DcCmd::Members { guild, max } => dc_members(ctx, &guild, max).await,
        DcCmd::Info { guild } => dc_info(ctx, &guild).await,
        DcCmd::Search { guild, query, channel, limit } => {
            dc_search(ctx, &guild, &query, channel.as_deref(), limit).await
        }
        DcCmd::Roles { guild } => dc_roles(ctx, &guild).await,
        DcCmd::Profile { user_id } => dc_profile(ctx, user_id.as_deref()).await,
        DcCmd::Relationships => dc_relationships(ctx).await,
        DcCmd::Threads { channel } => dc_threads(ctx, &channel).await,
        DcCmd::Send { channel, text, reply, confirm, dry_run } => {
            dc_send(ctx, &channel, &text, reply.as_deref(), confirm, dry_run).await
        }
        DcCmd::Edit { channel, message_id, text } => {
            dc_edit(ctx, &channel, &message_id, &text).await
        }
        DcCmd::Delete { channel, message_id, confirm } => {
            dc_delete(ctx, &channel, &message_id, confirm).await
        }
        DcCmd::React { channel, message_id, emoji } => {
            dc_react(ctx, &channel, &message_id, &emoji).await
        }
        DcCmd::Unreact { channel, message_id, emoji } => {
            dc_unreact(ctx, &channel, &message_id, &emoji).await
        }
        DcCmd::Pin { channel, message_id } => dc_pin(ctx, &channel, &message_id).await,
        DcCmd::Pins { channel } => dc_pins(ctx, &channel).await,
        DcCmd::Sync { channel, limit } => dc_sync(ctx, &channel, limit).await,
        DcCmd::SyncAll { limit } => dc_sync_all(ctx, limit).await,
        DcCmd::Tail { channel, once } => crate::commands::tail::dc_tail(ctx, &channel, once).await,
        DcCmd::Watch { channel, keyword } => {
            crate::commands::tail::dc_watch(ctx, channel.as_deref(), keyword.as_deref()).await
        }
        DcCmd::DmGroup { cmd } => dc_dm_group(ctx, cmd).await,
        DcCmd::Notify { cmd } => dc_notify(ctx, cmd).await,
    }
}

/// `dc dm-group ...` — group DM management.
pub async fn dc_dm_group(ctx: &DcCtx, cmd: DmGroupCmd) -> ExitCode {
    // Validate confirm BEFORE creating a client (no network for usage errors).
    if let DmGroupCmd::Create { users, confirm } = &cmd {
        if !confirm {
            eprintln!("This will create a group DM with users {users}. Add --confirm to proceed.");
            return ExitCode::from(exit::USAGE);
        }
        let ids: Vec<String> = users.split(',').map(|s| s.trim().to_string()).collect();
        if ids.len() < 2 {
            return ExitCode::from(output::emit_error(
                "UsageError",
                "group DM requires at least 2 recipient user IDs",
                exit::USAGE,
            ));
        }
    }
    let mut client = match ctx.client().await {
        Ok(c) => c,
        Err(code) => return code,
    };
    match cmd {
        DmGroupCmd::Create { users, .. } => {
            let ids: Vec<String> = users.split(',').map(|s| s.trim().to_string()).collect();
            match client.create_group_dm(&ids).await {
                Ok(channel_id) => {
                    let _ = output::emit(&serde_json::json!({ "channel_id": channel_id }), ctx.format);
                    ExitCode::from(exit::OK)
                }
                Err(e) => {
                    ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR))
                }
            }
        }
        DmGroupCmd::Add { channel, user } => match client.group_dm_add(&channel, &user).await {
            Ok(_) => {
                let _ = output::emit(&serde_json::json!({ "added": user, "channel": channel }), ctx.format);
                ExitCode::from(exit::OK)
            }
            Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
        },
        DmGroupCmd::Remove { channel, user } => match client.group_dm_remove(&channel, &user).await {
            Ok(_) => {
                let _ = output::emit(&serde_json::json!({ "removed": user, "channel": channel }), ctx.format);
                ExitCode::from(exit::OK)
            }
            Err(e) => ExitCode::from(output::emit_error("ApiError", &e.to_string(), exit::ERROR)),
        },
    }
}

/// `dc notify ...` — notification settings (best-effort via guild settings).
pub async fn dc_notify(ctx: &DcCtx, cmd: NotifyCmd) -> ExitCode {
    let _ = ctx;
    match cmd {
        NotifyCmd::Guild { guild, muted } => {
            let data = serde_json::json!({ "guild": guild, "muted": muted, "note": "notification settings via API pending" });
            let _ = output::emit(&data, Format::Json);
            ExitCode::from(exit::OK)
        }
        NotifyCmd::Channel { channel, muted } => {
            let data = serde_json::json!({ "channel": channel, "muted": muted, "note": "notification settings via API pending" });
            let _ = output::emit(&data, Format::Json);
            ExitCode::from(exit::OK)
        }
    }
}
