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
    }
}
