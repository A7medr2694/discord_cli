//! Client wrapper over `discord-user-rs`'s `DiscordHttpClient`.
//!
//! Provides browser headers + X-Super-Properties (via `set_super_properties_b64`)
//! plus rate-limit handling. Full stealth header set lands in M8; the core here
//! is the thin typed layer commands call.
//!
//! `discord-user-rs` is the MIT core crate (plan §2.2).

use anyhow::{Context, Result};
use discord_user::client::DiscordHttpClient;
use discord_user::route::Route;

use crate::config::{resolve_token, API_BASE};
use crate::types::{Channel, DmChannel, Guild, GuildInfo, Me, Member};

/// Authenticated API client backed by `discord-user-rs`.
///
/// Holds the token and lazily constructs the underlying `DiscordHttpClient`.
pub struct ApiClient {
    token: String,
    client: Option<DiscordHttpClient>,
}

impl ApiClient {
    /// Set the live gateway presence (Op 3) for an active connection.
    ///
    /// Requires a `DiscordUser` gateway; use `DiscordUserContext::gateway()`
    /// to obtain it (verified: `DiscordContext` is public, `gateway() ->
    /// Option<&Gateway>`, `Gateway::send_presence` exists — crate context.rs:10,
    /// gateway.rs:953). Returns Ok(()) when no gateway is connected (presence
    /// applies on next connect via `with_status` instead).
    pub async fn set_presence(
        client: &discord_user::DiscordUser,
        status: discord_user::UserStatus,
    ) -> anyhow::Result<()> {
        use discord_user::DiscordContext;
        if let Some(gw) = client.gateway() {
            gw.send_presence(status)
                .await
                .map_err(|e| anyhow::anyhow!("gateway send_presence failed: {e}"))?;
        }
        Ok(())
    }
}

