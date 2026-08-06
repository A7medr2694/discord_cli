//! `dc tail <CHANNEL>` — gateway live message follow (invisible presence).
//!
//! Uses `discord-user-rs`'s `DiscordUser` (auto-reconnect, RESUME/fresh-ID by
//! close code, auto-fetch build number). `--once` fetches history and exits.

use std::process::ExitCode;

use discord_core::output::{self, exit};

use super::dc::DcCtx;

/// `dc tail <CHANNEL> [--once]` — stream new messages.
pub async fn dc_tail(ctx: &DcCtx, channel_id: &str, once: bool) -> ExitCode {
    let token = match discord_core::config::resolve_token(ctx.token.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            return ExitCode::from(output::emit_error("AuthError", &e.to_string(), exit::ERROR))
        }
    };

    let mut client = discord_user::DiscordUser::new(token)
        .with_status(discord_user::UserStatus::Invisible); // never empty (renders online)

    if let Err(e) = client.init().await {
        return ExitCode::from(output::emit_error("GatewayError", &e.to_string(), exit::ERROR));
    }

    let target = channel_id.to_string();
    let _sub = client
        .on_message_create(move |event| {
            // Only stream from the requested channel (or all if no filter).
            if !target.is_empty() && event.message.channel_id.to_string() != target {
                return;
            }
            // JSONL line: timestamp, author, content — agent-friendly.
            let author = event.message.author.username.clone();
            let content = event.message.content.clone();
            let ts = chrono::Utc::now().to_rfc3339();
            let line = serde_json::json!({
                "type": "message",
                "timestamp": ts,
                "channel_id": event.message.channel_id.to_string(),
                "author": author,
                "content": content,
            });
            println!("{}", serde_json::to_string(&line).unwrap_or_default());
        })
        .await;

    if once {
        // --once: brief listen then exit (history fetch is elsewhere).
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let _ = client.disconnect().await;
        return ExitCode::from(exit::OK);
    }

    // Stay alive until ctrl-c.
    tokio::signal::ctrl_c().await.ok();
    let _ = client.disconnect().await;
    ExitCode::from(exit::OK)
}
