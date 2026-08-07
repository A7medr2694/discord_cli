# AGENTS.md — Guidance for AI agents driving discord-cli

This file helps AI agents (Claude Code, etc.) use `discord-cli` correctly.

## What this is

A Rust CLI + MCP server that operates a **user** Discord account (self-bot style) on the user's behalf. It reads/sends/searches across every server, group, and DM the account belongs to.

## ⚠️ Non-negotiable

- **ToS risk**: user-token automation can get the account banned. Never bulk-crawl, never `sync-all` unbounded, always respect rate limits. `auth --qr` uses Discord's login API (highest risk) — opt-in only, never automatic.
- **Destructive actions require `--confirm`** — the agent must not bypass this.
- **Never print or log the token.**
- **Stealth**: real-client headers + X-Super-Properties + masked launch_signature + per-install device_id are active; TLS ClientHello (JA3) spoofing is NOT yet implemented (rustls).

## Command conventions

- Top-level verbs: `status`, `whoami`, `auth`, `serve`, plus local queries `search` / `recent` / `stats` / `top` / `export` / `purge`.
- `dc` group: `guilds`, `channels`, `dms`, `history`, `read`, `send`, `edit`, `delete`, `react`, `unreact`, `pin`, `pins`, `members`, `info`, `search`, `roles`, `profile`, `relationships`, `threads`, `sync`, `sync-all`, `tail`, `watch`, `dm-group`, `notify`.
- **Output**: JSONL when piped, `--json` for a single envelope `{ok, schema_version, data|error}`.
- **Exit codes**: `0` ok, `1` error, `2` usage (missing `--confirm`), `3` not found, `4` forbidden, `5` network, `7` attachment/file IO error.
- **Errors → stderr**, data → stdout.

## Agent playbook (common flows)

### "Summarize a channel"
```
discord read <CHANNEL> -l 200 --json   # fetch recent messages
discord sync <CHANNEL> -l 2000         # archive for offline search
discord search "deploy" -n 50            # find mentions offline
```
Then synthesize the JSON into a summary. Prefer `--json` output (machine-readable).

### "What can I see?"
```
discord guilds --json
discord channels <GUILD> --json
discord dms --json
```

### "Reply to someone"
```
discord send <CHANNEL> --text "..." --reply <MSG_ID>   # reply is auto-approved
discord send <CHANNEL> --text "..." --confirm          # new message needs --confirm
discord send <CHANNEL> --text "log attached" --file ./build.log --confirm   # send with attachment
discord send <CHANNEL> --text - --file ./report.pdf --confirm       # --text - reads stdin
```

### "Watch for keywords"
```
discord watch --keyword "bug" --jsonl    # stream matching messages as JSONL
discord watch --typing --jsonl               # also emit typing events as JSONL
discord typing <CHANNEL>                     # send a typing indicator
discord join "https://discord.gg/abc123" --confirm   # join a server via invite
discord leave "old-server" --confirm               # leave a server
discord presence dnd                                # set presence (online|idle|dnd|invisible)
```

## MCP mode

`discord serve` exposes 11 tools over stdio. `send_message` is intentionally **not** auto-approved in the MCP client — the agent should request approval.

## Style

- Reference the user by name only if known; never assume gender.
- When in doubt about a write action, use `--dry-run` first.
