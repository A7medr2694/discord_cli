//! `discord` CLI entry point.
//!
//! Global flags: `--token`, `--no-color`, `--json`, `--yaml`, `--format`.
//! Commands: `status`, `whoami` (M1.4); later `dc`, `search`, `serve`, `auth`.

use std::process::ExitCode;

mod commands;
mod resolve;

use clap::{CommandFactory, Parser, Subcommand};
use discord_core::client::ApiClient;
use discord_core::config::load_env;
use discord_core::output::{self, exit, Format};

use commands::dc::{DcCtx, DmGroupCmd, NotifyCmd};

#[derive(Parser, Debug)]
#[command(
    name = "discord",
    version,
    about = "Discord CLI + MCP server for AI agents (user-token/selfbot style)",
    long_about = "Read/send/search Discord as the logged-in user, for AI agents.
WARNING: automating a user account may violate Discord ToS — use only on accounts you control."
)]
struct Cli {
    /// Discord token (overrides env/.env/keyring).
    #[arg(long, global = true)]
    token: Option<String>,

    /// Disable ANSI color (also honored via NO_COLOR).
    #[arg(long, global = true)]
    no_color: bool,

    /// Force JSON envelope output.
    #[arg(long, global = true)]
    json: bool,

    /// Force YAML envelope output.
    #[arg(long, global = true)]
    yaml: bool,

