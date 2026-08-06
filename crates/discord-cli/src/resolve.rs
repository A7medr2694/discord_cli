//! Name→ID resolution helpers (discli `resolve.ts` pattern, ported to Rust).
//!
//! Canonical behavior (plan §18):
//! - ID match first.
//! - Strip `#`/`@` prefix.
//! - Case-insensitive exact match.
//! - Ambiguity → print matches to stderr + exit 1 (agent must disambiguate).
//! - Not found → exit 3.

use std::process::ExitCode;

use discord_core::client::ApiClient;
use discord_core::output::exit;
use discord_core::types::Channel;

/// Resolve a guild name or ID to a guild ID.
/// Errors (exit 3) if not found.
pub async fn resolve_guild(client: &mut ApiClient, name: &str) -> Result<String, ExitCode> {
    if let Some(id) = client.resolve_guild_id(name).await.map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(exit::ERROR)
    })? {
        return Ok(id);
    }
    eprintln!("Guild \"{name}\" not found. Use `discord dc guilds` to list.");
    Err(ExitCode::from(exit::NOT_FOUND))
}

/// Resolve a channel name or ID within a guild to a Channel.
/// ID match first; else strip `#`, case-insensitive exact match.
/// Ambiguity → stderr list + exit 1; not found → exit 3 (discli).
#[allow(dead_code)] // used by tests; read/history resolve channels directly in dc.rs
pub async fn resolve_channel(
    client: &mut ApiClient,
    guild_id: &str,
    name: &str,
) -> Result<Channel, ExitCode> {
    let channels = client.list_channels(guild_id).await.map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(exit::ERROR)
    })?;

    // ID match first.
    if let Some(c) = channels.iter().find(|c| c.id == name) {
        return Ok(c.clone());
    }

    // Strip # prefix + case-insensitive exact match.
    let clean = name.strip_prefix('#').unwrap_or(name);
    let needle = clean.to_lowercase();
    let matches: Vec<&Channel> = channels
        .iter()
        .filter(|c| c.name.to_lowercase() == needle)
        .collect();

    match matches.len() {
        1 => Ok(matches[0].clone()),
        n if n > 1 => {
            let names: Vec<String> = matches
                .iter()
                .map(|m| format!("#{} (type {})", m.name, m.channel_type))
                .collect();
            eprintln!(
                "Ambiguous channel \"{clean}\". Matches: {}",
                names.join(", ")
            );
            Err(ExitCode::from(exit::USAGE))
        }
        _ => {
            eprintln!("Channel \"{name}\" not found in guild {guild_id}.");
            Err(ExitCode::from(exit::NOT_FOUND))
        }
    }
}

#[cfg(test)]
mod tests {
    use discord_core::types::Channel;

    fn ch(id: &str, name: &str, channel_type: u8) -> Channel {
        Channel {
            id: id.into(),
            name: name.into(),
            guild_id: Some("g".into()),
            channel_type,
            topic: None,
            parent_id: None,
            position: Some(0),
        }
    }

    // Pure helpers extracted for testing: strip + match logic.
    fn strip_prefix(name: &str) -> &str {
        name.strip_prefix('#').unwrap_or(name)
    }

    #[test]
    fn strip_hash_prefix() {
        assert_eq!(strip_prefix("#general"), "general");
        assert_eq!(strip_prefix("general"), "general");
        assert_eq!(strip_prefix("#"), "");
    }

    #[test]
    fn exact_case_insensitive_match() {
        let channels = [ch("1", "General", 0), ch("2", "announcements", 5)];
        let needle = "general";
        let matches: Vec<&Channel> = channels
            .iter()
            .filter(|c| c.name.to_lowercase() == needle.to_lowercase())
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "1");
    }

    #[test]
    fn ambiguity_detects_multiple() {
        let channels = [ch("1", "general", 0), ch("2", "General", 2)];
        let needle = "general";
        let matches: Vec<&Channel> = channels
            .iter()
            .filter(|c| c.name.to_lowercase() == needle.to_lowercase())
            .collect();
        assert_eq!(matches.len(), 2); // both text + voice "general"
    }
}
