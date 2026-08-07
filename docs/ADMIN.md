# ADMIN.md — Admin / Moderation Commands (F1–F9)

This document covers the **admin/moderation** commands added for managing
servers the logged-in account administers. These ops hit write endpoints that
normally require guild permissions, so they routinely return **403** — the CLI
now maps that to **exit code 4** (FORBIDDEN), live for the first time.

> ⚠️ **ToS / account-risk warning** — Automating a *user* account violates
> Discord's Terms of Service. Admin actions are especially visible to server
> moderators and Discord's automated abuse detection. Use only on servers you
> own/administrate, at LOW volume, and never batch-run destructive commands.
> Every destructive command requires an explicit `--confirm`.

## Permission matrix

Admin actions require the guild permission shown; a 403 → exit 4 when the
account lacks it.

| Feature (F) | Command(s) | Required permission |
|-------------|------------|---------------------|
| Channel CRUD (F1) | `channel-create/rename/topic/move/clone/slowmode/delete` | `MANAGE_CHANNELS` |
| Role CRUD (F2) | `role-create/edit/delete/assign/remove` | `MANAGE_ROLES` |
| Emoji CRUD (F3) | `emoji-list/upload/delete` | `MANAGE_GUILD_EXPRESSIONS` |
| Member moderation (F4) | `member-kick` / `member-ban` / `member-unban` | `KICK_MEMBERS` / `BAN_MEMBERS` |
| Member moderation (F4) | `member-nick` | `MANAGE_NICKNAMES` |
| Permissions (F5) | `perm-set` (overwrite write) | `MANAGE_CHANNELS` |
| Permissions (F5) | `perm-view` / `perm-lock` / `perm-unlock` / `perm-list` | `MANAGE_CHANNELS` (view is read-only) |
| Server settings (F6) | `server-set` / `server-icon` | `MANAGE_GUILD` |
| Audit log (F7) | `audit-log` / `audit-types` | `VIEW_AUDIT_LOG` |
| Invites (F8) | `invite-list` / `invite-create` / `invite-delete` | `MANAGE_CHANNELS` (list/delete) / `CREATE_INSTANT_INVITE` (create) |
| Embed (F9) | `embed` | `SEND_MESSAGES` (same scope as `send`) |

## ToS risk table

| Action | Risk | Notes |
|--------|------|-------|
| `channel-delete` | **Highest** — irreversible, visible to all members | Requires `--confirm`; no recovery |
| `member-kick` / `member-ban` | **Highest** — ban removes a user; visible in audit log | `--confirm` mandatory (non-interactive); reason recorded via `X-Audit-Log-Reason` |
| `channel-create` / `clone` | High — spam detection | Bounded by rate limits; prefer `--dry-run` |
| `role-create/delete` | High — can break permissions | `--confirm` on delete; @everyone guarded |
| `role-assign/remove` | Medium | Verify target member before assigning elevated roles |
| `emoji-upload` | Medium | 256KiB image cap; name alnum+underscore |
| `emoji-delete` | Medium | `--confirm`; managed emojis rejected |
| `perm-lock` / `perm-unlock` | Medium | Lock denies @everyone send; unlock restores — both `--confirm` |

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | OK |
| 1 | Error |
| 2 | Usage / missing `--confirm` / invalid option |
| 3 | Not found |
| 4 | **Forbidden (403)** — admin ops lacking permission |
| 5 | Network |
| 7 | Attachment / file IO error |

## Command reference

### Channels

```
discord channel-create <GUILD> <NAME> [--type T] [--category C] [--topic T] [--slowmode N] [--dry-run]
discord channel-rename <GUILD> <CHANNEL> <NEW_NAME> [--dry-run]
discord channel-topic <GUILD> <CHANNEL> <TOPIC>
discord channel-move <GUILD> <CHANNEL> [--category C] [--position N]     # ≥1 option required
discord channel-clone <GUILD> <CHANNEL> [--name N]
discord channel-slowmode <GUILD> <CHANNEL> <SECONDS>
discord channel-delete <GUILD> <CHANNEL> [--confirm]                       # --confirm required
```

- `--type`: `text|voice|category|announcement|stage|forum` (→ `0|2|4|5|13|15`).
- Channel name: 1–100 chars, no `#`. Topic ≤1024. Slowmode 0–21600.
- Channels are resolved by ID first, then `#name` (case-insensitive exact).
  Ambiguity → exit 2 with a list. Categories via `resolve_category` (type 4).

### Roles