    /// Output format override: json|jsonl|yaml|rich|auto.
    #[arg(long, global = true, value_name = "FMT")]
    format: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate the configured token (exit 1 on failure).
    Status,
    /// Show the authenticated user's profile.
    Whoami,
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
        /// Channel name or ID.
        channel: String,
        /// Max messages to fetch (default 1000).
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
        /// Max members (default 50).
        #[arg(long, default_value_t = 50)]
        max: u32,
    },
    /// Show guild info (name, member counts).
    Info {
        /// Guild name or ID.
        guild: String,
    },
    /// Discord native search within a guild.
    GuildSearch {
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
        /// Message content. "-" reads from stdin.
        #[arg(long)]
        text: Option<String>,
        /// Attach a file (repeatable; max 10 per message).
        #[arg(long)]
        file: Vec<String>,
        /// Reply to a message id.
        #[arg(long)]
        reply: Option<String>,
        /// Send a typing indicator first (mimics a human composing).
        #[arg(long)]
        typing: bool,
        /// Confirm a non-reply send (never interactive).
        #[arg(long)]
        confirm: bool,
        /// Preview what would be sent without sending.
        #[arg(long)]
        dry_run: bool,
    },
    /// Send a typing indicator to a channel (one-shot).
    Typing {
        /// Channel name or ID.
        channel: String,
    },
    /// Join a server via invite code or URL (requires --confirm).
    Join {
        /// Invite code or full URL (discord.gg/..., discord.com/invite/...).
        invite: String,
        /// Confirm joining (never interactive).
        #[arg(long)]
        confirm: bool,
    },
    /// Leave a server (requires --confirm).
    Leave {
        /// Guild name or ID.
        guild: String,
        /// Confirm leaving (never interactive).
        #[arg(long)]
        confirm: bool,
    },
    /// Show or set presence (online|idle|dnd|invisible).
    Presence {
        /// New status. Omit to show the configured default.
        status: Option<String>,
    },
    /// Create a thread (standalone, from message, or forum post).
    ThreadCreate {
        /// Channel name or ID.
        channel: String,
        /// Thread name.
        #[arg(long)]
        name: String,
        /// Create from this message ID (text/announcement parent).
        #[arg(long)]
        message_id: Option<String>,
        /// Starter message content (required for forum; optional standalone).
        #[arg(long)]
        text: Option<String>,
        /// Auto-archive minutes (60|1440|4320|10080; default 1440).
        #[arg(long)]
        archive: Option<u32>,
        /// Comma-separated forum tag IDs.
        #[arg(long)]
        tags: Option<String>,
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
        /// Also emit typing-indicator events as JSONL.
        #[arg(long)]
        typing: bool,
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
    /// FTS5 search of the local SQLite archive.
    Search {
        /// Keyword to search.
        keyword: String,
        /// Filter by channel name.
        #[arg(short, long)]
        channel: Option<String>,
        /// Max results (default 50).
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },
    /// Recent stored messages.
    Recent {
        /// Filter by channel name.
        #[arg(short, long)]
        channel: Option<String>,
        /// Only messages from the last N hours.
        #[arg(long)]
        hours: Option<i64>,
        /// Max results (default 50).
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },
    /// Per-channel message counts.
    Stats,
    /// Top senders.
    Top {
        /// Filter by channel name.
        #[arg(short, long)]
        channel: Option<String>,
        /// Max senders (default 10).
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
    /// Export stored messages for a channel.
    Export {
        /// Channel ID.
        channel: String,
        /// Output as JSON (default text).
        #[arg(long)]
        json: bool,
        /// Output file path.
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Delete stored messages for a channel (requires -y).
    Purge {
        /// Channel ID.
        channel: String,
        /// Confirm purge.
        #[arg(short, long)]
        yes: bool,
    },
    /// Auth: auto-detect token, or paste manually, validate, save.
    Auth {
        /// Save the detected/pasted token to .env.
        #[arg(long)]
        save: bool,
        /// Paste the token manually instead of auto-detect.
        #[arg(long)]
        paste: bool,
    },
    /// Start the MCP server (stdio) for AI agents.
    Serve,
}

fn main() -> ExitCode {
    // Discord-user-rs's binary uses a roomy stack for clap's recursive debug
    // assertions on Windows (default 1 MiB overflows). Same trick.
    #[cfg(windows)]
    {
        let worker = std::thread::Builder::new()
            .name("discord-cli".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(run)
            .expect("spawn worker");
        worker.join().unwrap_or(ExitCode::from(1))
    }
    #[cfg(not(windows))]
    run()
}

#[tokio::main]
async fn run() -> ExitCode {
    load_env();
    let cli = Cli::parse();
    let format = output::resolve_format(cli.json, cli.yaml, cli.format.as_deref());

    // NO_COLOR honored.
    if cli.no_color || std::env::var("NO_COLOR").is_ok() {
        // Output stays plain; no color lib in core yet.
    }

    // Build a DcCtx for the discord operations (share token + format).
    let dcctx = DcCtx {
        token: cli.token.clone(),
        format,
    };
    let ctx = &dcctx;

    match cli.command {
        Some(Command::Status) => cmd_status(&cli, format).await,
        Some(Command::Whoami) => cmd_whoami(&cli, format).await,
        Some(Command::Guilds) => commands::dc::dc_guilds(ctx).await,
        Some(Command::Channels { guild }) => commands::dc::dc_channels(ctx, &guild).await,
        Some(Command::Dms) => commands::dc::dc_dms(ctx).await,
        Some(Command::History {
            channel,
            limit,
            before,
            after,
        }) => commands::dc::dc_history(ctx, &channel, limit, before, after).await,
        Some(Command::Read {
            channel,
            limit,
            before,
        }) => commands::dc::dc_read(ctx, &channel, limit, before).await,
        Some(Command::Members { guild, max }) => commands::dc::dc_members(ctx, &guild, max).await,
        Some(Command::Info { guild }) => commands::dc::dc_info(ctx, &guild).await,
        Some(Command::GuildSearch {
            guild,
            query,
            channel,
            limit,
        }) => commands::dc::dc_search(ctx, &guild, &query, channel.as_deref(), limit).await,
        Some(Command::Roles { guild }) => commands::dc::dc_roles(ctx, &guild).await,
        Some(Command::Profile { user_id }) => {
            commands::dc::dc_profile(ctx, user_id.as_deref()).await
        }
        Some(Command::Relationships) => commands::dc::dc_relationships(ctx).await,
        Some(Command::Threads { channel }) => commands::dc::dc_threads(ctx, &channel).await,
        Some(Command::Send {
            channel,
            text,
            file,
            reply,
            typing,
            confirm,
            dry_run,
        }) => {
            commands::dc::dc_send(
                ctx,
                &channel,
                commands::dc::SendOpts {
                    text: text.as_deref(),
                    files: &file,
                    reply: reply.as_deref(),
                    typing,
                    confirm,
                    dry_run,
                },
            )
            .await
        }
        Some(Command::Typing { channel }) => commands::dc::dc_typing(ctx, &channel).await,
        Some(Command::Join { invite, confirm }) => {
            commands::dc::dc_join(ctx, &invite, confirm).await
        }
        Some(Command::Leave { guild, confirm }) => {
            commands::dc::dc_leave(ctx, &guild, confirm).await
        }
        Some(Command::Presence { status }) => {
            commands::dc::dc_presence(ctx, status.as_deref()).await
        }
        Some(Command::ThreadCreate {
            channel,
            name,
            message_id,
            text,
            archive,
            tags,
        }) => {
            commands::dc::dc_thread_create(
                ctx,
                &channel,
                &name,
                message_id.as_deref(),
                text.as_deref(),
                archive,
                tags.as_deref(),
            )
            .await
        }
        Some(Command::Edit {
            channel,
            message_id,
            text,
        }) => commands::dc::dc_edit(ctx, &channel, &message_id, &text).await,
        Some(Command::Delete {
            channel,
            message_id,
            confirm,
        }) => commands::dc::dc_delete(ctx, &channel, &message_id, confirm).await,
        Some(Command::React {
            channel,
            message_id,
            emoji,
        }) => commands::dc::dc_react(ctx, &channel, &message_id, &emoji).await,
        Some(Command::Unreact {
            channel,
            message_id,
            emoji,
        }) => commands::dc::dc_unreact(ctx, &channel, &message_id, &emoji).await,
        Some(Command::Pin {
            channel,
            message_id,
        }) => commands::dc::dc_pin(ctx, &channel, &message_id).await,
        Some(Command::Pins { channel }) => commands::dc::dc_pins(ctx, &channel).await,
        Some(Command::Sync { channel, limit }) => commands::dc::dc_sync(ctx, &channel, limit).await,
        Some(Command::SyncAll { limit }) => commands::dc::dc_sync_all(ctx, limit).await,
        Some(Command::Tail { channel, once }) => commands::tail::dc_tail(ctx, &channel, once).await,
        Some(Command::Watch {
            channel,
            keyword,
            typing,
        }) => commands::tail::dc_watch(ctx, channel.as_deref(), keyword.as_deref(), typing).await,
        Some(Command::DmGroup { cmd }) => commands::dc::dc_dm_group(ctx, cmd).await,
        Some(Command::Notify { cmd }) => commands::dc::dc_notify(ctx, cmd).await,
        Some(Command::Search {
            keyword,
            channel,
            limit,
        }) => commands::local::cmd_search(&keyword, channel.as_deref(), limit, format),
        Some(Command::Recent {
            channel,
            hours,
            limit,
        }) => commands::local::cmd_recent(channel.as_deref(), hours, limit, format),
        Some(Command::Stats) => commands::local::cmd_stats(format),
        Some(Command::Top { channel, limit }) => {
            commands::local::cmd_top(channel.as_deref(), limit, format)
        }
        Some(Command::Export {
            channel,
            json,
            output,
        }) => commands::local::cmd_export(&channel, json, output.as_deref(), format),
        Some(Command::Purge { channel, yes }) => commands::local::cmd_purge(&channel, yes, format),
        Some(Command::Auth { save, paste }) => cmd_auth(save, paste, format).await,
        Some(Command::Serve) => cmd_serve().await,
        None => {
            // No subcommand: print help.
            let mut c = Cli::command();
            let _ = c.print_help();
            println!();
            ExitCode::from(exit::OK)
        }
    }
}

async fn cmd_status(cli: &Cli, format: Format) -> ExitCode {
    let mut client = match ApiClient::from_env(cli.token.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            return ExitCode::from(output::emit_error("AuthError", &e.to_string(), exit::ERROR))
        }
    };

    match client.validate().await {
        Ok(true) => {
            let data = serde_json::json!({ "authenticated": true });
            let _ = output::emit(&data, format);
            ExitCode::from(exit::OK)
        }
        _ => {
            let data = serde_json::json!({ "authenticated": false });
            let _ = output::emit(&data, format);
            ExitCode::from(exit::ERROR)
        }
    }
}

/// `auth [--save] [--paste]` — auto-detect or paste token, validate, save.
async fn cmd_auth(save: bool, paste: bool, format: Format) -> ExitCode {
    // Paste flow.
    if paste {
        match discord_auth::auth::auth_paste(save).await {
            Ok(_token) => {
                let _ = output::emit(
                    &serde_json::json!({ "authenticated": true, "token_saved": save, "token": "***" }),
                    format,
                );
                return ExitCode::from(exit::OK);
            }
            Err(e) => {
                return ExitCode::from(output::emit_error("AuthError", &e.to_string(), exit::ERROR))
            }
        }
    }
    // Auto-detect flow.
    let tokens = discord_auth::auth::find_tokens();
    if tokens.is_empty() {
        return ExitCode::from(output::emit_error(
            "NoTokenFound",
            "no token found in local Discord/browser. Use --paste to enter manually.",
            exit::ERROR,
        ));
    }
    // Validate each candidate, pick first valid.
    for (source, token) in &tokens {
        if let Ok(true) = discord_auth::auth::validate_token(token).await {
            if save {
                let _ = discord_auth::auth::save_token_to_env(token, None);
            }
            let _ = output::emit(
                &serde_json::json!({ "authenticated": true, "source": source, "token_saved": save }),
                format,
            );
            return ExitCode::from(exit::OK);
        }
    }
    ExitCode::from(output::emit_error(
        "InvalidTokens",
        "found token(s) but none validated against Discord",
        exit::ERROR,
    ))
}

/// `serve` — start the MCP server over stdio.
async fn cmd_serve() -> ExitCode {
    match discord_mcp::server::serve_stdio().await {
        Ok(_) => ExitCode::from(exit::OK),
        Err(e) => ExitCode::from(output::emit_error("McpError", &e.to_string(), exit::ERROR)),
    }
}

async fn cmd_whoami(cli: &Cli, format: Format) -> ExitCode {
    let mut client = match ApiClient::from_env(cli.token.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            return ExitCode::from(output::emit_error("AuthError", &e.to_string(), exit::ERROR))
        }
    };

    match client.get_me().await {
        Ok(me) => {
            let _ = output::emit(&me, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error("NotFound", &e.to_string(), exit::ERROR)),
    }
}