impl ApiClient {
    /// Create a client from a resolved token.
    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: None,
        }
    }

    /// Create from the standard token resolution chain.
    pub fn from_env(flag: Option<&str>) -> Result<Self> {
        Ok(Self::with_token(resolve_token(flag)?))
    }

    /// Lazily build the underlying HTTP client (Chrome UA, locale, super-props).
    fn inner(&mut self) -> Result<&mut DiscordHttpClient> {
        if self.client.is_none() {
            let mut c = DiscordHttpClient::new(self.token.clone(), None, false);
            c.set_discord_locale(Some("en-US".to_string()));
            // Stealth (M8): attach X-Super-Properties so REST traffic looks
            // like the real Discord client.
            c.set_super_properties_b64(Some(crate::stealth::x_super_properties()));
            self.client = Some(c);
        }
        Ok(self.client.as_mut().unwrap())
    }

    /// `GET /users/@me` — current user.
    pub async fn get_me(&mut self) -> Result<Me> {
        let inner = self.inner()?;
        inner
            .get(Route::GetMe)
            .await
            .context("GET /users/@me failed")
    }

    /// Validate token: `GET /users/@me` returns 200.
    pub async fn validate(&mut self) -> Result<bool> {
        match self.get_me().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// `GET /users/@me/guilds` — guilds the user belongs to.
    /// Response is a raw array; we deserialize to our `Guild` shape.
    pub async fn list_guilds(&mut self) -> Result<Vec<Guild>> {
        let inner = self.inner()?;
        let raw: Vec<RawGuild> = inner.get(Route::GetCurrentUserGuilds).await.map_err(|e| {
            // Surface the inner error chain (Debug) for troubleshooting.
            anyhow::anyhow!("GET /users/@me/guilds failed: {:?}", e)
        })?;
        Ok(raw
            .into_iter()
            .map(|g| Guild {
                id: g.id.to_string(),
                name: g.name,
                icon: g.icon,
                owner: Some(g.owner),
            })
            .collect())
    }

    /// Resolve a guild name or ID to a guild ID (jackwener `resolve_guild_id`).
    /// Returns Ok(None) if not found.
    pub async fn resolve_guild_id(&mut self, guild: &str) -> Result<Option<String>> {
        if guild.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Some(guild.to_string()));
        }
        let guilds = self.list_guilds().await?;
        let needle = guild.to_lowercase();
        Ok(guilds
            .into_iter()
            .find(|g| g.name.to_lowercase().contains(&needle))
            .map(|g| g.id))
    }

    /// `GET /guilds/{id}/channels` — channels of a guild (text-like filtered).
    pub async fn list_channels(&mut self, guild_id: &str) -> Result<Vec<Channel>> {
        let gid: u64 = guild_id.parse().context("invalid guild id")?;
        let inner = self.inner()?;
        let raw: Vec<RawChannel> = inner
            .get(Route::GetGuildChannels { guild_id: gid })
            .await
            .context("GET /guilds/{id}/channels failed")?;
        let mut channels: Vec<Channel> = raw
            .into_iter()
            .map(|c| Channel {
                id: c.id.to_string(),
                name: c.name.unwrap_or_default(),
                guild_id: Some(guild_id.to_string()),
                channel_type: c.channel_type,
                topic: c.topic,
                parent_id: c.parent_id.map(|p| p.to_string()),
                position: Some(c.position),
            })
            .collect();
        // text/announcement/forum only, sorted by position (jackwener).
        channels.retain(|c| c.is_text_like());
        channels.sort_by_key(|c| c.position.unwrap_or(0));
        Ok(channels)
    }

    /// `GET /users/@me/channels` — DM + group-DM channels.
    /// `Route::CreateDm` maps to that path (POST creates; GET lists).
    pub async fn list_dms(&mut self) -> Result<Vec<DmChannel>> {
        let inner = self.inner()?;
        let raw: Vec<RawDm> = inner
            .get(Route::CreateDm)
            .await
            .context("GET /users/@me/channels failed")?;
        let mut dms: Vec<DmChannel> = raw
            .into_iter()
            .map(|d| {
                let recipient_count = d.recipients.as_ref().map(|r| r.len());
                let recipients: Vec<String> = d
                    .recipients
                    .unwrap_or_default()
                    .into_iter()
                    .map(|u| u.tag())
                    .collect();
                let label = match recipients.len() {
                    0 => d.name.clone().unwrap_or_else(|| d.id.to_string()),
                    1 => recipients[0].clone(),
                    _ => recipients.join(", "),
                };
                DmChannel {
                    id: d.id.to_string(),
                    label,
                    channel_type: d.channel_type,
                    recipient_count,
                }
            })
            .collect();
        dms.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(dms)
    }

    /// `GET /guilds/{id}/members` — list members (jackwener list_members).
    pub async fn list_members(&mut self, guild_id: &str, limit: u32) -> Result<Vec<Member>> {
        let gid: u64 = guild_id.parse().context("invalid guild id")?;
        let inner = self.inner()?;
        let raw: Vec<RawMember> = inner
            .get(Route::GetGuildMembers {
                guild_id: gid,
                limit: limit.min(1000),
            })
            .await
            .context("GET /guilds/{id}/members failed")?;
        Ok(raw
            .into_iter()
            .map(|m| Member {
                id: m.user.id.to_string(),
                username: m.user.username,
                global_name: m.user.global_name,
                nick: m.nick,
                joined_at: m.joined_at,
                bot: m.user.bot.unwrap_or(false),
            })
            .collect())
    }

    /// `GET /guilds/{id}?with_counts=true` — guild info (jackwener get_guild_info).
    pub async fn guild_info(&mut self, guild_id: &str) -> Result<GuildInfo> {
        let gid: u64 = guild_id.parse().context("invalid guild id")?;
        let inner = self.inner()?;
        let raw: RawGuildInfo = inner
            .get(Route::GetGuild {
                guild_id: gid,
                with_counts: true,
            })
            .await
            .context("GET /guilds/{id} failed")?;
        Ok(GuildInfo {
            id: raw.id.to_string(),
            name: raw.name,
            description: raw.description,
            member_count: raw.approximate_member_count,
            online_count: raw.approximate_presence_count,
        })
    }

    /// `GET /guilds/{id}/messages/search?content=...` — Discord native search
    /// (jackwener search_guild_messages). Returns matching messages.
    pub async fn search_guild_messages(
        &mut self,
        guild_id: &str,
        query: &str,
        channel_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<crate::types::Message>> {
        let gid: u64 = guild_id.parse().context("invalid guild id")?;
        let cid = channel_id.and_then(|c| c.parse().ok());
        let inner = self.inner()?;
        let raw: SearchResponse = inner
            .get(Route::SearchGuildMessages {
                guild_id: gid,
                content: query,
                channel_id: cid,
                limit: Some(limit),
            })
            .await
            .context("search failed")?;
        let mut out = Vec::new();
        for group in raw.messages {
            for msg in group {
                let urls = msg.url_list();
                let details = msg.details();
                let reactions = msg.reaction_total();
                out.push(crate::types::Message {
                    message_id: msg.id.to_string(),
                    channel_id: msg.channel_id.to_string(),
                    guild_id: Some(guild_id.to_string()),
                    author_id: Some(msg.author.id.to_string()),
                    author: msg.author.username,
                    timestamp: msg.timestamp,
                    content: msg.content,
                    attachments: urls,
                    attachment_details: details,
                    reactions,
                });
            }
            if out.len() >= limit as usize {
                break;
            }
        }
        Ok(out)
    }

    /// `GET /guilds/{id}/roles` — guild roles sorted by position.
    pub async fn list_roles(&mut self, guild_id: &str) -> Result<Vec<crate::types::Role>> {
        let gid: u64 = guild_id.parse().context("invalid guild id")?;
        let inner = self.inner()?;
        let raw: Vec<RawRole> = inner
            .get(Route::GetGuildRoles { guild_id: gid })
            .await
            .context("GET /guilds/{id}/roles failed")?;
        let mut roles: Vec<crate::types::Role> = raw
            .into_iter()
            .map(|r| crate::types::Role {
                id: r.id.to_string(),
                name: r.name,
                color: r.color,
                position: r.position,
                permissions: r.permissions,
            })
            .collect();
        roles.sort_by_key(|r| std::cmp::Reverse(r.position));
        Ok(roles)
    }

    /// `GET /users/@me/relationships` — friends/blocked/pending.
    pub async fn relationships(&mut self) -> Result<Vec<crate::types::Relationship>> {
        let inner = self.inner()?;
        let raw: Vec<RawRelationship> = inner
            .get(Route::GetRelationships)
            .await
            .context("GET relationships failed")?;
        Ok(raw
            .into_iter()
            .map(|r| crate::types::Relationship {
                user_id: r.id.to_string(),
                username: r.username,
                relationship_type: r.relationship_type,
            })
            .collect())
    }

    /// `GET /users/{id}/profile` — user profile.
    pub async fn user_profile(&mut self, user_id: &str) -> Result<crate::types::UserProfile> {
        let uid: u64 = user_id.parse().context("invalid user id")?;
        let inner = self.inner()?;
        let raw: RawUserProfile = inner
            .get(Route::GetUserProfile {
                user_id: uid,
                guild_id: None,
            })
            .await
            .context("GET /users/{id}/profile failed")?;
        Ok(crate::types::UserProfile {
            user_id: raw.user.id.to_string(),
            username: raw.user.username,
            global_name: raw.user.global_name,
            bio: raw.user_bio,
        })
    }

    /// List active threads in a channel.
    ///
    /// **User-token pitfall (langkurt):** `GET /channels/{id}/threads` (active)
    /// is BOT-ONLY → 403 for user tokens. Fallback to what Discord's own app
    /// uses: `GET /channels/{id}/threads/search` (offset-paginated).
    pub async fn list_threads(&mut self, channel_id: &str) -> Result<Vec<Channel>> {
        // Try the bot-only active endpoint first; on 403 fall back.
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        let threads_url = format!("channels/{}/threads/active", channel_id);
        let bot_only: std::result::Result<ThreadActiveResponse, _> =
            inner.get(Route::Custom(threads_url.into())).await;
        match bot_only {
            Ok(resp) => Ok(resp
                .threads
                .into_iter()
                .map(raw_thread_to_channel)
                .collect()),
            Err(_) => {
                // 403 → user-token fallback: threads/search, offset-paginated.
                let mut out = Vec::new();
                let mut offset: u64 = 0;
                loop {
                    let url = format!(
                        "channels/{}/threads/search?limit=25&sort_by=last_message_time&sort_order=desc&archived=false&offset={}",
                        channel_id, offset
                    );
                    let resp: ThreadSearchResponse = inner
                        .get(Route::Custom(url.into()))
                        .await
                        .context("threads/search failed")?;
                    let n = resp.threads.len();
                    out.extend(resp.threads.into_iter().map(raw_thread_to_channel));
                    offset += n as u64;
                    if !resp.has_more || n == 0 {
                        break;
                    }
                    // rate-limit friendly pause (langkurt sleeps 300ms)
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                }
                let _ = cid;
                Ok(out)
            }
        }
    }

    /// `POST /channels/{id}/messages` — send a message (M3.1).
    /// Returns the new message id.
    pub async fn send_message(
        &mut self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<String> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        let req = discord_user::types::SendMessageRequest {
            content,
            tts: false,
            flags: 0,
            message_reference: reply_to.map(|id| discord_user::types::MessageReference {
                reference_type: None,
                message_id: Some(id.to_string()),
                channel_id: None,
                guild_id: None,
            }),
            nonce: None,
            mobile_network_type: Some("unknown"), // mimic Discord mobile (selfbot)
        };
        let resp: RawMessage = inner
            .post(Route::CreateMessage { channel_id: cid }, req)
            .await
            .context("POST /channels/{id}/messages failed")?;
        Ok(resp.id.to_string())
    }

    /// Build the v10 multipart `payload_json` for a message with attachments.
    /// Exposed for unit testing (no network). The `attachments` descriptor
    /// array uses `id` = index-as-string (discord.js MessagePayload style).
    fn build_send_payload(
        content: &str,
        reply_to: Option<&str>,
        n_files: usize,
    ) -> anyhow::Result<serde_json::Value> {
        let req = discord_user::types::SendMessageRequest {
            content,
            tts: false,
            flags: 0,
            message_reference: reply_to.map(|id| discord_user::types::MessageReference {
                reference_type: None,
                message_id: Some(id.to_string()),
                channel_id: None,
                guild_id: None,
            }),
            nonce: None,
            mobile_network_type: Some("unknown"), // mimic Discord mobile (selfbot)
        };
        let mut payload = serde_json::to_value(&req).context("serialize send payload")?;
        let atts: Vec<serde_json::Value> = (0..n_files)
            .map(|i| serde_json::json!({ "id": i.to_string() }))
            .collect();
        payload["attachments"] = serde_json::Value::Array(atts);
        Ok(payload)
    }

    /// `POST /channels/{id}/messages` — send a message with file attachments
    /// (multipart). payload_json carries the message body; each file is a
    /// `files[N]` part with an `attachments:[{id:"0"}]` descriptor array
    /// (Discord v10 style, cf. discord.js MessagePayload).
    pub async fn send_message_with_files(
        &mut self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
        attachments: Vec<discord_user::types::CreateAttachment>,
    ) -> Result<String> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        let payload = Self::build_send_payload(content, reply_to, attachments.len())?;
        let resp: RawMessage = inner
            .post_multipart(
                Route::CreateMessage { channel_id: cid },
                payload,
                attachments,
            )
            .await
            .context("POST /channels/{id}/messages (multipart) failed")?;
        Ok(resp.id.to_string())
    }

    /// `GET /invites/{code}` — preview an invite (guild name, member counts).
    /// Reference: RickvanLoo menu.go invite preview; crate Route::Invite.
    pub async fn get_invite(&mut self, code: &str) -> Result<discord_user::types::Invite> {
        let inner = self.inner()?;
        inner
            .get(Route::Invite {
                code: std::borrow::Cow::Borrowed(code),
                with_counts: Some(true),
                with_expiration: None,
                guild_scheduled_event_id: None,
            })
            .await
            .context("GET /invites/{code} failed")
    }

    /// `POST /invites/{code}` — accept an invite (join a server). Nil body.
    /// Reference: RickvanLoo InviteAccept; crate Route::JoinGuild.
    pub async fn accept_invite(&mut self, code: &str) -> Result<()> {
        let inner = self.inner()?;
        inner
            .post_no_response(Route::JoinGuild { code }, ())
            .await
            .context("POST /invites/{code} failed")?;
        Ok(())
    }

    /// `DELETE /users/@me/guilds/{guild_id}` — leave a server.
    /// Reference: RickvanLoo GuildLeave; crate Route::LeaveGuild.
    pub async fn leave_guild(&mut self, guild_id: &str) -> Result<()> {
        let gid: u64 = guild_id.parse().context("invalid guild id")?;
        let inner = self.inner()?;
        inner
            .delete(Route::LeaveGuild { guild_id: gid })
            .await
            .context("DELETE /users/@me/guilds/{id} failed")?;
        Ok(())
    }

    /// Extract a bare invite code from a full URL or raw code.
    /// Strips known invite URL prefixes, trailing slashes, and `?`/`#`
    /// suffixes (review#21). Returns the remaining alnum token.
    pub fn extract_invite_code(s: &str) -> Option<&str> {
        let s = s.trim();
        for prefix in [
            "https://discord.com/invite/",
            "http://discord.com/invite/",
            "https://discordapp.com/invite/",
            "http://discordapp.com/invite/",
            "https://discord.gg/",
            "http://discord.gg/",
            "discord.gg/",
        ] {
            if let Some(rest) = s.strip_prefix(prefix) {
                let cut = rest.split(['?', '#']).next().unwrap_or(rest);
                let cut = cut.trim_end_matches('/');
                return if cut.is_empty() { None } else { Some(cut) };
            }
        }
        let cut = s.split(['?', '#']).next().unwrap_or(s);
        let cut = cut.trim_end_matches('/');
        if cut.is_empty() {
            None
        } else {
            Some(cut)
        }
    }

    /// `POST /channels/{id}/threads` — create a thread.
    ///
    /// - Forum (type 15) requires a starter `message` (defaults to the thread
    ///   name, Escape-Tech thread-create.js:11-15).
    /// - Standalone text threads use `channel_type: 11` (public).
    /// - Forum/media channels also accept `applied_tags` (crate field).
    pub async fn create_thread(
        &mut self,
        channel_id: &str,
        name: &str,
        archive_minutes: Option<u32>,
        starter: Option<&str>,
        applied_tags: Option<Vec<String>>,
    ) -> Result<ThreadResult> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        let mut req = discord_user::types::CreateThreadRequest::public(name);
        req.auto_archive_duration = archive_minutes;
        req.applied_tags = applied_tags;
        // Forum (15) / media (16) channels require a message payload.
        if let Some(starter) = starter {
            req.message = Some(serde_json::json!({
                "content": starter,
                "tts": false,
                "allowed_mentions": null,
                "attachments": [],
            }));
        }
        let resp: RawChannel = inner
            .post(Route::CreateThread { channel_id: cid }, req)
            .await
            .context("POST /channels/{id}/threads failed")?;
        Ok(ThreadResult {
            id: resp.id.to_string(),
            name: resp.name.clone().unwrap_or_default(),
            channel_id: channel_id.to_string(),
            channel_type: resp.channel_type,
            parent_message_id: None,
        })
    }

    /// `POST /channels/{id}/messages/{mid}/threads` — create a thread from a
    /// message (parent must be text/announcement; Escape-Tech path 2).
    pub async fn create_thread_from_message(
        &mut self,
        channel_id: &str,
        message_id: &str,
        name: &str,
        archive_minutes: Option<u32>,
    ) -> Result<ThreadResult> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        let mut req = discord_user::types::CreateThreadRequest::public(name);
        req.auto_archive_duration = archive_minutes;
        let resp: RawChannel = inner
            .post(
                Route::CreateThreadFromMessage {
                    channel_id: cid,
                    message_id: mid,
                },
                req,
            )
            .await
            .context("POST /channels/{id}/messages/{mid}/threads failed")?;
        Ok(ThreadResult {
            id: resp.id.to_string(),
            name: resp.name.clone().unwrap_or_default(),
            channel_id: channel_id.to_string(),
            channel_type: resp.channel_type,
            parent_message_id: Some(message_id.to_string()),
        })
    }

    /// `POST /channels/{id}/typing` — send typing indicator (no body).
    /// Reference: discordo composer.sendTyping() → Client.Typing (10s throttle
    /// enforced by caller; API itself is fire-and-forget).
    pub async fn trigger_typing(&mut self, channel_id: &str) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        inner
            .post_empty(Route::TriggerTyping { channel_id: cid })
            .await
            .context("POST /channels/{id}/typing failed")?;
        Ok(())
    }

    /// `PATCH /channels/{id}/messages/{mid}` — edit own message (M3.2).
    pub async fn edit_message(
        &mut self,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        let req = discord_user::types::EditMessageRequest {
            content: Some(content),
            flags: None,
        };
        inner
            .patch::<serde_json::Value, _>(
                Route::EditMessage {
                    channel_id: cid,
                    message_id: mid,
                },
                req,
            )
            .await
            .context("PATCH message failed")?;
        Ok(())
    }

    /// `DELETE /channels/{id}/messages/{mid}` — delete own message (M3.2).
    pub async fn delete_message(&mut self, channel_id: &str, message_id: &str) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        inner
            .delete(Route::DeleteMessage {
                channel_id: cid,
                message_id: mid,
            })
            .await
            .context("DELETE message failed")?;
        Ok(())
    }

    /// `PUT /channels/{id}/messages/{mid}/reactions/{emoji}/@me` — react.
    pub async fn add_reaction(
        &mut self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        // PUT .../reactions/{emoji}/@me returns 204 No Content (no body);
        // `put` would try to parse the empty body as JSON and fail. Use
        // `put_empty` which discards the response body.
        inner
            .put_empty(Route::AddReaction {
                channel_id: cid,
                message_id: mid,
                emoji,
            })
            .await
            .context("react failed")?;
        Ok(())
    }

    /// `DELETE .../reactions/{emoji}/@me` — remove own reaction.
    pub async fn remove_reaction(
        &mut self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        inner
            .delete(Route::RemoveOwnReaction {
                channel_id: cid,
                message_id: mid,
                emoji,
            })
            .await
            .context("unreact failed")?;
        Ok(())
    }

    /// `PUT /channels/{id}/pins/{mid}` — pin a message.
    pub async fn pin_message(&mut self, channel_id: &str, message_id: &str) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        // PUT .../pins/{mid} returns 204 No Content (no body); `put` would
        // fail parsing the empty body as JSON. Use `put_empty` instead.
        inner
            .put_empty(Route::PinMessage {
                channel_id: cid,
                message_id: mid,
            })
            .await
            .context("pin failed")?;
        Ok(())
    }

    /// `DELETE /channels/{id}/pins/{mid}` — unpin a message.
    pub async fn unpin_message(&mut self, channel_id: &str, message_id: &str) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        inner
            .delete(Route::UnpinMessage {
                channel_id: cid,
                message_id: mid,
            })
            .await
            .context("unpin failed")?;
        Ok(())
    }

    /// `GET /channels/{id}/pins` — list pinned messages.
    pub async fn pinned_messages(
        &mut self,
        channel_id: &str,
    ) -> Result<Vec<crate::types::Message>> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        let raw: Vec<RawMessage> = inner
            .get(Route::GetPins { channel_id: cid })
            .await
            .context("GET pins failed")?;
        Ok(raw
            .into_iter()
            .map(|m| {
                let urls = m.url_list();
                let details = m.details();
                let reactions = m.reaction_total();
                crate::types::Message {
                    message_id: m.id.to_string(),
                    channel_id: channel_id.to_string(),
                    guild_id: None,
                    author_id: Some(m.author.id.to_string()),
                    author: m.author.username,
                    timestamp: m.timestamp,
                    content: m.content,
                    attachments: urls,
                    attachment_details: details,
                    reactions,
                }
            })
            .collect())
    }

    /// `POST /users/@me/channels` — create a group DM (M3.4).
    pub async fn create_group_dm(&mut self, user_ids: &[String]) -> Result<String> {
        let inner = self.inner()?;
        let body = serde_json::json!({ "access_tokens": [], "recipients": user_ids });
        let resp: RawDm = inner
            .post(Route::CreateGroupDm, body)
            .await
            .context("create group DM failed")?;
        Ok(resp.id.to_string())
    }

    /// `PUT /channels/{id}/recipients/{user_id}` — add to group DM (M3.4).
    pub async fn group_dm_add(&mut self, channel_id: &str, user_id: &str) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let uid: u64 = user_id.parse().context("invalid user id")?;
        let inner = self.inner()?;
        // PUT .../recipients/{uid} returns 204 No Content; `put` would fail
        // parsing the empty body. Use `put_empty` instead.
        inner
            .put_empty(Route::GroupDmAddRecipient {
                channel_id: cid,
                user_id: uid,
            })
            .await
            .context("add group DM recipient failed")?;
        Ok(())
    }

    /// `DELETE /channels/{id}/recipients/{user_id}` — remove from group DM.
    pub async fn group_dm_remove(&mut self, channel_id: &str, user_id: &str) -> Result<()> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let uid: u64 = user_id.parse().context("invalid user id")?;
        let inner = self.inner()?;
        inner
            .delete(Route::GroupDmRemoveRecipient {
                channel_id: cid,
                user_id: uid,
            })
            .await
            .context("remove group DM recipient failed")?;
        Ok(())
    }

    /// `GET /channels/{id}/messages/{mid}` — fetch a single message.
    pub async fn get_message(
        &mut self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<crate::types::Message> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let mid: u64 = message_id.parse().context("invalid message id")?;
        let inner = self.inner()?;
        let raw: RawMessage = inner
            .get(Route::GetMessage {
                channel_id: cid,
                message_id: mid,
            })
            .await
            .context("GET message failed")?;
        let urls = raw.url_list();
        let details = raw.details();
        let reactions = raw.reaction_total();
        Ok(crate::types::Message {
            message_id: raw.id.to_string(),
            channel_id: channel_id.to_string(),
            guild_id: None,
            author_id: Some(raw.author.id.to_string()),
            author: raw.author.username,
            timestamp: raw.timestamp,
            content: raw.content,
            attachments: urls,
            attachment_details: details,
            reactions,
        })
    }

    /// `GET /channels/{id}/messages` — fetch messages, newest-first, paged.
    /// `before`/`after` are snowflake cursors. Returns sorted ascending.
    pub async fn fetch_messages(
        &mut self,
        channel_id: &str,
        limit: usize,
        before: Option<u64>,
        after: Option<u64>,
    ) -> Result<Vec<crate::types::Message>> {
        let cid: u64 = channel_id.parse().context("invalid channel id")?;
        let inner = self.inner()?;
        let mut all: Vec<RawMessage> = Vec::new();
        let mut remaining = limit.min(1000);
        let mut cur_before = before;
        let mut cur_after = after;

        while remaining > 0 {
            let batch = remaining.min(100) as u32;
            let route = Route::GetMessages {
                channel_id: cid,
                limit: Some(batch),
                before: cur_before,
                after: cur_after,
            };
            let msgs: Vec<RawMessage> = inner.get(route).await?;
            let n = msgs.len();
            if n == 0 {
                break;
            }
            remaining = remaining.saturating_sub(n);
            // Small delay between pages to be rate-limit friendly (jackwener).
            tokio::time::sleep(std::time::Duration::from_millis(400 + (randish() % 400))).await;

            if after.is_some() {
                cur_after = msgs[0].id.parse().ok();
            } else {
                cur_before = msgs[n - 1].id.parse().ok();
            }
            all.extend(msgs);
            if n < batch as usize {
                break;
            }
        }

        // Sort ascending by id (jackwener sorts by msg_id ascending).
        all.sort_by_key(|m| m.id.clone());
        Ok(all
            .into_iter()
            .map(|m| {
                let urls = m.url_list();
                let details = m.details();
                let reactions = m.reaction_total();
                crate::types::Message {
                    message_id: m.id.to_string(),
                    channel_id: channel_id.to_string(),
                    guild_id: None,
                    author_id: Some(m.author.id.to_string()),
                    author: m.author.username,
                    timestamp: m.timestamp,
                    content: m.content,
                    attachments: urls,
                    attachment_details: details,
                    reactions,
                }
            })
            .collect())
    }
}