```
discord role-create <GUILD> <NAME> [--color HEX] [--permissions LIST] [--mentionable] [--hoist] [--dry-run]
discord role-edit <GUILD> <ROLE> [--name N] [--color HEX] [--permissions LIST] [--mentionable] [--no-mentionable] [--hoist] [--no-hoist] [--dry-run]
discord role-delete <GUILD> <ROLE> [--confirm]                              # --confirm required
discord role-assign <GUILD> <ROLE> <USER>
discord role-remove <GUILD> <ROLE> <USER>
```

- `--color`: `#RRGGBB` or `RRGGBB`.
- `--permissions`: comma-separated permission names (`send_messages,manage_roles,administrator`).
  Unknown names → exit 2. Case-insensitive.
- `role-edit` requires ≥1 option, else exit 2.
- The `@everyone` role (id == guild id) cannot be created or deleted (exit 2).
- Roles resolved by ID first, then `@name`/`name` (case-insensitive exact).
- Members resolved by bare ID or username/global_name/nick (up to 1000 members).

### Emojis

```
discord emoji-list <GUILD> [--count N]
discord emoji-upload <GUILD> <NAME> <FILE>
discord emoji-delete <GUILD> <EMOJI> [--confirm]                           # --confirm required
```

- Name: alphanumeric + underscore only, 2–32 chars.
- Image: PNG/JPG/GIF, **≤256KiB**. Missing/oversized file → exit 7.
- `--emoji` accepts `:name:`, `name`, or ID. Managed (bot-owned) emojis rejected.

### Members (F4)

```
discord member-kick <GUILD> <USER> [--reason R] [--confirm]        # KICK_MEMBERS; gated
discord member-ban <GUILD> <USER> [--reason R] [--delete-days D] [--confirm]   # BAN_MEMBERS; gated
discord member-unban <GUILD> <USER> [--confirm]                    # BAN_MEMBERS; gated
discord member-nick <GUILD> <USER> <NICKNAME>                      # MANAGE_NICKNAMES; empty clears
```

- Kick reason is sent via the `X-Audit-Log-Reason` header; ban reason in the body.
- `--delete-days` 0–7 (validated; exit 2 out of range) → `delete_message_seconds` (days × 86400, capped 604800).
- `member-unban` takes a **user ID** directly — banned users are not in the member
  list (bare ID or friend username lookup only).
- `member-nick` with an empty nickname clears it.

### Permissions (F5)

```
discord perm-view <GUILD> <CHANNEL>                                # list overwrites (read-only)
discord perm-set <GUILD> <CHANNEL> <ROLE> [--allow A] [--deny D]   # MANAGE_CHANNELS; ≥1 of allow/deny
discord perm-lock <GUILD> <CHANNEL> [--dry-run] [--confirm]        # read-only for @everyone (gated)
discord perm-unlock <GUILD> <CHANNEL> [--confirm]                  # restore @everyone send (gated)
discord perm-list                                                  # name → bit table (local)
```

- `--allow`/`--deny`: comma-separated permission names (`send_messages,manage_roles`).
  Both sides are always transmitted — an absent side is sent as `0` (Discord clears
  a side only when the field is omitted; we never omit).
- `perm-lock` targets the **@everyone** overwrite (overwrite_id == guild_id) and
  denies `SEND_MESSAGES | SEND_MESSAGES_IN_THREADS | CREATE_PUBLIC_THREADS`.
- `perm-unlock` deletes the @everyone overwrite; warns on stderr that it restores
  @everyone send access.
- `perm-view` resolves overwrite targets: kind 0 → role name (`@name`), kind 1 →
  member username (requires `list_roles`/`list_members`).

### Server settings (F6)

```
discord server-set <GUILD> [--name N] [--description D] [--verification V] [--notifications N] [--content-filter C] [--afk-timeout T] [--system-channel ID] [--rules-channel ID] [--dry-run]
discord server-icon <GUILD> <FILE>
```

- Requires **MANAGE_GUILD**; ≥1 option else exit 2. `--dry-run` previews the payload.
- `--verification`: `none|low|medium|high|very_high` (→ 0–4).
- `--notifications`: `all_messages|only_mentions` (→ 0–1).
- `--content-filter`: `disabled|members_without_roles|all_members` (→ 0–2).
- `--afk-timeout`: `60|300|900|1800|3600` whitelist.
- Description is max 120 chars and only applies to **community** servers.
- `server-icon`: PNG/JPG/GIF **≤256KiB** (data-URI via `build_image_data_uri`);
  missing file → exit 7.

### Audit log (F7)

