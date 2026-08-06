//! Configuration: token resolution (flag > env > .env > keyring) + data dirs + settings.
//!
//! Ported from jackwener `config.py` (Apache-2.0, in `.tmp/`) and
//! `discord-cli-rs/src/config.rs` (MIT, `.tmp/`). Resolution order is the
//! contract every command relies on: `--token` flag → `DISCORD_TOKEN` env →
//! `./.env` → OS keyring.

use std::env;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

/// App identity used for data dirs and keyring service names.
pub const APP_NAME: &str = "discord-cli";
/// Discord REST API base (v10 — the current stable; matches discord-user-rs).
pub const API_BASE: &str = "https://discord.com/api/v10";

/// Settings resolved once at startup (color, output defaults).
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Disable ANSI color (also honored via NO_COLOR).
    pub no_color: bool,
    /// Explicit output format override (None = auto by isTTY).
    pub output_format: Option<OutputFormat>,
}

/// Output format selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Jsonl,
    Yaml,
    Rich,
}

/// Load `./.env` from cwd first, then fall back to the repo checkout root.
pub fn load_env() {
    let _ = dotenvy::dotenv();
    for candidate in [PathBuf::from(".env"), repo_root().join(".env")] {
        if candidate.is_file() {
            let _ = dotenvy::from_path(&candidate);
            return;
        }
    }
}

/// Absolute path of the workspace root (dir containing Cargo.toml), if known.
pub fn repo_root() -> PathBuf {
    env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Platform-appropriate base directory for application data
/// (mirrors jackwener `_default_data_home`).
pub fn default_data_home() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg);
    }
    #[cfg(windows)]
    {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local);
        }
        PathBuf::from(env::var("APPDATA").unwrap_or_else(|_| ".".into()))
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(
            env::var("HOME")
                .map(|h| format!("{}/Library/Application Support", h))
                .unwrap_or_else(|_| ".".into()),
        )
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        PathBuf::from(
            env::var("HOME")
                .map(|h| format!("{}/.local/share", h))
                .unwrap_or_else(|_| ".".into()),
        )
    }
}

/// Data dir for this app, created if missing.
pub fn data_dir() -> Result<PathBuf> {
    let raw = env::var("DATA_DIR").unwrap_or_default();
    let d = if raw.is_empty() {
        default_data_home().join(APP_NAME)
    } else {
        resolve_path(&raw)
    };
    std::fs::create_dir_all(&d).context("create data dir")?;
    Ok(d)
}

/// SQLite database path (default `<data_dir>/messages.db`), parent created.
pub fn db_path() -> Result<PathBuf> {
    let raw = env::var("DB_PATH").unwrap_or_default();
    let p = if raw.is_empty() {
        data_dir()?.join("messages.db")
    } else {
        resolve_path(&raw)
    };
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    Ok(p)
}

/// Resolve a possibly-relative path against cwd (expands ~).
pub fn resolve_path(raw: &str) -> PathBuf {
    let p = PathBuf::from(shellexpand::tilde(raw).to_string());
    if p.is_absolute() {
        p
    } else {
        env::current_dir().unwrap_or_default().join(p)
    }
}

/// Get the configured Discord token, or raise a clear error.
/// Order: explicit flag → DISCORD_TOKEN env → ./.env → keyring.
pub fn resolve_token(flag: Option<&str>) -> Result<String> {
    if let Some(t) = flag {
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Ok(t) = env::var("DISCORD_TOKEN") {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    if let Ok(t) = keyring_token() {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    Err(anyhow!(
        "DISCORD_TOKEN not set. Run `discord auth --save` (auto-detect), `discord auth --paste`, \
         or set the token in .env / DISCORD_TOKEN."
    ))
}

/// Read token from OS keyring (service `discord-cli`, user `token`).
pub fn keyring_token() -> Result<String> {
    let entry = keyring::Entry::new(APP_NAME, "token")?;
    Ok(entry.get_password().unwrap_or_default())
}

/// Save token to OS keyring.
pub fn save_token_keyring(token: &str) -> Result<()> {
    let entry = keyring::Entry::new(APP_NAME, "token")?;
    entry.set_password(token)?;
    Ok(())
}

/// Delete token from keyring (best-effort).
pub fn delete_token_keyring() -> Result<()> {
    let entry = keyring::Entry::new(APP_NAME, "token")?;
    entry.delete_credential()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_beats_env() {
        let _env = std::env::var("DISCORD_TOKEN");
        std::env::set_var("DISCORD_TOKEN", "envtoken");
        let r = resolve_token(Some("flagtoken"));
        assert_eq!(r.unwrap(), "flagtoken");
        std::env::remove_var("DISCORD_TOKEN");
        if let Ok(v) = _env {
            std::env::set_var("DISCORD_TOKEN", v);
        }
    }

    #[test]
    fn env_token_used_when_no_flag() {
        let _env = std::env::var("DISCORD_TOKEN");
        std::env::set_var("DISCORD_TOKEN", "envtoken");
        let r = resolve_token(None);
        assert_eq!(r.unwrap(), "envtoken");
        std::env::remove_var("DISCORD_TOKEN");
        if let Ok(v) = _env {
            std::env::set_var("DISCORD_TOKEN", v);
        }
    }

    #[test]
    fn missing_token_errors_clearly() {
        let _env = std::env::var("DISCORD_TOKEN");
        std::env::remove_var("DISCORD_TOKEN");
        let r = resolve_token(None);
        assert!(r.is_err());
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("DISCORD_TOKEN"), "msg: {msg}");
        if let Ok(v) = _env {
            std::env::set_var("DISCORD_TOKEN", v);
        }
    }

    #[test]
    fn api_base_is_v10() {
        assert_eq!(API_BASE, "https://discord.com/api/v10");
    }
}
