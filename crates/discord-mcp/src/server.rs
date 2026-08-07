//! MCP server (rmcp stdio) exposing Discord tools to AI agents.
//!
//! Tools return **JSON** (not plaintext — fixing the langkurt gap).
//! Uses rmcp's `#[tool_router]` + `#[tool]` macros (official pattern).

use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};

use discord_core::client::ApiClient;

/// No-arg parameter type (empty schema).
#[derive(Serialize, Deserialize, JsonSchema, Default)]
pub struct EmptyParams {}

/// Guild ID parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GuildParams {
    /// The guild ID (snowflake).
    pub guild_id: String,
}

/// Channel read parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// The channel ID (snowflake).
    pub channel_id: String,
    /// Max messages to read (default 50).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Fetch messages before this snowflake.
    #[serde(default)]
    pub before: Option<String>,
}

/// Send message parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SendParams {
    /// The channel ID (snowflake).
    pub channel_id: String,
    /// Message content.
    pub content: String,
    /// Reply to this message ID.
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Local file paths to attach (server-side; max 10, each ≤10MiB).
    #[serde(default)]
    pub files: Option<Vec<String>>,
}

/// Create a thread.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ThreadCreateParams {
    /// The channel ID (snowflake).
    pub channel_id: String,
    /// Thread name.
    pub name: String,
    /// Create from this message ID (text/announcement parent).
    #[serde(default)]
    pub message_id: Option<String>,
    /// Starter message content (required for forum; optional standalone).
    #[serde(default)]
    pub text: Option<String>,
    /// Auto-archive minutes (60|1440|4320|10080).
    #[serde(default)]
    pub archive: Option<u32>,
}

/// Download archived attachments.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DownloadParams {
    /// Filter by channel name or ID (from archive).
    #[serde(default)]
    pub channel: Option<String>,
    /// Filter by guild name or ID (from archive).
    #[serde(default)]
    pub guild: Option<String>,
    /// Media type (image|gif|video|all).
    #[serde(default)]
    pub media_type: Option<String>,
    /// Max files (0 = unlimited).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Output directory (default <data_dir>/media).
    #[serde(default)]
    pub out_dir: Option<String>,
    /// Only files from messages on/after this date (30d|6m|1y|YYYY-MM-DD).
    #[serde(default)]
    pub since: Option<String>,
}

/// Parse a `since` value (YYYY-MM-DD or <n><d|m|y>) — mirrors the download
/// CLI's parse_since so MCP and CLI behave identically.
fn parse_mcp_since(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d
            .and_hms_opt(0, 0, 0)
            .map(|t| chrono::DateTime::from_naive_utc_and_offset(t, chrono::Utc));
    }
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = num.parse().ok()?;
    if n <= 0 {
        return None;
    }
    let now = chrono::Utc::now();
    match unit {
        "d" => Some(now - chrono::Duration::days(n)),
        "m" => Some(now - chrono::Duration::days(30 * n)),
        "y" => Some(now - chrono::Duration::days(365 * n)),
        _ => None,
    }
}

/// Set presence (persisted; applies to next tail/watch connect).
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PresenceParams {
    /// online | idle | dnd | invisible.
    pub status: String,
}

/// Join a server via invite.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct JoinParams {
    /// Invite code or full URL (discord.gg/..., discord.com/invite/...).
    pub invite_code: String,
    /// Must be true to actually join (advisory — client-side approval).
    pub confirm: bool,
}

/// Leave a server.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct LeaveParams {
    /// The guild ID (snowflake).
    pub guild_id: String,
    /// Must be true to actually leave (advisory — client-side approval).
    pub confirm: bool,
}

/// Get single message parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetMessageParams {
    /// The channel ID (snowflake).
    pub channel_id: String,
    /// The message ID (snowflake).
    pub message_id: String,
}

/// List members parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MembersParams {
    /// The guild ID (snowflake).
    pub guild_id: String,
    /// Max members (default 50).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// List threads parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ThreadsParams {
    /// The channel ID (snowflake).
    pub channel_id: String,
}