```
discord audit-log <GUILD> [--count N] [--type ACTION] [--user ID]   # VIEW_AUDIT_LOG
discord audit-types                                                # name → code table (local)
```

- Requires **VIEW_AUDIT_LOG**; 403 → exit 4.
- `--count` 1–100 (default 50; Discord caps at 100).
- `--type` is an **action name** (e.g. `member_kick`, `channel_create`, `role_delete`)
  resolved via the `AUDIT_ACTION_MAP`; unknown names → exit 2 listing valid ones.
- `--user` is a numeric user ID (audit entries reference users by snowflake).
- Output rows: `{user_id, username, action_name, action_type, target_id, reason, change_summary}`.
  `username` is resolved from the response `users` when present.
- `audit-types` prints the full name → code table locally (no API).

### Invites (F8)

```
discord invite-list <GUILD>                                     # MANAGE_CHANNELS
discord invite-create <GUILD> <CHANNEL> [--max-age N] [--max-uses N] [--temporary]   # CREATE_INSTANT_INVITE
discord invite-delete <CODE|URL> [--guild G] [--confirm]        # MANAGE_CHANNELS; gated
```

- `invite-create` targets a text-like channel; sets `unique: true` (one-time link).
  `--max-age` seconds (0 = never, default 86400), `--max-uses` (0 = unlimited).
  Not destructive — no `--confirm` needed (matches the reply-send path).
- `invite-delete` accepts a bare code or a full URL (`discord.gg/...`,
  `discord.com/invite/...`); `extract_invite_code` strips the prefix. `--confirm`
  required (exit 2 absent). `--guild` is context only.
- Output `invite-create`/`list` includes the `https://discord.gg/<code>` link.

### Embed (F9)

```
discord embed <CHANNEL> --title T [--description D] [--color HEX] [--url U] [--image I] [--thumbnail T] [--footer F] [--author A] [--field 'Name|Value|inline']... [--content C] [--reply ID] [--confirm] [--dry-run]
```

- Same send scope as `send` (SEND_MESSAGES); **--confirm required** to send,
  `--dry-run` previews `{action:"send_embed", title, description, fields: N}`.
- Requires ≥1 of `--title`/`--description`/`--content` else exit 2.
- `--color`: `#RRGGBB` or `RRGGBB` (via `parse_color_hex`); invalid → exit 2.
- `--field` repeatable, `Name|Value` or `Name|Value|inline` (inline defaults
  false); malformed (not 2–3 parts, empty name/value, or bad inline) → exit 2.
- Discord embed limits enforced by `validate_embed`: title ≤256, description
  ≤4096, ≤10 fields, field name ≤256, field value ≤1024, color 0x000000–0xFFFFFF.
- Rich-card note: embeds are the **visible "card"** surface — a branded message
  is far more noticeable to members and to Discord's abuse detection than plain
  text. Use at low volume.

## MCP tools

`discord serve` exposes the same operations to AI agents (all call the core
`ApiClient` — never Route directly):

- `create_channel` / `edit_channel` / `delete_channel` (delete gated `confirm: true`)
- `list_roles` / `create_role` / `edit_role` / `delete_role` / `assign_role` / `remove_role`
- `list_emojis` / `create_emoji` / `delete_emoji` (delete gated `confirm: true`)
- `kick_member` / `ban_member` / `unban_member` (gated `confirm: true`; ban takes
  `delete_message_days` 0–7) / `set_nickname` (no confirm)
- `view_overwrites` (read-only) / `set_overwrites` (`role_id` XOR `user_id`, ≥1 of
  allow/deny) / `lock_channel` / `unlock_channel` (both gated `confirm: true`)
- `edit_guild` (no confirm — reversible; note MANAGE_GUILD in the tool doc). Icon
  upload is CLI-only (`server-icon`) — file paths are awkward over MCP.
- `get_audit_logs` (read-only, VIEW_AUDIT_LOG; `action_type` is an action NAME,
  `limit` capped 100)
- `list_invites` / `create_invite` (no confirm — not destructive; `unique: true`) /
  `delete_invite` (gated `confirm: true`)
- `send_embed` (gated `confirm: true`; `fields: [{name, value, inline}]`; requires
  ≥1 of title/description/content; validation enforced)

## E2E

`scripts/e2e_admin.sh` exercises the full flow against a real administered
server. Destructive steps are gated behind `E2E_CONFIRM=1`. When no
administered guild is available it prints `[SKIP]` and exits 0.

```
E2E_CONFIRM=1 ./scripts/e2e_admin.sh
```
