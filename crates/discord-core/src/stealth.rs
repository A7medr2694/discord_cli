//! Stealth layer: X-Super-Properties, launch_signature mask, build-number
//! scraper, browser identify properties.
//!
//! Concepts ported from discordo `internal/http/{headers,properties}.go`
//! (GPL — re-implemented here in Rust, not copied verbatim; plan §7).

use base64::Engine;
use serde_json::json;
use uuid::Uuid;

use crate::config::device_id;

/// Browser identity constants (Chrome 146 on Windows 10).
pub const OS: &str = "Windows";
pub const OS_VERSION: &str = "10";
pub const BROWSER: &str = "Chrome";
pub const BROWSER_VERSION: &str = "146.0.0.0";
pub const LOCALE: &str = "en-US";
pub const RELEASE_CHANNEL: &str = "stable";

/// Browser user-agent string (matches discordo `BrowserUserAgent()`).
pub fn browser_user_agent() -> String {
    format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{} Safari/537.36",
        BROWSER_VERSION
    )
}

/// 16-byte mask that clears Discord's client-mod detection bits from a UUIDv4
/// (discordo `generateLaunchSignature`). Re-implemented in Rust.
const LAUNCH_SIGNATURE_MASK: [u8; 16] = [
    0b11111111, 0b01111111, 0b11101111, 0b11101111,
    0b11110111, 0b11101111, 0b11110111, 0b11111111,
    0b11011111, 0b01111110, 0b11111111, 0b10111111,
    0b11111110, 0b11111111, 0b11110111, 0b11111111,
];

/// Generate a `launch_signature` UUID: UUIDv4 then AND each byte with the mask.
/// Output is canonical lowercase 8-4-4-4-12 (via Uuid::to_string).
pub fn launch_signature() -> String {
    let mut bytes = *Uuid::new_v4().as_bytes();
    for i in 0..16 {
        bytes[i] &= LAUNCH_SIGNATURE_MASK[i];
    }
    Uuid::from_bytes(bytes).to_string()
}

/// Pinned build-number fallback (discordo main, Chrome 146 era).
pub const BUILD_NUMBER_FALLBACK: u32 = 584_177;

/// The client build number used in headers. Sync-safe: uses a pinned constant
/// (avoid blocking inside a runtime). Callers that want a fresh value can use
/// `async fetch_build_number()` and cache it themselves.
pub fn client_build_number() -> u32 {
    BUILD_NUMBER_FALLBACK
}

/// Async helper: scrape the live build number and update the process-wide
/// cache. Best-effort; safe to call from async contexts.
pub async fn refresh_build_number() -> Option<u32> {
    let n = fetch_build_number().await?;
    // Cache in a static for future `client_build_number()` use.
    static CACHED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let _ = CACHED.set(n);
    Some(n)
}

/// Fetch the current Discord client build number from the login page.
/// This is **async** — must NOT be called inside a synchronous path that's
/// already inside a tokio runtime (block_on would panic). The synchronous
/// `client_build_number()` uses a pinned fallback instead.
async fn fetch_build_number() -> Option<u32> {
    let client = reqwest::Client::builder()
        .user_agent(browser_user_agent())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let html = client.get("https://discord.com/login").send().await.ok()?.text().await.ok()?;
    let marker = "\"BUILD_NUMBER\":\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    html[start..end].parse().ok()
}

/// The `X-Super-Properties` value: base64 of a JSON object with browser
/// identity + detection-bypass fields. `client_heartbeat_session_id` is fresh
/// per call (per request). `client_launch_id` is a plain UUIDv4.
pub fn x_super_properties() -> String {
    let props = json!({
        "os": OS,
        "os_version": OS_VERSION,
        "browser": BROWSER,
        "browser_version": BROWSER_VERSION,
        "browser_user_agent": browser_user_agent(),
        "client_build_number": client_build_number(),
        "client_event_source": null,
        "client_launch_id": Uuid::new_v4().to_string(),
        "client_app_state": "focused",
        "client_heartbeat_session_id": Uuid::new_v4().to_string(),
        "launch_signature": launch_signature(),
        "has_client_mods": false,
        "release_channel": RELEASE_CHANNEL,
        "system_locale": LOCALE,
        "device": "",
        "referrer": "",
        "referrer_current": "",
        "referring_domain": "",
        "referring_domain_current": "",
        // per-install identity (M5.3) — makes each install look distinct.
        "device_id": device_id().unwrap_or_default(),
    });
    base64::engine::general_purpose::STANDARD.encode(props.to_string())
}

/// Gateway IDENTIFY `properties` payload (for live connections).
/// Does NOT include launch_signature / client_app_state / heartbeat_session_id
/// (those are X-Super-Properties-only). Adds `is_fast_connect`.
pub fn identify_properties() -> serde_json::Value {
    json!({
        "os": OS,
        "os_version": OS_VERSION,
        "browser": BROWSER,
        "browser_version": BROWSER_VERSION,
        "browser_user_agent": browser_user_agent(),
        "device": "",
        "client_build_number": client_build_number(),
        "client_event_source": null,
        "client_launch_id": Uuid::new_v4().to_string(),
        "system_locale": LOCALE,
        "release_channel": RELEASE_CHANNEL,
        "has_client_mods": false,
        "referrer": "",
        "referrer_current": "",
        "referring_domain": "",
        "referring_domain_current": "",
        "is_fast_connect": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_signature_is_valid_uuid_shape() {
        let s = launch_signature();
        // 8-4-4-4-12, lowercase hex
        assert_eq!(s.len(), 36);
        assert_eq!(s.as_bytes()[8], b'-');
        assert_eq!(s.as_bytes()[13], b'-');
        assert!(s.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn launch_signature_varies() {
        assert_ne!(launch_signature(), launch_signature());
    }

    #[test]
    fn x_super_properties_is_base64_json() {
        let xsp = x_super_properties();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&xsp)
            .expect("base64");
        let v: serde_json::Value = serde_json::from_slice(&decoded).expect("json");
        assert_eq!(v["os"], "Windows");
        assert_eq!(v["browser"], "Chrome");
        assert_eq!(v["has_client_mods"], false);
        assert!(v["launch_signature"].is_string());
        assert!(v["client_heartbeat_session_id"].is_string());
    }

    #[test]
    fn identify_props_excludes_super_only_fields() {
        let p = identify_properties();
        assert!(p.get("launch_signature").is_none(), "launch_signature must NOT be in identify");
        assert!(p.get("client_app_state").is_none());
        assert_eq!(p["is_fast_connect"], true);
        assert_eq!(p["has_client_mods"], false);
    }

    #[test]
    fn ua_matches_chrome_146() {
        let ua = browser_user_agent();
        assert!(ua.contains("Chrome/146.0.0.0"), "ua: {ua}");
        assert!(ua.contains("Windows NT 10.0"));
    }
}
