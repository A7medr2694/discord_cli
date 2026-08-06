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
use discord_core::output::{self, Format, exit};

use commands::dc::{DcCmd, DcCtx};

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
    /// Discord operations — list guilds/channels, read messages, etc.
    Dc {
        #[command(subcommand)]
        cmd: DcCmd,
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
        return worker.join().unwrap_or(ExitCode::from(1));
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

    match cli.command {
        Some(Command::Status) => cmd_status(&cli, format).await,
        Some(Command::Whoami) => cmd_whoami(&cli, format).await,
        Some(Command::Dc { cmd }) => {
            let ctx = DcCtx {
                token: cli.token.clone(),
                format,
            };
            commands::dc::dispatch(&ctx, cmd).await
        }
        Some(Command::Search { keyword, channel, limit }) => {
            commands::local::cmd_search(&keyword, channel.as_deref(), limit, format)
        }
        Some(Command::Recent { channel, hours, limit }) => {
            commands::local::cmd_recent(channel.as_deref(), hours, limit, format)
        }
        Some(Command::Stats) => commands::local::cmd_stats(format),
        Some(Command::Top { channel, limit }) => {
            commands::local::cmd_top(channel.as_deref(), limit, format)
        }
        Some(Command::Export { channel, json, output }) => {
            commands::local::cmd_export(&channel, json, output.as_deref(), format)
        }
        Some(Command::Purge { channel, yes }) => {
            commands::local::cmd_purge(&channel, yes, format)
        }
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
            return ExitCode::from(output::emit_error(
                "AuthError",
                &e.to_string(),
                exit::ERROR,
            ))
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
            Ok(token) => {
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
            return ExitCode::from(output::emit_error(
                "AuthError",
                &e.to_string(),
                exit::ERROR,
            ))
        }
    };

    match client.get_me().await {
        Ok(me) => {
            let _ = output::emit(&me, format);
            ExitCode::from(exit::OK)
        }
        Err(e) => ExitCode::from(output::emit_error(
            "NotFound",
            &e.to_string(),
            exit::ERROR,
        )),
    }
}
