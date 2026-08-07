//! `discord download` — offline attachment download from the SQLite archive.
//!
//! Ported from langkurt `cmd/discocli/commands/download.go` + `timeutil.go`
//! (MIT, `.tmp/`). Runs purely against the DB — zero Discord API calls.

use std::process::ExitCode;

use anyhow::Result;
use discord_core::output::{self, exit};
use discord_db::attachments::AttachmentFilter;
use discord_db::db as ddb;

use super::dc::DcCtx;

/// Discord epoch (ms) — snowflake timestamp base (langkurt timeutil.go:49).
#[allow(dead_code)] // used by time_to_snowflake (--since filter)
const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;

/// Parse a `--since` value: exact date `YYYY-MM-DD` or `<n><d|m|y>`.
/// Returns a UTC DateTime (langkurt ParseSince timeutil.go:13-43).
fn parse_since(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Exact date.
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d
            .and_hms_opt(0, 0, 0)
            .map(|t| chrono::DateTime::from_naive_utc_and_offset(t, chrono::Utc));
    }
    // <n><d|m|y> relative.
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

/// Convert a DateTime to a snowflake cutoff (langkurt TimeToSnowflake:49-57).
/// `(ms - epoch) << 22`, clamped at 0 for pre-epoch dates.
#[allow(dead_code)] // reserved for --since snowflake cutoff (tests cover math)
fn time_to_snowflake(t: chrono::DateTime<chrono::Utc>) -> String {
    let ms = t.timestamp_millis();
    let shifted = (ms - DISCORD_EPOCH_MS).max(0) << 22;
    shifted.to_string()
}

/// Sanitise a name for use as a directory segment (langkurt
/// sanitiseName download.go:123-134).
fn sanitise_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Download a single attachment (plain GET with browser UA). Returns bytes.
async fn fetch_attachment(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(discord_core::stealth::browser_user_agent())
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(bytes.to_vec())
}

/// Options for `dc download` (avoids clippy too_many_arguments).
#[derive(Default)]
pub struct DownloadOpts<'a> {
    pub guild: Option<&'a str>,
    pub channel: Option<&'a str>,
    pub media_type: Option<&'a str>,
    pub since: Option<&'a str>,
    pub min_reactions: Option<i64>,
    pub limit: Option<i64>,
    pub out: Option<&'a str>,
}

