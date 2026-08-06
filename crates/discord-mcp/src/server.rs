//! MCP server (rmcp stdio) exposing Discord tools to AI agents.
//!
//! Tools return **JSON** (not plaintext — fixing the langkurt gap).
//! Uses rmcp's `#[tool_router]` + `#[tool]` macros (official pattern).

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
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
    pub async fn list_channels(&self, Parameters(req): Parameters<GuildParams>) -> Result<String, String> {
        let mut c = self.client()?;
        let channels = c.list_channels(&req.guild_id).await.map_err(|e| e.to_string())?;
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
    pub async fn read_messages(&self, Parameters(req): Parameters<ReadParams>) -> Result<String, String> {
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
    #[tool(description = "Send a message to a channel. Gate behind approval in client.")]
    pub async fn send_message(&self, Parameters(req): Parameters<SendParams>) -> Result<String, String> {
        let mut c = self.client()?;
        let id = c
            .send_message(&req.channel_id, &req.content, req.reply_to.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(r#"{{"message_id":"{id}"}}"#))
    }

    /// Search messages in a guild.
    #[tool(description = "Native Discord search within a guild.")]
    pub async fn search_messages(&self, Parameters(req): Parameters<SearchParams>) -> Result<String, String> {
        let mut c = self.client()?;
        let limit = req.limit.unwrap_or(25);
        let msgs = c
            .search_guild_messages(&req.guild_id, &req.query, None, limit)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&msgs).unwrap_or_else(|_| "[]".into()))
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
    let server = DiscordMcpServer::new().serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
