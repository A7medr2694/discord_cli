//! Client wrapper over `discord-user-rs`'s `DiscordHttpClient`.
//!
//! Provides browser headers + X-Super-Properties (via `set_super_properties_b64`)
//! + rate-limit handling. Full stealth header set lands in M8; the core here is
//! the thin typed layer commands call.
//!
//! `discord-user-rs` is the MIT core crate (plan §2.2).

use anyhow::{Context, Result};
use discord_user::client::DiscordHttpClient;
use discord_user::route::Route;

use crate::config::{API_BASE, resolve_token};
use crate::types::Me;

/// Authenticated API client backed by `discord-user-rs`.
///
/// Holds the token and lazily constructs the underlying `DiscordHttpClient`.
pub struct ApiClient {
    token: String,
    client: Option<DiscordHttpClient>,
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
            // X-Super-Properties set in M8 (stealth); for now leave default.
            self.client = Some(c);
        }
        Ok(self.client.as_mut().unwrap())
    }

    /// `GET /users/@me` — current user.
    pub async fn get_me(&mut self) -> Result<Me> {
        let inner = self.inner()?;
        inner.get(Route::GetMe).await.context("GET /users/@me failed")
    }

    /// Validate token: `GET /users/@me` returns 200.
    pub async fn validate(&mut self) -> Result<bool> {
        match self.get_me().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
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
}