/// Search message parameter.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// The guild ID (snowflake).
    pub guild_id: String,
    /// Search query.
    pub query: String,
    /// Max results (default 25).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// The MCP server.
#[derive(Clone)]
pub struct DiscordMcpServer {
    tool_router: ToolRouter<Self>,
}

impl DiscordMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    fn client(&self) -> Result<ApiClient, String> {
        ApiClient::from_env(None).map_err(|e| e.to_string())
    }
}

impl Default for DiscordMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl DiscordMcpServer {
    /// List all Discord servers the user belongs to.
    #[tool(description = "List all Discord servers (guilds) the logged-in user belongs to.")]
    pub async fn list_guilds(&self, _params: Parameters<EmptyParams>) -> Result<String, String> {
        let mut c = self.client()?;
        let guilds = c.list_guilds().await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&guilds).unwrap_or_else(|_| "[]".into()))
    }

    /// List text channels of a guild.
    #[tool(description = "List text/announcement/forum channels of a guild.")]
    pub async fn list_channels(
        &self,
        Parameters(req): Parameters<GuildParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let channels = c
            .list_channels(&req.guild_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&channels).unwrap_or_else(|_| "[]".into()))
    }

    /// List DM and group-DM channels.
    #[tool(description = "List DM and group-DM channels of the user.")]
    pub async fn list_dms(&self, _params: Parameters<EmptyParams>) -> Result<String, String> {
        let mut c = self.client()?;
        let dms = c.list_dms().await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&dms).unwrap_or_else(|_| "[]".into()))
    }

    /// Read recent messages from a channel.
    #[tool(description = "Read recent messages from a channel (agent-friendly JSON).")]
    pub async fn read_messages(
        &self,
        Parameters(req): Parameters<ReadParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let limit = req.limit.unwrap_or(50) as usize;
        let before = req.before.as_deref().and_then(|s| s.parse().ok());
        let msgs = c
            .fetch_messages(&req.channel_id, limit, before, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&msgs).unwrap_or_else(|_| "[]".into()))
    }

    /// Send a message to a channel.
    ///
    /// If `files` is set, each path is read from the MCP server's local
    /// filesystem and attached (max 10 files, each ≤10MiB). Note: the
    /// `--confirm` gate is advisory here — approval is enforced by the MCP
    /// client, not the server.
    #[tool(
        description = "Send a message to a channel, optionally with local file attachments. Gate behind approval in client (advisory server-side)."
    )]
    pub async fn send_message(
        &self,
        Parameters(req): Parameters<SendParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let id = match req.files.as_deref() {
            None => c
                .send_message(&req.channel_id, &req.content, req.reply_to.as_deref())
                .await
                .map_err(|e| e.to_string())?,
            Some(paths) => {
                // Server-local attachment load (same caps as CLI: 10 files, 10MiB).
                let mut atts = Vec::with_capacity(paths.len());
                for path in paths {
                    let data = std::fs::read(path)
                        .map_err(|e| format!("cannot read file \"{path}\": {e}"))?;
                    if data.len() > 10 * 1024 * 1024 {
                        return Err(format!("file too large (>10MiB): {path}"));
                    }
                    let filename = std::path::Path::new(path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone());
                    let mime = mime_guess::from_path(path)
                        .first_raw()
                        .unwrap_or("application/octet-stream")
                        .to_string();
                    atts.push(discord_user::types::CreateAttachment {
                        filename,
                        data,
                        mime_type: mime,
                        description: None,
                    });
                }
                if atts.len() > 10 {
                    return Err("too many files: max 10 per message".into());
                }
                c.send_message_with_files(
                    &req.channel_id,
                    &req.content,
                    req.reply_to.as_deref(),
                    atts,
                )
                .await
                .map_err(|e| e.to_string())?
            }
        };
        Ok(format!(r#"{{"message_id":"{id}"}}"#))
    }

    /// Search messages in a guild.
    #[tool(description = "Native Discord search within a guild.")]
    pub async fn search_messages(
        &self,
        Parameters(req): Parameters<SearchParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let limit = req.limit.unwrap_or(25);
        let msgs = c
            .search_guild_messages(&req.guild_id, &req.query, None, limit)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&msgs).unwrap_or_else(|_| "[]".into()))
    }

    /// Read a DM channel's recent messages.
    #[tool(description = "Read recent messages from a DM channel (same as read_messages).")]
    pub async fn read_dm(&self, Parameters(req): Parameters<ReadParams>) -> Result<String, String> {
        let mut c = self.client()?;
        let limit = req.limit.unwrap_or(50) as usize;
        let before = req.before.as_deref().and_then(|s| s.parse().ok());
        let msgs = c
            .fetch_messages(&req.channel_id, limit, before, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&msgs).unwrap_or_else(|_| "[]".into()))
    }

    /// Download archived attachments summary (langkurt MCP mirror).
    ///
    /// Reports pending attachment count + sample filenames from the local
    /// archive; the actual fetch is done via the `discord download` CLI
    /// (the MCP server avoids a binary-only dependency).
    #[tool(
        description = "Report archived attachments pending download (sync first); fetch via the download CLI."
    )]
    pub async fn download_attachments(
        &self,
        Parameters(req): Parameters<DownloadParams>,
    ) -> Result<String, String> {
        let db_path = discord_core::config::db_path().map_err(|e| e.to_string())?;
        let conn = discord_db::db::open(db_path.to_str().unwrap_or("discord.db"))
            .map_err(|e| e.to_string())?;
        let mut filter = discord_db::attachments::AttachmentFilter {
            media_type: req
                .media_type
                .filter(|t| *t != "all")
                .map(|s| s.to_string()),
            limit: req.limit.unwrap_or(0),
            ..Default::default()
        };
        if let Some(s) = &req.since {
            let parsed = match parse_mcp_since(s) {
                Some(t) => t.to_rfc3339(),
                None => return Err(format!("invalid since \"{s}\" (YYYY-MM-DD or 30d/6m/1y)")),
            };
            filter.since = Some(parsed);
        }
        if let Some(c) = &req.channel {
            let id = discord_db::db::find_channel_id(&conn, c)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("channel \"{c}\" not found in archive (sync first)"))?;
            filter.channel_id = Some(id);
        }
        if let Some(g) = &req.guild {
            let id = discord_db::db::find_guild_id(&conn, g)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("guild \"{g}\" not found in archive"))?;
            filter.guild_id = Some(id);
        }
        let rows = discord_db::attachments::list_pending_attachments(&conn, &filter)
            .map_err(|e| e.to_string())?;
        let files: Vec<String> = rows.iter().take(20).map(|a| a.filename.clone()).collect();
        Ok(serde_json::json!({
            "pending": rows.len(),
            "sample_files": files,
            "note": "run `discord download` (CLI) to actually fetch files",
        })
        .to_string())
    }

    /// Create a thread (standalone, from message, or forum post).
    #[tool(
        description = "Create a thread: standalone, from a message, or a forum post (with starter text)."
    )]
    pub async fn create_thread(
        &self,
        Parameters(req): Parameters<ThreadCreateParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let result = match req.message_id.as_deref() {
            Some(mid) => c
                .create_thread_from_message(&req.channel_id, mid, &req.name, req.archive)
                .await
                .map_err(|e| e.to_string())?,
            None => c
                .create_thread(
                    &req.channel_id,
                    &req.name,
                    req.archive,
                    req.text.as_deref(),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?,
        };
        Ok(serde_json::json!({
            "type": if req.message_id.is_some() { "message_thread" }
                    else if result.channel_type == 15 || result.channel_type == 16 { "forum_post" }
                    else { "standalone_thread" },
            "id": result.id,
            "name": result.name,
            "channel_id": result.channel_id,
        })
        .to_string())
    }

    /// Set presence for future connections (persisted to config.json).
    ///
    /// The MCP server has no live gateway per invocation, so this persists
    /// the status — it takes effect on the next `tail`/`watch` connect.
    #[tool(
        description = "Set presence status (online|idle|dnd|invisible), persisted for future connections."
    )]
    pub async fn set_presence(
        &self,
        Parameters(req): Parameters<PresenceParams>,
    ) -> Result<String, String> {
        if !discord_core::config::set_configured_presence(&req.status) {
            return Err(format!(
                "invalid presence: {} (valid: online, idle, dnd, invisible)",
                req.status
            ));
        }
        Ok(serde_json::json!({ "presence": req.status, "saved": true }).to_string())
    }

    /// Join a server via invite code or URL.
    ///
    /// Previews the invite (guild name, member count) then accepts. The
    /// `confirm` flag is advisory — client-side approval is the real gate.
    #[tool(description = "Join a server via invite code or URL. confirm must be true (advisory).")]
    pub async fn join_guild(
        &self,
        Parameters(req): Parameters<JoinParams>,
    ) -> Result<String, String> {
        if !req.confirm {
            return Err("join_guild requires confirm: true".into());
        }
        let code = match ApiClient::extract_invite_code(&req.invite_code) {
            Some(c) => c.to_string(),
            None => return Err(format!("invalid invite: {}", req.invite_code)),
        };
        let mut c = self.client()?;
        // Preview first, then accept (satisfies the {guild_name,members} contract).
        let info = c.get_invite(&code).await.map_err(|e| e.to_string())?;
        let guild_name = info
            .guild
            .as_ref()
            .and_then(|g| g.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let members = info.approximate_member_count.unwrap_or(0);
        c.accept_invite(&code).await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "joined": true,
            "invite_code": code,
            "guild_name": guild_name,
            "approximate_member_count": members,
        })
        .to_string())
    }

    /// Leave a server.
    ///
    /// The `confirm` flag is advisory — client-side approval is the real gate.
    #[tool(description = "Leave a server by guild_id. confirm must be true (advisory).")]
    pub async fn leave_guild(
        &self,
        Parameters(req): Parameters<LeaveParams>,
    ) -> Result<String, String> {
        if !req.confirm {
            return Err("leave_guild requires confirm: true".into());
        }
        let mut c = self.client()?;
        c.leave_guild(&req.guild_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "left": true, "guild_id": req.guild_id }).to_string())
    }

    /// Get a single message by channel + message ID.
    #[tool(description = "Fetch a single message by channel_id and message_id.")]
    pub async fn get_message(
        &self,
        Parameters(req): Parameters<GetMessageParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let msg = c
            .get_message(&req.channel_id, &req.message_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&msg).unwrap_or_else(|_| "{}".into()))
    }

    /// List guild members.
    #[tool(description = "List members of a guild.")]
    pub async fn list_members(
        &self,
        Parameters(req): Parameters<MembersParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let members = c
            .list_members(&req.guild_id, req.limit.unwrap_or(50))
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&members).unwrap_or_else(|_| "[]".into()))
    }

    /// List threads in a channel (user-token fallback).
    #[tool(description = "List active threads in a channel (handles user-token fallback).")]
    pub async fn list_threads(
        &self,
        Parameters(req): Parameters<ThreadsParams>,
    ) -> Result<String, String> {
        let mut c = self.client()?;
        let threads = c
            .list_threads(&req.channel_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&threads).unwrap_or_else(|_| "[]".into()))
    }

    /// Get local archive sync status.
    #[tool(description = "Get per-channel sync status of the local SQLite archive.")]
    pub async fn get_sync_status(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<String, String> {
        // Best-effort: report whether a local DB exists.
        let db_path = discord_core::config::db_path().map_err(|e| e.to_string())?;
        let exists = db_path.exists();
        Ok(serde_json::json!({
            "db_path": db_path.to_string_lossy(),
            "db_exists": exists,
            "note": "run `discord dc sync-all` to populate the archive"
        })
        .to_string())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DiscordMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Discord CLI MCP server. Manage the logged-in Discord user account: \
             list servers/channels/DMs, read and send messages, search. \
             ToS: automating a user account may violate Discord ToS.",
        )
    }
}

/// Run the server over stdio (called by the `serve` subcommand).
pub async fn serve_stdio() -> anyhow::Result<()> {
    let server = DiscordMcpServer::new()
        .serve(rmcp::transport::stdio())
        .await?;
    server.waiting().await?;
    Ok(())
}
