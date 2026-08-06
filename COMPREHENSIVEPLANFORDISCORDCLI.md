# COMPREHENSIVE PLAN — Discord CLI in Rust for AI Agents

> **Goal:** A Rust CLI + MCP server that an AI agent (Claude Code, etc.) uses to manage the human's **personal Discord account** — reading messages in any server/group/DM the human belongs to (no bot invitation required), sending messages, searching history, managing guilds — outputting machine-readable data.
>
> **Auth:** user account token (self-bot style). **Why not a bot?** A bot only sees channels it is invited to. A user token reads *everything* the account can see.
>
> **⚠️ ToS notice:** automating a user account violates Discord's Terms of Service and can result in account termination. This tool is built for accounts you control, on machines you control. Safety is built in by default (rate limiting, invalid-request budget, bounded reads, invisible presence, no mass-crawl). This warning MUST appear in the README and `--help`.

---

## Table of Contents

1. [Context & Motivation](#1-context--motivation)
2. [Research Findings](#2-research-findings)
   - 2.1 The 14 researched repos
   - 2.2 Rust ecosystem
   - 2.3 What to copy / what to avoid (licensing)
3. [Confirmed Decisions](#3-confirmed-decisions)
4. [Architecture Overview](#4-architecture-overview)
5. [Dependency Manifest](#5-dependency-manifest)
6. [SQLite Schema](#6-sqlite-schema)
7. [Stealth / Anti-Detection Layer](#7-stealth--anti-detection-layer)
8. [Authentication](#8-authentication)
9. [Command Surface](#9-command-surface)
10. [Output Contract](#10-output-contract)
11. [MCP Server Tools](#11-mcp-server-tools)
12. [Rate Limiting & Safety](#12-rate-limiting--safety)
13. [Error Handling & Exit Codes](#13-error-handling--exit-codes)
14. [Implementation Order (Milestones)](#14-implementation-order-milestones)
15. [Files to Create](#15-files-to-create)
16. [References & Sources](#16-references--sources)
17. [Verification Plan](#17-verification-plan)
18. [Open Items / Future Work](#18-open-items--future-work)

---

## 1. Context & Motivation

The user wants to delegate Discord account management to an AI agent: read messages across all their servers and DMs, reply, search history, and manage their guilds — from the terminal, without opening the Discord app. Because the human is already a member of these servers, a **user token** gives read access to everything the account can see — unlike a bot, which is limited to servers where it was invited.

**The critical realization:** the underlying technique is proven by many open-source projects (self-bot clients). The reason to build a *new* tool instead of using an existing one is: (a) the user wants it in **Rust**, (b) we want first-class **AI-agent ergonomics** (structured output + MCP), (c) we want **DM support** (which most CLI bots lack), and (d) we want a **modern anti-detection layer** (which most repos lack).

## 2. Research Findings

### 2.1 The 14 researched repos (cloned to `discord_cli/.tmp/`)

Research was done by 4 parallel sub-agents reading full source, plus direct source extraction. Summary:

| Repo | Lang | Auth | Read-all-guilds | DM | Search | Send | AI-friendly | Verdict |
|---|---|---|---|---|---|---|---|---|
| `jackwener/discord-cli` | Python | **user** | ✅ REST `/@me/guilds` | ❌ | ✅ native+SQLite | ❌ | ⭐⭐ JSON/YAML+SKILL | **Best REST-first template** (port to Rust) |
| `ayn2op/discordo` ⭐5.4k | Go | **user** | ✅ Gateway+REST | ✅ | ❌ | ✅ | ✖ TUI-only | **Best anti-detection** (TLS+headers) |
| `langkurt/discord-cli` | Go | **user+bot** | ✅ REST | ❌ | ✅ FTS5 | ✅ | ⭐ MCP server real | **Best MCP + sync template** |
| `Stone-Red-Code/DiscordCLI` | C# | **user** | ✅ | ✅ PrivateChannels | ❌ | ✅ | ✖ REPL | Proof user-token DM works |
| `famasya/discord-cli-agent` | Go | bot | ❌ | ❌ | ✅ scan | ✅ | ⭐⭐ JSONL/exit-codes | **Best agent output model** |
| `ibbybuilds/discli` | TS | bot | ❌ | ❌ | ❌ | ✅ | ⭐⭐ YAML/SCHEMA/SKILL | **Best command-surface + safety** |
| `fourjr/discord-cli` | Python | user | ❌ 1 channel | ❌ | ❌ | ✅ | ✖ | Old (2020), reference only |
| `mrarfarf/discord-cli` | Go | user | ❌ 1 channel | ❌ | ❌ | ❌ | line-stdout | **Keyring + QR-remote-auth reference** |
| `Escape-Technologies/discord-cli` | JS | bot | ❌ | ❌ | ✅ scan | ✅ | `--json` | Bot-only |
| `RickvanLoo` / `Rivalo` | Go | email/pass | ❌ | ❌ | ❌ | ✅ | ✖ | Dead (email login removed) |
| `sinjs/clicord` | Go | user | ✅ | — | ❌ | ✅ | ✖ | **MIT**, email+2FA login |
| `ThePolishCat/discord-cli` | JS | bot | ❌ | ❌ | ❌ | ✅ | ✖ | Toy, skip |
| `Linotypefibre247/discord-cli` | Go | — | — | — | — | — | — | **⚠️ MALWARE** — deleted; `cmd/cli_discord_mopla.zip` (Launcher.cmd → lua51.exe → obfuscated Lua dropper). Do not re-clone. |

**Key confirmed facts across all user-token repos:**
- Reading ANY server of a human user requires a user token + `GET /users/@me/guilds` + `GET /guilds/{id}/channels` + `GET /channels/{id}/messages` (REST) or the gateway — **no bot needed to be present**.
- DMs/group-DMs are read via `GET /users/@me/channels` (works with user token; most bot CLIs lack this).
- Threads: some endpoints are bot-only (e.g. `ThreadsActive`) — user-token fallback is `GET /channels/{id}/threads/search` (what Discord's own app uses).
- Rate limiting + jitter + backoff is mandatory (jackwener sleeps 0.3–1.0s; langkurt 400–700ms + 429 backoff 2s→10s).
- The heavy anti-detection lift is TLS fingerprint spoofing + browser headers + X-Super-Properties.

### 2.2 Rust ecosystem (via Exa research + `cargo` verification)

| Crate | Version | Role |
|---|---|---|
| `discord-user-rs` | 0.6.1 (MIT, active 2026) | **Core.** Purpose-built Rust selfbot client: `DiscordHttpClient` (rate-limit-aware, X-Super-Properties, Cloudflare-1015 `InvalidRequestTracker`), `DiscordUser` (full gateway: reconnect, RESUME vs fresh-IDENTIFY by close code, auto-fetch build number, typed events, `MessageOps`/`ChannelOps`/`GuildOps`/`RelationshipOps`/`StatusOps`). |
| `discord-cli-rs` | 0.2.0 (MIT) | Rust port of jackwener by same author. 30+ commands, SQLite, auth (win/linux/mac). **Reference only** (user chose build-from-scratch). |
| `rmcp` | 3.1.1 (official MCP Rust SDK) | MCP server via `#[tool]` macro + stdio transport. Standard for Claude Code. |
| `webclaw-tls` | git-only | **Perfect Chrome 146 JA4 + Akamai match**; requires `[patch.crates-io]` on rustls/h2/hyper. Escalation option. |
| `impersonate-rs` | 0.1.0 | curl-impersonate FFI wrapper; `ja3()`/`akamai()` custom fingerprints. Needs system lib. |
| `hyprcurl` | — | curl_cffi-inspired Rust client; per-request browser impersonation. |

**Recommendation:** default to hardened `reqwest` (Chrome UA + browser headers + `http1_only()`) because Discord does not currently enforce hard JA4 on its API; keep TLS impersonation as an **optional feature-gated** escalation.

### 2.3 What to copy / what to avoid (licensing)

| Repo | License | Copy status |
|---|---|---|
| `jackwener` | Apache-2.0 | ✅ Copy/port freely |
| `langkurt` | MIT | ✅ Copy/port freely |
| `famasya` | MIT | ✅ Copy/port freely |
| `mrarfarf` | BSD/MIT-style | ✅ Copy/port freely |
| `discord-cli-rs` / `discord-user-rs` | MIT | ✅ Copy/port freely |
| `fourjr` | no license file | ⚠️ Reference only |
| `ayn2op-discordo` | GPL-3.0 | ⚠️ Copy **concepts** only (headers, launch_signature mask, identify props) — re-implement in Rust. Verbatim would force GPL on this project. |
| `RickvanLoo`/`Rivalo` | GPL-3.0 | ⚠️ Concepts only |
| `Stone-Red-Code` | GPL-3.0 | ⚠️ Concepts only |

Since the target is Rust and the `.tmp` code is Go/Python/C#/TS, "copy" = **port the logic** into Rust. The license concern applies to the ported result. Default stance: adapt concepts from GPL repos, port code from permissive repos.

## 3. Confirmed Decisions

1. **Build from scratch** in Rust, with `discord-user-rs` as the core library — but free to closely follow / copy code from `.tmp` repos (user explicitly approved: "follow chính xác hoặc copy code từ .tmp là k thành vấn đề"). **Hybrid strategy:** use `DiscordHttpClient` + `Route` + `set_super_properties_b64()` from the crate as the transport core (don't rewrite REST plumbing); write our own CLI surface + MCP server + stealth helpers on top. Reference (don't enable wholesale) the crate's `cli` feature and `discord-cli-rs` for command ideas.
2. **Auth:** both auto-detect (Windows DPAPI + LevelDB; Linux/macOS fallback) **and** manual paste.
3. **AI interface:** MCP server (stdio, `rmcp`) **plus** a CLI (JSONL by default).
4. **Plan language:** English (technical terms kept in English).

## 4. Architecture Overview

```
discord_cli/                        (new Rust workspace, binary crate "discord-cli")
├── Cargo.toml
├── src/
│   ├── main.rs                     clap entry: top-level commands + global flags
│   ├── config.rs                   token resolution (flag > env > .env > keyring), data dir, settings
│   ├── output.rs                   JSONL/JSON/YAML/rich renderers, envelope {ok, schema_version, data|error}
│   ├── db.rs                       rusqlite: WAL, schema below, dedup UNIQUE(channel_id,msg_id)
│   ├── types.rs                    serde structs: Guild, Channel, Message, DM, Snowflake helpers
│   ├── auth/
│   │   ├── mod.rs                  resolve_token(): CLI flag → env → .env → auto-detect → prompt
│   │   ├── detect.rs               LevelDB scan (Discord app/PTB/Canary/Chrome/Brave/Edge) + regex
│   │   ├── windows.rs              DPAPI decrypt of local-state key, Chromium-leveldb token decrypt
│   │   ├── linux.rs                (keyring / dbus fallback)
│   │   └── macos.rs                (keyring / Chrome Keychain fallback)
│   ├── client.rs                   wrapper over DiscordHttpClient: browser headers, X-Super-Properties,
│   │                               InvalidRequestTracker on 401/403/429, jitter+backoff
│   ├── stealth.rs                  build-number scraper, identify properties, header assembly
│   ├── mcp.rs                      rmcp server: #[tool] impls, stdio transport
│   └── commands/                   one file per CLI verb (see Command surface)
├── tests/                          integration tests (mock HTTP via reqwest mock / fake DiscordHttpClient)
├── scripts/                        clone-branches.* already exist (repo research tooling)
└── README.md                       ToS warning, quick start, output contract, MCP wiring
```

**Design principle (from discordo):** keep a UI-agnostic transport/state layer separate from the CLI presentation. The CLI and the MCP server share the same `client.rs` / `db.rs` / `types.rs` core; `mcp.rs` and `commands/*` are thin presentation layers over it.

**⚠️ Major finding (research re-run):** `discord-user-rs` v0.6.1 ships a feature `cli` that **already merges in a ~60-command CLI** — `dc guilds/channels/dms/dms-history/send/edit/delete/react/pin/dm/dm-group/typing/voice-message/sync/search-remote/snapshot/profile/relationships/roles/stickers/emojis/events/audit/threads/pins/tail` + `dcw_*` write-API groups (guild/members/perms/roles/webhooks/friends/invites/stage/soundboard/automod/usersettings) + local `search/recent/stats/today/top/timeline/export/purge`. It also has a `Route` enum registry (declarative, rate-limit bucketing via `get_route_key`, `Custom` escape hatch) and `DiscordHttpClient`. **Implication for "build from scratch":** we can either (a) enable `discord-user-rs`'s `cli` feature and build the MCP server + stealth on top of it, or (b) use the crate as a library and write our own CLI (the current plan). Recommendation: **hybrid** — use `DiscordHttpClient` + `Route` from the crate as the core (don't rewrite the REST plumbing), write our own CLI surface (so we control command grammar, JSONL output, MCP), and port the *concepts* of its commands into our command table. Avoid reinventing what the crate already provides.

## 5. Dependency Manifest

```toml
[package]
name = "discord-cli"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"          # YAML output
rusqlite = { version = "0.32", features = ["bundled"] }  # no system sqlite dep
rmcp = { version = "3", features = ["server", "transport-io", "schemars"] }
discord-user-rs = { version = "0.6", default-features = false, features = ["collector", "builder", "cache"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = "0.4"
uuid = { version = "1", features = ["v4"] }
base64 = "0.22"
dotenvy = "0.15"
dirs = "5"                 # cross-platform data dirs

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = ["Win32_Security_Cryptography", "Win32_Storage_FileSystem"] }
# optional later: TLS impersonation escalation
# webclaw_http = { git = "https://github.com/oh0123/webclaw-tls", ... }  # or impersonate-rs
```

Note: `discord-user-rs` feature `cli` (clap+rusqlite+serde_yaml) exists but we keep the library lean and build our own CLI for full control.

## 6. SQLite Schema

Modeled on langkurt (MIT, verbatim-portable) + jackwener (Apache-2.0). Verified from source:

```sql
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;
-- rusqlite: one connection, SetMaxOpenConns-equivalent = single writer

CREATE TABLE guilds (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, icon TEXT
);
CREATE TABLE channels (
  id TEXT PRIMARY KEY, guild_id TEXT REFERENCES guilds(id),
  name TEXT NOT NULL, type INTEGER NOT NULL DEFAULT 0,
  topic TEXT, parent_id TEXT
);
CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  channel_id TEXT NOT NULL REFERENCES channels(id),
  guild_id TEXT,
  author_id TEXT NOT NULL, author_name TEXT NOT NULL,
  content TEXT NOT NULL,
  timestamp DATETIME NOT NULL,          -- UTC RFC3339 string
  edited INTEGER NOT NULL DEFAULT 0,
  reaction_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_messages_channel ON messages(channel_id);
CREATE INDEX idx_messages_timestamp ON messages(timestamp DESC);
CREATE INDEX idx_messages_author ON messages(author_id);
CREATE INDEX idx_messages_reactions ON messages(reaction_count DESC);

CREATE TABLE sync_state (
  channel_id TEXT PRIMARY KEY,
  last_message_id TEXT,      -- forward cursor (newest seen)
  oldest_message_id TEXT,    -- backward cursor (oldest seen)
  synced_at DATETIME
);

-- FTS5 external-content full-text search:
CREATE VIRTUAL TABLE messages_fts USING fts5(
  content, author_name,
  content='messages', content_rowid='rowid',
  tokenize="unicode61 remove_diacritics 1"
);
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content, author_name) VALUES (new.rowid, new.content, new.author_name);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content, author_name) VALUES('delete', old.rowid, old.content, old.author_name);
END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content, author_name) VALUES('delete', old.rowid, old.content, old.author_name);
  INSERT INTO messages_fts(rowid, content, author_name) VALUES (new.rowid, new.content, new.author_name);
END;

-- Optional (attachment download feature, from langkurt):
CREATE TABLE attachments (
  id TEXT PRIMARY KEY, message_id TEXT NOT NULL REFERENCES messages(id),
  channel_id TEXT NOT NULL, url TEXT NOT NULL, filename TEXT NOT NULL,
  content_type TEXT, size INTEGER, local_path TEXT
);
CREATE TABLE links (
  id TEXT PRIMARY KEY, message_id TEXT NOT NULL REFERENCES messages(id),
  channel_id TEXT NOT NULL, url TEXT NOT NULL, local_path TEXT,
  failed INTEGER NOT NULL DEFAULT 0, fail_reason TEXT, proxy_url TEXT
);
```

**Key correctness details (from langkurt, port as-is):**
- **Search query binding:** FTS5 `MATCH ?` takes the raw query verbatim — do NOT escape/interpolate (FTS5 query syntax is the feature). Search SQL:
  ```sql
  SELECT m.id, m.channel_id, c.name AS channel_name,
         COALESCE(g.name,'DM') AS guild_name, m.author_name, m.content,
         m.timestamp, rank
  FROM messages_fts
  JOIN messages m ON messages_fts.rowid = m.rowid
  JOIN channels c ON m.channel_id = c.id
  LEFT JOIN guilds g ON m.guild_id = g.id
  WHERE messages_fts MATCH ?
    [AND c.name = ?] [AND g.name = ?]
  ORDER BY rank LIMIT ?;
  ```
- **Guild/Message upsert:** `INSERT OR REPLACE` (message upserts reaction_count). **Attachment upsert:** `INSERT OR IGNORE` (keep existing local_path).
- **sync_state upsert** (resumable):
  ```sql
  INSERT INTO sync_state(channel_id, last_message_id, oldest_message_id, synced_at)
  VALUES (?,?,?,?)
  ON CONFLICT(channel_id) DO UPDATE SET
    last_message_id = CASE WHEN excluded.last_message_id > last_message_id
                           THEN excluded.last_message_id ELSE last_message_id END,
    oldest_message_id = CASE WHEN oldest_message_id='' OR excluded.oldest_message_id < oldest_message_id
                             THEN excluded.oldest_message_id ELSE oldest_message_id END,
    synced_at = excluded.synced_at;
  ```
- Snowflake comparisons are **string** comparisons (safe).
- `guild_id` can be NULL for DMs → display with `COALESCE(g.name,'DM')`, `COALESCE(c.name, m.channel_id)`.
- `isSnowflake(s)` = length 17–20, all digits.

## 7. Stealth / Anti-Detection Layer

Ported from discordo's approach (concepts re-implemented in Rust; discordo is GPL — do not copy verbatim). `client.rs` + `stealth.rs`.

**Key simplification (research re-run):** `discord-user-rs`'s `DiscordHttpClient` ALREADY sets Chrome UA, `X-Discord-Locale`, `X-Discord-Timezone`, and has `set_super_properties_b64()`. So the Rust port does NOT need to re-implement the full header set — it needs to (1) generate the X-Super-Properties JSON (with launch_signature mask + fresh client_heartbeat_session_id), (2) base64 it, (3) call `set_super_properties_b64()` on the client.

**Note on the rust-branch reality check:** mrarfarf's stealth layer is ALSO header-level only (no JA3/JA4 TLS fingerprint spoofing — Go `net/http` default TLS). So header-level impersonation (UA + X-Super-Properties + sec-fetch-* + per-install device_id) is what real selfbot repos actually ship. **Full TLS fingerprint spoofing** (Chrome_146 JA4/Akamai) is an optional escalation (reqwest-impersonate / cpr / impersonate-rs / webclaw-tls) gated behind a feature flag — verify against `tls.peet.ws` before enabling; Discord may not need it.

**Every REST request headers:**
```
User-Agent:      Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36
Accept:          */*
Accept-Encoding: gzip, deflate, br, zstd
Accept-Language: en-US,en;q=0.9
Origin:          https://discord.com/api/v10
Referer:         https://discord.com/channels/@me
Priority:        u=1, i
Sec-Fetch-Dest:  empty
Sec-Fetch-Mode:  cors
Sec-Fetch-Site:  same-origin
X-Debug-Options: bugReporterEnabled
X-Discord-Locale: en-US
X-Super-Properties: base64(JSON)
Authorization:   <raw user token>   (NO "Bot " prefix)
```

**X-Super-Properties JSON payload** (base64-encoded):
```json
{
  "os": "Windows",
  "os_version": "10",
  "client_build_number": <scraped>,
  "client_launch_id": "<uuid>",
  "client_app_state": "focused",
  "client_heartbeat_session_id": "<uuid>",
  "launch_signature": "<uuid-with-detection-bit-mask>",
  "has_client_mods": false,
  "release_channel": "stable",
  "browser": "Chrome",
  "browser_user_agent": "<UA>",
  "browser_version": "146.0.0.0",
  "system_locale": "en-US"
}
```
`launch_signature` uses a 16-byte bitmask that clears Discord's client-mod detection bits. Exact mask (from discordo, re-implement in Rust):
```rust
const LAUNCH_SIGNATURE_MASK: [u8; 16] = [
    0b11111111, 0b01111111, 0b11101111, 0b11101111,
    0b11110111, 0b11101111, 0b11110111, 0b11111111,
    0b11011111, 0b01111110, 0b11111111, 0b10111111,
    0b11111110, 0b11111111, 0b11110111, 0b11111111,
];
// let uuid = Uuid::new_v4(); let bytes = uuid.as_bytes();
// for i in 0..16 { bytes[i] &= LAUNCH_SIGNATURE_MASK[i]; }
// format as canonical lowercase 8-4-4-4-12 hex
```
`client_heartbeat_session_id` is a fresh UUIDv4 **per request** (not cached). `client_launch_id` is a plain UUIDv4.

**Build number** (`stealth.rs`) — two verified approaches:
- **Scraper** (discordo main): fetch `https://discord.com/login`, regex `"BUILD_NUMBER":"(\d+)"` (or `"BUILD_NUMBER":\s*"(\d+)"`), cache in `OnceLock`, fallback to pinned constant. Value changes ~weekly.
- **Hard-coded** (mrarfarf rust branch): `client_build_number = 482285` (Chrome 143) or discordo main's `584177` (Chrome 146) — simpler, must bump manually.
- Recommendation: scraper with a pinned fallback (best of both).

**Gateway identify properties** (for live tail) — exact IDENTIFY op-2 payload properties (discordo/arikawa):
```json
{
  "os": "Windows", "os_version": "10",
  "browser": "Chrome", "browser_version": "146.0.0.0",
  "browser_user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
  "device": "",
  "client_build_number": 584177,
  "client_event_source": null,
  "client_launch_id": "<uuid v4>",
  "system_locale": "en-US", "release_channel": "stable",
  "has_client_mods": false,
  "referrer": "", "referrer_current": "",
  "referring_domain": "", "referring_domain_current": "",
  "is_fast_connect": true
}
```
Notes: `launch_signature`/`client_app_state`/`client_heartbeat_session_id` are X-Super-Properties-only, NOT in identify. `is_fast_connect` only in identify. For user accounts `intents` must be nil/omitted. Gateway URL: `wss://gateway.discord.gg?v=9&encoding=json` (or v10 — see §18 API-version decision).

**TLS:** default = hardened reqwest (`http1_only()`, Chrome UA, browser headers). Optional feature `tls-impersonation` gates `webclaw-tls` / `impersonate-rs` behind a `[patch.crates-io]` or FFI — evaluated in milestone 8.

## 8. Authentication

**Token resolution order** (`config.rs`):
1. `--token <T>` CLI flag
2. `DISCORD_TOKEN` env var
3. `./.env` (`DISCORD_TOKEN=...`) — via `dotenvy`
4. Auto-detect from local Discord/browser session (LevelDB scan)
5. Interactive paste prompt (if TTY) — fallback for non-Windows or when detection fails

**`auth` command:**
- `auth --save` — auto-detect token, validate against `GET /users/@me`, write/upsert into `./.env`.
- `auth --paste` — prompt to paste token manually, validate, save.
- Validation: `GET /users/@me` returns 200 with a valid profile → token good; exit 1 on failure.

**Auto-detect (Windows, from jackwener/discord-cli-rs):**
- Search paths: `%APPDATA%\discord\Local Storage\leveldb`, `discordptb`, `discordcanary`, `%LOCALAPPDATA%\Google\Chrome\User Data\Default\Local Storage\leveldb`, Brave, Edge.
- Token regex: `[\w-]{24,}\.[\w-]{6}\.[\w-]{27,}` and `mfa\.[\w-]{84}`.
- Discord desktop stores tokens DPAPI-encrypted in LevelDB; Chrome/Edge on Windows store them AES-GCM-encrypted with the DPAPI-wrapped key in `Local State`. Windows module decrypts via DPAPI (`CryptUnprotectData`). (This mirrors `discord-cli-rs/src/auth/windows.rs`.)

**Storage:** prefer OS keyring when available (`keyring` crate), fallback to `.env`. Provide `--no-keyring` to force `.env`.

## 9. Command Surface

### Top-level (config/auth)
| Command | Description |
|---|---|
| `auth [--save] [--paste]` | Auto-detect token, or paste manually; validate; save to `.env`/keyring |
| `status` | Validate token, exit 1 on failure |
| `whoami [--json]` | Show current user profile |

### Discord operations (`discord dc ...`)
| Command | Description |
|---|---|
| `dc guilds [--json]` | List joined guilds (name/id/icon/owner) |
| `dc channels <GUILD> [--json]` | List text/announcement/forum channels (type 0/5/15), sorted by position |
| `dc dms [--json]` | List DM + group-DM channels (`GET /users/@me/channels`) |
| `dc history <CHANNEL> [-n 1000] [--before/--after]` | Fetch message history (REST, paginated 100) |
| `dc read <CHANNEL> [-n 50] [--before id] [--json]` | Read recent messages (default 50, cursor-able) — the key AI-facing read |
| `dc send <CHANNEL> --text "..." [--file PATH] [--reply ID]` | Send message, optional attachment / reply |
| `dc edit <CHANNEL> <MSG_ID> --text "..."` | Edit own message |
| `dc delete <CHANNEL> <MSG_ID>` | Delete own message (require `--confirm`) |
| `dc sync <CHANNEL> [-n 5000]` | Incremental sync to SQLite (two-phase: forward past newest, backward past oldest) |
| `dc sync-all [-n 5000] [--since 30d]` | Discover accessible channels and sync each (bounded) |
| `dc tail <CHANNEL> [--once] [--json]` | Follow new messages (REST poll or gateway) |
| `dc search <GUILD> <QUERY> [-c CH] [--json]` | Discord native search |
| `dc members <GUILD> [--max 50] [--json]` | List guild members |
| `dc info <GUILD> [--json]` | Show guild info |
| `dc threads <CHANNEL> [--json]` | List active/archived threads (user-token fallback `GET /channels/{id}/threads/search`) |
| `dc roles <GUILD> [--json]` | List roles |
| `dc react <CHANNEL> <MSG_ID> <EMOJI>` | Add reaction |
| `dc unreact <CHANNEL> <MSG_ID> <EMOJI>` | Remove own reaction |
| `dc pins <CHANNEL> [--json]` | List pinned messages |

### Local query (offline SQLite)
| Command | Description |
|---|---|
| `search <KEYWORD> [-c CH] [-n 50] [--json]` | FTS5 full-text search of local archive |
| `recent [-c CH] [--hours N] [-n 50] [--json]` | Newest stored messages |
| `stats [--json]` | Per-channel message counts |
| `today [-c CH] [--json]` | Today's messages |
| `top [-c CH] [--hours N] [--json]` | Top senders |
| `timeline [-c CH] [--hours N] [--json]` | Activity timeline |
| `export <CHANNEL> [-f text|json] [-o FILE]` | Export stored messages |
| `purge <CHANNEL> [-y]` | Delete stored messages for a channel (destructive, requires `--confirm`) |

### MCP server
`discord serve` — starts the `rmcp` stdio MCP server (see §11).

## 10. Output Contract

Mimics the best from jackwener SCHEMA.md + famasya JSONL + discli safety.

- **Default when stdout is a pipe (AI):** **JSONL** — one JSON object per line. Global `--json` forces JSON array; `--yaml` forces YAML.
- **Envelope** for `--json`/`--yaml`:
  ```json
  { "ok": true,  "schema_version": "1", "data": [...] }
  { "ok": false, "schema_version": "1", "error": { "code": "NotFound", "message": "..." } }
  ```
- **Errors → stderr**, data → stdout.
- **Exit codes:** `0` ok, `1` error, `2` usage, `3` not found, `4` forbidden/ratelimited, `5` network.
- **Safety:** destructive commands (`delete`, `purge`) require `--confirm`; `--dry-run` prints what would happen.

## 11. MCP Server Tools

Expose as `#[tool]` functions via `rmcp`, returning **JSON-serialized** data (NOT plaintext — fixing the langkurt gap). Input/Output schema generated with `schemars`.

| Tool | Params | Purpose |
|---|---|---|
| `list_guilds` | — | All guilds the user belongs to |
| `list_channels` | `guild_id` | Text channels of a guild |
| `list_dms` | — | DM + group-DM channels (`/users/@me/channels`) |
| `read_messages` | `channel_id`, `limit?`, `before?` | Direct REST read (the missing langkurt tool) |
| `read_dm` | `channel_id`, `limit?`, `before?` | Read DM channel messages |
| `send_message` | `channel_id`, `content`, `reply_to?`, `file_path?` | Send (gated — not auto-approved) |
| `search_messages` | `query`, `guild_id?`, `channel_id?`, `limit?` | Native or local search |
| `get_message` | `channel_id`, `message_id` | Single message |
| `sync_channel` | `channel_id`, `limit?` | Incremental sync (bounded, streaming progress) |
| `get_sync_status` | — | Per-channel sync counts |
| `list_members` | `guild_id`, `limit?` | Guild members |
| `list_threads` | `channel_id` | Threads (user fallback endpoint) |

**Sync (`sync_channel`) two-phase logic** (from langkurt, port to Rust):
- **Phase A (new, forward):** only if `state.last_message_id != ""`; `after = last_message_id`; loop `GET /channels/{id}/messages?after=...&limit=100`; upsert each; advance cursor to newest seen; `sleep(500ms)`.
- **Phase B (backward, history):** `before = state.oldest_message_id` (resume); paginate backward with `FetchMessages` (400ms base + jitter, 429 backoff 2s→10s cap); optional `stop_before` cutoff for `--since`.
- **Persist:** `last = max(newest, state.last_message_id)`, `oldest = min(oldest, state.oldest_message_id)`.

**User-token pitfalls to preserve (from langkurt):**
1. `GET /channels/{id}/threads` → **403 for user tokens** (bot-only) → fallback `GET /channels/{id}/threads/search?limit=25&sort_by=last_message_time&sort_order=desc&archived=false&offset=N` (what Discord's own app uses), paginate `offset += len(threads)` until `!has_more`.
2. Forum (type 15) / media (16) channels are **containers** — messages live in threads; fetch threads (active + archived via `ArchiveTimestamp` cursor) first.
3. `guild_id` NULL for DMs → `COALESCE(g.name,'DM')`.
4. MCP handlers in langkurt return **plaintext** (strings.Builder) — this plan fixes that: return **JSON** so agents can parse reliably.

**Wiring for Claude Code:** README documents:
```bash
claude mcp add discord --env DISCORD_TOKEN=... -- <path-to-binary> serve
```
Some tools (send, sync) stay out of auto-approve (like langkurt's design).

## 12. Rate Limiting & Safety

**Fetch loop (from langkurt's `FetchMessages`, port to Rust):**
- `base_delay = 400ms`, `jitter = 0..300ms` random per iteration
- On `429` (RESTError): `sleep(backoff)` with `backoff *= 2` starting at `2s`, **cap 10s**; retry indefinitely
- On success: reset `backoff = 2s`
- Page size: 100 (`ChannelMessages(100, beforeID, ...)`)
- Snowflake cursor comparisons are **string** comparisons (safe — fixed-length decimal)

**Additional (from jackwener):**
- `429` → sleep `retry_after + random(0.5, 2.0)`
- `X-RateLimit-Remaining == 0` → sleep `X-RateLimit-Reset-After + random(0.2, 1.0)`
- Max 3 retries per request (jackwener), or infinite-with-backoff (langkurt) — pick: infinite-with-backoff for sync loops, bounded for one-shot commands

**Cloudflare-1015 protection (from `discord-user-rs`'s `InvalidRequestTracker`):**
- Track 401/403/429 in a 10-min sliding window
- Warn at ~7k, hard-stop at ~9.5k invalid responses (Cloudflare IP ban imminent)

**Safety defaults:**
- Bounded reads: `sync-all` defaults to a cap (e.g. 200/channel) unless overridden; `--since` filters
- Default gateway presence `UserStatus::Invisible`
- `--confirm` required for destructive ops; `--dry-run` previews

## 13. Error Handling & Exit Codes

- `DiscordError` (thiserror) with variants mapping to exit codes:
  - `NotFound` → 3
  - `Forbidden`/`Ratelimited` → 4
  - `Network`/`Timeout` → 5
  - `Usage` (clap) → 2
  - everything else → 1
- Errors always printed to stderr; `--json`/`--yaml`/JSONL modes emit the error envelope to stdout too.

## 14. Implementation Order (Milestones)

1. **Scaffold** — `cargo init`, deps (incl. `discord-user-rs`), `config.rs` (token resolution env→`.env`→paste→keyring), `output.rs` (JSONL/JSON/YAML + envelope + exit codes 0/1/2/3/4/5 + isTTY detection + progress→stderr/data→stdout), `status`/`whoami`. Validate token against `GET /users/@me`.
2. **Read path** — `dc guilds`, `dc channels`, `dc history`, `dc read`, `dc dms`; `client.rs` wrapping `DiscordHttpClient` + browser headers + `set_super_properties_b64` + rate limit. Use `Route` enum from crate. Manual smoke test with a real token.
3. **Send path** — `dc send`/`edit`/`delete`/`react`/`pin` (+ `--confirm`, `--dry-run` structured record), file attachment (multiple, like discord-user-rs).
4. **SQLite + search** — schema, `sync`, `sync-all`, `search`/`recent`/`stats`/`today`/`top`/`timeline`/`export`/`purge`, FTS5 triggers. Search = fetch-then-filter (famasya post-filter pagination) + local FTS5 offline (langkurt SQL, bind verbatim).
5. **Auth auto-detect** — Windows DPAPI+LevelDB (and Linux/macOS best-effort) → `auth --save`; per-install device_id.
6. **MCP server** — `serve` subcommand with rmcp, 12 tools, **JSON** results (fix langkurt plaintext gap), Claude Code wiring.
7. **Gateway tail** — `dc tail` via `DiscordUser` (invisible — never empty status), live `read` subscription; dedup bounded FIFO seen-set.
8. **Stealth hardening** — build-number scraper (with pinned fallback), launch_signature mask, X-Super-Properties via `set_super_properties_b64`, optional TLS-impersonation feature-gate.
9. **Tests + docs** — trait-based fake-API tests (famasya pattern) asserting cursor walks; README with ToS warning, output contract, MCP wiring, sample agent flows.

## 15. Files to Create

- `Cargo.toml`, `src/main.rs`, `src/config.rs`, `src/output.rs`, `src/db.rs`, `src/types.rs`, `src/client.rs`, `src/stealth.rs`, `src/mcp.rs`
- `src/auth/mod.rs`, `src/auth/detect.rs`, `src/auth/windows.rs`, `src/auth/linux.rs`, `src/auth/macos.rs`
- `src/commands/` — one file per verb (e.g. `dc_guilds.rs`, `dc_read.rs`, `dc_send.rs`, `dc_sync.rs`, `dc_tail.rs`, `dc_search.rs`, `dc_dms.rs`, `search.rs`, `export.rs`)
- `tests/integration.rs`, `tests/mcp_tools.rs`, `README.md`

## 16. References & Sources

**In-repo (`.tmp/`, user OK'd copying/porting):**
- `jackwener-discord-cli` (Apache-2.0) — REST endpoints, pagination, SQLite schema, output envelope, tail-polling, LevelDB token extraction → primary template for read/sync/search/tail
- `langkurt-discord-cli` (MIT) — MCP tool set, two-phase sync, user-token thread fallback, rate-limit jitter/backoff → primary template for MCP + sync
- `famasya-discord-cli-agent` (MIT) — JSONL + exit-code model
- `mrarfarf-discord-cli` (BSD/MIT-style) — keyring storage, QR-remote-auth reference
- `ibbybuilds-discli` (MIT) — noun-verb grammar, `--confirm`/`--dry-run`, name→ID resolution, output auto-detect
- `ayn2op-discordo` (GPL-3.0) — stealth headers, X-Super-Properties + launch_signature mask, identify props, build-number scraper (adapt concepts, re-implement in Rust)

**Crates:** `discord-user-rs`, `discord-cli-rs`, `rmcp` (+ docs), `webclaw-tls`, `impersonate-rs`, `hyprcurl`, `reqwest-impersonate` / `cpr` (TLS impersonation ports of curl-impersonate for Rust).

**Discord API docs:** Gateway lifecycle (Hello/heartbeat/IDENTIFY/RESUME/Invalid Session), thread permissions, REST message endpoints.

## 17. Verification Plan

1. `cargo build --release` → single binary.
2. **Unit/integration tests:** mock the HTTP layer (reqwest mock or a fake `DiscordHttpClient`) to test rate limiting, pagination, SQLite dedup, FTS5 search, output envelope, exit codes — no real account needed.
3. **Manual smoke (real token, low volume):** `auth --save` → `status` → `whoami` → `dc guilds` → `dc channels <guild>` → `dc dms` → `dc read <channel>` → `dc send --confirm`.
4. **MCP wiring test:** `claude mcp add discord ...` then ask Claude Code to "list my servers, read the last 10 messages in #general, and reply" — verify each tool round-trips.
5. **Safety check:** run `sync-all` bounded; observe rate-limit sleeps; confirm no 429 floods; verify `--confirm` blocks destructive ops without a flag.
6. **Search test:** after sync, `search <keyword>` returns FTS5 hits; `--json` emits the envelope.

## 18. Distinctive techniques to port (research re-run, from all repos)

**Search (no server-side search exists for selfbots — always fetch-then-filter):**
- Escape-Tech channel-scan: walk every text channel 100/page, cursor = `batch.last().id`; hard caps `--limit` (global), `--max-pages`/channel (default 10), `batch.size===0`; no-permission channel → `catch { break }`; date short-circuit kills channel early. Relative date `Nd` → now−N days.
- famasya post-filter pagination: fetch 100/batch → filter in-memory (--search substring on content/thread-name, --since/--until) → **page counts matched rows not fetched** (`start=(page-1)*size`). Early-break when oldest crosses --since. Validation: --around xor --before/--after; --since ≤ --until.

**--before/--after two DIFFERENT semantics (don't conflate):** `read` = message-ID cursors; `search` = DATE cursors.

**Output idioms (combine all three):**
- isTTY detection: piped → YAML/JSONL; TTY → pretty table/human. `--json`/`--yaml`/`--markdown`/`--format` overrides.
- famasya JSONL default: `serde_json` per-row encode, `SetEscapeHTML(false)` equivalent → `serialize_with` raw, one object per line.
- Progress → **stderr**, data → **stdout** (Escape-Tech clears with `\r\x1b[K`).

**Name→ID resolution (discli, port exactly):** ID-match first → strip `#`/`@` prefix → case-insensitive → ambiguity → stderr list + exit; not-found → exit 3. resolveMember: ID attempt → search username OR global_name OR nick.

**Exit codes:** converge on discli {0,1,2,3,4} + famasya {0,2,3,4,5,6,7}: keep usage(2)/not-found(3)/forbidden(4) split; 429 → "Rate limited. Retry after Xs." exit; 401 → auth exit.

**Destructive safety (discli):** `--confirm` never interactive; `--dry-run` prints structured record `{action, name, ...}`.

**Stealth per-install uniqueness (mrarfarf):** device_id = `discord-cli-{instanceID}` (random 3-hex persisted in config) — makes each install look distinct. Port it.

**Invisible presence trap (mrarfarf):** empty status string renders "online" — never send empty; default `UserStatus::Invisible`.

**Gateway event dedup (mrarfarf):** bounded FIFO seen-set (max 10k), cheap `markSeen` pre-check BEFORE building content string; `historicalOnce` guard on onReady (re-fires on resume); `isAuthError` matches REST 401 OR gateway close 4004 (not error strings) → auto re-auth.

**Permission bitfield (discli):** `perm = 1 << n` consts table; `PermissionOverwrite {id, type(0=role/1=member), allow, deny}`.

**DMs (Stone-Red-Code + sinjs + mrarfarf):** `client.PrivateChannels` cached; DM label = `user#disc`, GroupDM = join of recipient tags; permission gate ViewChannel.

**Mention resolution (Stone-Red-Code):** regex `<@(.*?)>`/`<#(.*?)>`/`<@&(.*?)>`; resolve via message mention lists, fallback `GetUserAsync(id)`/`GetChannelAsync(id)`; render `@user#disc`/`#channel`/`@role`.

**Two user-token auth paths:** QR remote-auth (mrarfarf — modern, works with/without MFA) AND email+password+TOTP (sinjs: `POST /auth/login`, `POST /auth/mfa/totp`).

**Forum post (famasya):** `ForumThreadStartComplex` = thread + first message in one call; returns `https://discord.com/channels/{guildID}/{threadID}`.

**Test layering (famasya):** client depends on an `API` trait; fakeAPI implements it; `NewClientWithAPI(api)` injection; tests record every call's cursor args and assert the exact pagination walk. Port as Rust trait + mock.

## 19. Open Items / Future Work

- **API version decision:** discordo/arikawa uses **v9** (`https://discord.com/api/v9/`), jackwener uses **v10**. Rust port must pick ONE for consistency. Recommendation: **v10** for REST (jackwener + `discord-user-rs` both use v10; it's the current stable) and **v10** for gateway (`?v=10&encoding=json`). Keep v9 only if a specific endpoint requires it (remote-auth login is v9 in discordo — verify against current docs). `discord-user-rs` already defaults to v10 for both HTTP and gateway.
- **`ayn2op-discordo` rust branch** (`.tmp/branches/ayn2op-discordo/rust`) — a 1-commit "rust rewrite" SPIKE, ~1% maturity: 6 tiny Rust files (blank TUI chat, inert login form, empty Config, Elm `tea` runtime ~150 lines), **NO stealth layer, NO HTTP, NO gateway, NO QR auth in Rust**. The Discord code lives in a **stale Go snapshot** (no go.mod → doesn't build) that is OLDER than main. GPL-3.0. **Not a viable base.** If copying stealth logic, source from **MAIN** (`internal/http/properties.go`, `internal/tls/profile.go` Chrome_146, `internal/http/generator.go`) — newer, includes real TLS impersonation (bogdanfinn/tls-client) + build-number scrape. Confirms naming: `KEYRING_SERVICE="discordo"`, `KEYRING_USER="token"`, `TOKEN_ENV_VAR_KEY="DISCORDO_TOKEN"`.
- **TLS impersonation** (optional `tls-impersonation` feature) — verify against `tls.peet.ws` before enabling; Discord may not need it today. Rust options: `reqwest-impersonate` / `cpr` / `impersonate-rs` (curl-impersonate FFI) / `webclaw-tls`. The exact Chrome 146 TLS profile bytes live in `bogdanfinn/utls` (Go) — replicating them in Rust is the hardest part of the port.
- **QR remote-auth** (from mrarfarf/discordo) as a no-password path to a fresh token — advanced auth option. Exact flow (verified from source, both repos agree):
  1. Connect `wss://remote-auth-gateway.discord.gg/?v=2` with Chrome UA + `Origin: https://discord.com`.
  2. On `hello` (fields `heartbeat_interval` ms, `timeout_ms`), start heartbeat loop sending `{"op":"heartbeat"}` every interval.
  3. Generate RSA-2048 keypair; send `{"op":"init","encoded_public_key":<base64 STD of x509.MarshalPKIXPublicKey(pub)>}` (SPKI DER).
  4. On `nonce_proof` (`encrypted_nonce`): base64 decode → `rsa.DecryptOAEP(sha256.New(), nil, key, nonce, nil)` → re-encode **base64 RawURL (no padding)** → send `{"op":"nonce_proof","nonce":...}`.
  5. On `pending_remote_init` (`fingerprint`): render QR with content `https://discord.com/ra/<fingerprint>` (half-block chars █▀▄).
  6. On `pending_ticket` (`encrypted_user_payload`): RSA-OAEP decrypt → `split(":")` → [1]=discriminator, [3]=username (display only).
  7. On `pending_login` (`ticket`): POST `https://discord.com/api/v9/users/@me/remote-auth/login` body `{"ticket":...}` with headers `X-Fingerprint:<fp>` + `Referer: https://discord.com/login` (or `/ra/{fp}`) → response `encrypted_token` → RSA-OAEP decrypt → the user token.
  - **WS concurrency:** gorilla forbids concurrent writers → heartbeat goroutine + main loop share a mutex around writes (port with `tokio::sync::Mutex`).
  - **Rust crates:** `rsa` (OAEP-SHA256), `x509-cert`/`der` (SPKI), `qrcode`, `tokio-tungstenite`.
- **`dc watch`** — long-running gateway subscription that streams new messages as JSONL to an agent pipe.
- **Attachment download** (from langkurt) — `--download` on sync.
- **GPL consideration** — if the user prefers verbatim porting of discordo's stealth code, the project must adopt GPL-3.0.
- **Agent prompt / SKILL.md** — ship a Claude Code skill file so the agent "knows" the tool's conventions.
- **`--dry-run` and `--confirm` audit** for every destructive command.