/// `dc download [--guild G] [--channel C] [--type T] [--since S]
///            [--min-reactions N] [--limit N] [--out DIR]`
pub async fn dc_download(ctx: &DcCtx, opts: DownloadOpts<'_>) -> ExitCode {
    let DownloadOpts {
        guild,
        channel,
        media_type,
        since,
        min_reactions,
        limit,
        out,
    } = opts;
    // Validate media type early (usage exit 2).
    if let Some(t) = media_type {
        if !matches!(t, "image" | "gif" | "video" | "all") {
            eprintln!("invalid --type: \"{t}\" (valid: image, gif, video, all)");
            return ExitCode::from(exit::USAGE);
        }
    }
    // Resolve guild/channel names to IDs via the archive (langkurt resolves
    // from local DB, not the API).
    let db_path = match discord_core::config::db_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::ERROR);
        }
    };
    let conn = match ddb::open(db_path.to_str().unwrap_or("discord.db")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error opening archive: {e}");
            return ExitCode::from(exit::ERROR);
        }
    };

    // Build filter. Guild/channel resolved by name via channels table.
    let mut filter = AttachmentFilter {
        media_type: media_type.filter(|t| *t != "all").map(|s| s.to_string()),
        min_reactions,
        limit: limit.unwrap_or(0),
        ..Default::default()
    };
    if let Some(s) = since {
        match parse_since(s) {
            Some(t) => filter.since = Some(t.to_rfc3339()),
            None => {
                eprintln!("invalid --since: \"{s}\" (use YYYY-MM-DD or 30d/6m/1y)");
                return ExitCode::from(exit::USAGE);
            }
        }
    }
    // channel/guild name -> id via channels table (review#5: guild_id NULL in
    // messages, so channel_id filter is authoritative; guild via JOIN).
    if let Some(c) = channel {
        let ch_id = ddb::find_channel_id(&conn, c).ok().flatten();
        match ch_id {
            Some(id) => filter.channel_id = Some(id),
            None => {
                eprintln!("channel \"{c}\" not found in archive (sync it first)");
                return ExitCode::from(exit::NOT_FOUND);
            }
        }
    }
    if let Some(g) = guild {
        let gid = ddb::find_guild_id(&conn, g).ok().flatten();
        match gid {
            Some(id) => filter.guild_id = Some(id),
            None => {
                eprintln!("guild \"{g}\" not found in archive");
                return ExitCode::from(exit::NOT_FOUND);
            }
        }
    }

    let rows = match discord_db::attachments::list_pending_attachments(&conn, &filter) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error querying attachments: {e}");
            return ExitCode::from(exit::ERROR);
        }
    };
    if rows.is_empty() {
        let data = serde_json::json!({ "downloaded": 0, "skipped": 0, "pending": 0 });
        let _ = output::emit(&data, ctx.format);
        return ExitCode::from(exit::OK);
    }

    // Resolve per-guild/channel names for output dirs (from archive).
    let out_root = out.map(|s| s.to_string()).unwrap_or_else(|| {
        discord_core::config::data_dir()
            .map(|p| p.join("media").to_string_lossy().into_owned())
            .unwrap_or_else(|_| "media".into())
    });

    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    for a in &rows {
        // Destination: <out>/<guild>/<channel>/<msgID8>_<filename>.
        let guild_name = ddb::guild_name_for_channel(&conn, &a.channel_id)
            .unwrap_or_else(|_| "unknown".to_string());
        let ch_name =
            ddb::channel_name(&conn, &a.channel_id).unwrap_or_else(|_| a.channel_id.clone());
        let dir = std::path::Path::new(&out_root)
            .join(sanitise_name(&guild_name))
            .join(sanitise_name(&ch_name));
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("cannot create dir {}: {e}", dir.display());
            continue;
        }
        let suffix: String = a
            .message_id
            .chars()
            .rev()
            .take(8)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let local = dir.join(format!("{suffix}_{}", a.filename));
        if local.exists() {
            skipped += 1;
            let _ =
                discord_db::attachments::mark_downloaded(&conn, &a.id, &local.to_string_lossy());
            continue;
        }
        // Progress to stderr (Escape-Tech \r\x1b[K pattern).
        eprint!("\rDownloading {}... ", local.display());
        match fetch_attachment(&a.url).await {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&local, &bytes) {
                    eprintln!("\rfailed to write {}: {e}", local.display());
                    continue;
                }
                let _ = discord_db::attachments::mark_downloaded(
                    &conn,
                    &a.id,
                    &local.to_string_lossy(),
                );
                downloaded += 1;
            }
            Err(e) => {
                eprintln!("\rfailed to fetch {}: {e}", a.url);
            }
        }
        eprint!("\r\x1b[K");
        // 200ms pacing between downloads (langkurt download.go:77).
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let data = serde_json::json!({
        "downloaded": downloaded,
        "skipped": skipped,
        "pending": rows.len(),
    });
    let _ = output::emit(&data, ctx.format);
    ExitCode::from(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_since_date_and_relative() {
        let d = parse_since("2026-01-01").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-01-01");
        assert!(parse_since("30d").is_some());
        assert!(parse_since("6m").is_some());
        assert!(parse_since("1y").is_some());
        assert!(parse_since("0d").is_none());
        assert!(parse_since("bogus").is_none());
    }

    #[test]
    fn snowflake_cutoff_math() {
        fn utc(s: &str) -> chrono::DateTime<chrono::Utc> {
            chrono::DateTime::parse_from_rfc3339(s).unwrap().into()
        }
        // 2015-01-01 = epoch -> 0.
        assert_eq!(time_to_snowflake(utc("2015-01-01T00:00:00Z")), "0");
        // 2016-01-01 = epoch + 1y -> (1y ms) << 22 > 0.
        assert!(
            time_to_snowflake(utc("2016-01-01T00:00:00Z"))
                .parse::<i64>()
                .unwrap()
                > 0
        );
        // Pre-epoch clamps to 0.
        assert_eq!(time_to_snowflake(utc("2014-01-01T00:00:00Z")), "0");
    }

    #[test]
    fn sanitise_name_replaces_special_chars() {
        assert_eq!(
            sanitise_name("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitise_name("general"), "general");
    }
}
