//! `discord` CLI entry point.
//!
//! Global flags: `--token`, `--no-color`, `--json`, `--yaml`, `--format`.
//! Commands: `status`, `whoami` (M1.4); later `dc`, `search`, `serve`, `auth`.

use std::process::ExitCode;

mod resolve;

use clap::{CommandFactory, Parser, Subcommand};
use discord_core::client::ApiClient;
use discord_core::config::load_env;
use discord_core::output::{self, Format, exit};

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

fn run() -> ExitCode {
    load_env();
    let cli = Cli::parse();
    let format = output::resolve_format(cli.json, cli.yaml, cli.format.as_deref());

    // NO_COLOR honored.
    if cli.no_color || std::env::var("NO_COLOR").is_ok() {
        // Output stays plain; no color lib in core yet.
    }

    match cli.command {
        Some(Command::Status) => cmd_status(&cli, format),
        Some(Command::Whoami) => cmd_whoami(&cli, format),
        None => {
            // No subcommand: print help.
            let mut c = Cli::command();
            let _ = c.print_help();
            println!();
            ExitCode::from(exit::OK)
        }
    }
}

#[tokio::main]
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

#[tokio::main]
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
