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
