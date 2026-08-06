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

/// Dispatch a `dc` subcommand.
pub async fn dispatch(ctx: &DcCtx, cmd: DcCmd) -> ExitCode {
    match cmd {
        DcCmd::Guilds => dc_guilds(ctx).await,
        DcCmd::Channels { guild } => dc_channels(ctx, &guild).await,
    }
}