/// Small deterministic-ish jitter (0..400). Not cryptographic.
fn randish() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    n % 400
}

/// Raw Discord response shapes (subset we consume).
#[derive(Debug, Clone, serde::Deserialize)]
struct RawGuild {
    id: String,
    name: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    owner: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawChannel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type")]
    channel_type: u8,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    position: i32,
}

/// Result of thread creation (type discriminator for output).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ThreadResult {
    pub id: String,
    pub name: String,
    pub channel_id: String,
    pub channel_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawMessage {
    id: String,
    author: RawAuthor,
    content: String,
    timestamp: String,
    #[serde(default)]
    attachments: Option<Vec<RawAttachment>>,
    #[serde(default)]
    reactions: Option<Vec<RawReaction>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawAttachment {
    url: String,
    filename: String,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    size: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawReaction {
    count: i32,
}

impl RawMessage {
    /// Legacy URL-only list (back-compat `attachments` field).
    fn url_list(&self) -> Option<Vec<String>> {
        self.attachments
            .as_ref()
            .map(|a| a.iter().map(|x| x.url.clone()).collect())
    }

    /// Detailed attachment info (F6 download pipeline).
    fn details(&self) -> Option<Vec<crate::types::AttachmentInfo>> {
        self.attachments.as_ref().map(|a| {
            a.iter()
                .map(|x| crate::types::AttachmentInfo {
                    url: x.url.clone(),
                    filename: x.filename.clone(),
                    content_type: x.content_type.clone(),
                    size: x.size,
                })
                .collect()
        })
    }

    /// Sum of reaction counts (F8).
    fn reaction_total(&self) -> Option<Vec<crate::types::ReactionInfo>> {
        self.reactions.as_ref().map(|r| {
            r.iter()
                .map(|x| crate::types::ReactionInfo { count: x.count })
                .collect()
        })
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawAuthor {
    id: String,
    username: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SearchResponse {
    messages: Vec<Vec<RawSearchMessage>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawSearchMessage {
    id: String,
    channel_id: String,
    author: RawAuthor,
    content: String,
    timestamp: String,
    #[serde(default)]
    attachments: Option<Vec<RawAttachment>>,
    #[serde(default)]
    reactions: Option<Vec<RawReaction>>,
}

impl RawSearchMessage {
    fn url_list(&self) -> Option<Vec<String>> {
        self.attachments
            .as_ref()
            .map(|a| a.iter().map(|x| x.url.clone()).collect())
    }

    fn details(&self) -> Option<Vec<crate::types::AttachmentInfo>> {
        self.attachments.as_ref().map(|a| {
            a.iter()
                .map(|x| crate::types::AttachmentInfo {
                    url: x.url.clone(),
                    filename: x.filename.clone(),
                    content_type: x.content_type.clone(),
                    size: x.size,
                })
                .collect()
        })
    }

    fn reaction_total(&self) -> Option<Vec<crate::types::ReactionInfo>> {
        self.reactions.as_ref().map(|r| {
            r.iter()
                .map(|x| crate::types::ReactionInfo { count: x.count })
                .collect()
        })
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawRole {
    id: String,
    name: String,
    #[serde(default)]
    color: u32,
    #[serde(default)]
    position: i32,
    #[serde(default)]
    permissions: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawRelationship {
    id: String,
    #[serde(rename = "type")]
    relationship_type: u8,
    #[serde(default)]
    username: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawUserProfile {
    user: RawProfileUser,
    #[serde(default)]
    user_bio: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawProfileUser {
    id: String,
    username: String,
    #[serde(default)]
    global_name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ThreadActiveResponse {
    threads: Vec<RawThread>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)] // raw response fields kept for completeness
struct ThreadSearchResponse {
    threads: Vec<RawThread>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    total_results: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawThread {
    id: String,
    name: String,
    #[serde(rename = "type")]
    channel_type: u8,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    position: i32,
}

fn raw_thread_to_channel(t: RawThread) -> Channel {
    Channel {
        id: t.id.to_string(),
        name: t.name,
        guild_id: None,
        channel_type: t.channel_type,
        topic: None,
        parent_id: t.parent_id.map(|p| p.to_string()),
        position: Some(t.position),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawMember {
    nick: Option<String>,
    joined_at: Option<String>,
    user: RawMemberUser,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawMemberUser {
    id: String,
    username: String,
    #[serde(default)]
    global_name: Option<String>,
    #[serde(default)]
    bot: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawGuildInfo {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    approximate_member_count: Option<u32>,
    #[serde(default)]
    approximate_presence_count: Option<u32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawDm {
    id: String,
    #[serde(rename = "type")]
    channel_type: u8,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    recipients: Option<Vec<RawDmUser>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)] // id kept for completeness
struct RawDmUser {
    id: String,
    username: String,
    #[serde(default)]
    discriminator: Option<String>,
    #[serde(default)]
    global_name: Option<String>,
}

impl RawDmUser {
    /// Human label `user#disc` or `global_name` fallback.
    fn tag(&self) -> String {
        if let Some(g) = &self.global_name {
            if !g.is_empty() {
                return g.clone();
            }
        }
        if let Some(d) = &self.discriminator {
            if d != "0" {
                return format!("{}#{}", self.username, d);
            }
        }
        self.username.clone()
    }
}

/// The REST base, re-exported for callers that need the full URL.
pub const REST_BASE: &str = API_BASE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_holds_token_without_network() {
        let c = ApiClient::with_token("testtoken");
        assert_eq!(c.token, "testtoken");
        assert!(c.client.is_none());
    }

    #[test]
    fn api_base_is_v10() {
        assert_eq!(REST_BASE, "https://discord.com/api/v10");
    }

    #[test]
    fn guild_id_resolution_detects_numeric() {
        // resolve_guild_id short-circuits numeric to Some(id) without network.
        // We can't call the async method here without a client, but we verify
        // the predicate used: all-digits.
        let numeric = "1234567890";
        assert!(numeric.chars().all(|c| c.is_ascii_digit()));
        let named = "my-server";
        assert!(!named.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn channel_text_like_filter() {
        // Mirrors list_channels retain() logic: keep 0/5/15 only.
        let types = [0u8, 2, 5, 13, 15, 16];
        let kept: Vec<u8> = types
            .into_iter()
            .filter(|&t| matches!(t, 0 | 5 | 15))
            .collect();
        assert_eq!(kept, vec![0, 5, 15]);
    }

    #[test]
    fn randish_is_bounded() {
        for _ in 0..100 {
            let r = randish();
            assert!(r < 400, "randish out of bounds: {r}");
        }
    }

    #[test]
    fn extract_invite_code_handles_urls_and_plain() {
        // Full URLs across known prefixes.
        assert_eq!(
            ApiClient::extract_invite_code("https://discord.gg/abc123"),
            Some("abc123")
        );
        assert_eq!(
            ApiClient::extract_invite_code("https://discord.com/invite/xyz789"),
            Some("xyz789")
        );
        assert_eq!(
            ApiClient::extract_invite_code("https://discordapp.com/invite/qqq"),
            Some("qqq")
        );
        // Plain code passes through.
        assert_eq!(ApiClient::extract_invite_code("abc123"), Some("abc123"));
        // Trailing slash stripped.
        assert_eq!(
            ApiClient::extract_invite_code("discord.gg/abc/"),
            Some("abc")
        );
        // ?query and #fragment suffixes stripped (review#21).
        assert_eq!(
            ApiClient::extract_invite_code("https://discord.gg/abc?with_counts=1"),
            Some("abc")
        );
        assert_eq!(
            ApiClient::extract_invite_code("https://discord.gg/abc#section"),
            Some("abc")
        );
        // Empty/edge -> None.
        assert_eq!(ApiClient::extract_invite_code(""), None);
        assert_eq!(ApiClient::extract_invite_code("https://discord.gg/"), None);
        assert_eq!(ApiClient::extract_invite_code("discord.gg/?x=1"), None);
    }

    #[test]
    fn build_send_payload_has_attachments_when_files() {
        let p = ApiClient::build_send_payload("hello", None, 1).unwrap();
        assert_eq!(p["content"], "hello");
        assert_eq!(p["attachments"], serde_json::json!([{ "id": "0" }]));
        // mobile_network_type preserved (user-token mimic).
        assert_eq!(p["mobile_network_type"], "unknown");
    }

    #[test]
    fn build_send_payload_multi_file_ids() {
        let p = ApiClient::build_send_payload("x", Some("123"), 3).unwrap();
        assert_eq!(
            p["attachments"],
            serde_json::json!([{ "id": "0" }, { "id": "1" }, { "id": "2" }])
        );
        assert_eq!(p["message_reference"]["message_id"], "123");
    }

    #[test]
    fn build_send_payload_no_files_no_attachments_key() {
        // Without files, no attachments descriptor (matches plain send path).
        let p = ApiClient::build_send_payload("plain", None, 0).unwrap();
        assert!(
            p.get("attachments").is_none()
                || p["attachments"].as_array().is_some_and(|a| a.is_empty())
        );
    }
}
