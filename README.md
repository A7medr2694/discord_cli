# discord-cli

**A Rust Discord CLI + MCP server for AI agents** — read, send, search, and manage Discord as the **logged-in user** (user-token / self-bot style), so it works in *every* server, group, and DM you belong to — no bot invitation needed.

> ⚠️ **ToS / account-risk warning**
>
> Automating a user account violates Discord's Terms of Service and can result in **account termination**. This tool is for accounts you control, on machines you control. Use it with restraint: rate limits are built in, reads are bounded, and destructive actions require explicit confirmation.

---

## Why not a bot?

A Discord **bot** can only read channels it is invited to. A **user token** reads *everything* the account can see — every server you've joined, every DM, every thread. That's the core requirement for an AI agent managing your account on your behalf.

## Features

- **Read anything**: `dc guilds`, `dc channels`, `dc dms` (DM + group-DM), `dc history`, `dc read`, `dc members`, `dc info`, `dc search` (native), `dc threads` (with user-token fallback), `dc roles`, `dc profile`, `dc relationships`, `dc pins`
- **Send / act**: `dc send` (with `--confirm` safety), `dc edit`, `dc delete`, `dc react`, `dc pin`, `dc dm-group` (create/add/remove), `dc notify`
- **Archive offline**: `dc sync` / `dc sync-all` → SQLite + FTS5 full-text search, then `search`, `recent`, `stats`, `top`, `export`, `purge`
- **Live**: `dc tail` / `dc watch` — JSONL streams over the gateway (invisible presence)
- **AI-ready**: `serve` starts an **MCP server** (stdio) exposing 11 tools; all commands emit **JSONL/JSON/YAML** with a stable envelope
- **Stealth**: browser UA + `X-Super-Properties` + `launch_signature` mask + per-install `device_id`

## Install

```bash
cargo build --release
# binary: target/release/discord(.exe)
```

Requires Rust 1.80+. The workspace uses `[patch]`-free deps and `tokio_unstable` cfg for `discord-user-rs` (see `.cargo/config.toml`).

## Quick start

```bash
# 1. Authenticate (auto-detect from local Discord/browser, or paste)
discord auth --save
# or: discord auth --paste --save

# 2. Verify
discord status
discord whoami

# 3. Explore
discord dc guilds
discord dc channels <GUILD>
discord dc dms

# 4. Read a channel (agent-facing)
discord dc read <CHANNEL> -n 50 --json

# 5. Archive + search offline
discord dc sync-all -n 200
discord search "keyword"
discord recent --hours 24
discord stats
discord top

# 6. Live follow
discord dc watch --channel <ID> --keyword "deploy"
```

## Output contract

- **Piped stdout → JSONL** (one object per line); TTY → human.
- `--json` / `--yaml` force a single envelope:
  ```json
  { "ok": true, "schema_version": "1", "data": [...] }
  { "ok": false, "schema_version": "1", "error": { "code": "NotFound", "message": "..." } }
  ```
- Errors → **stderr**, data → **stdout**.
- **Exit codes**: `0` ok, `1` error, `2` usage (e.g. missing `--confirm`), `3` not found, `4` forbidden, `5` network.

## MCP server (for AI agents)

```bash
# Add to Claude Code
claude mcp add discord --env DISCORD_TOKEN=$DISCORD_TOKEN -- <abs-path>/discord serve
```

The server exposes 11 tools: `list_guilds`, `list_channels`, `list_dms`, `read_messages`, `read_dm`, `get_message`, `send_message` (gate behind approval), `search_messages`, `list_members`, `list_threads`, `get_sync_status`. All return JSON.

**Example agent flow**: "list my servers → read the last 10 messages in #general → reply."

## Safety defaults

- `--confirm` required for destructive / non-reply sends (never interactive).
- `--dry-run` previews without acting.
- Rate limiting: jitter between pages, `429` backoff (2s→10s), `X-RateLimit-*` honored, Cloudflare-1015 invalid-request budget.
- `sync-all` is bounded per channel (default 200).
- Gateway presence defaults to **Invisible**.
- `purge` only touches the local archive, never Discord.

## Project layout

```
crates/
  discord-core/   client (REST), stealth, config, types, output envelope
  discord-auth/   token auto-detect (LevelDB), paste, keyring, device_id
  discord-db/     SQLite schema, FTS5 search, sync state
  discord-cli/    the `discord` binary + commands
  discord-mcp/    MCP server (rmcp stdio)
scripts/          clone research repos + e2e smoke test
```

## Development

```bash
cargo build         # debug
cargo test          # 44 unit/integration tests
./scripts/e2e.sh    # real-token smoke (DISCORD_TOKEN required)
```

## License

MIT.
