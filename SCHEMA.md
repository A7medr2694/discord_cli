# Output contract (SCHEMA.md)

Every command emits a stable, machine-readable envelope.

## Envelope

```json
{ "ok": true,  "schema_version": "1", "data": <any> }
{ "ok": false, "schema_version": "1", "error": { "code": "<string>", "message": "<string>", "details": "<optional>" } }
```

## Formats

- **Piped stdout** (non-TTY): **JSONL** — one JSON object per line (for list outputs).
- **`--json`**: single envelope, pretty-printed.
- **`--yaml`**: envelope as YAML.
- **`OUTPUT=json|jsonl|yaml|rich`**: env override.
- **TTY**: human-readable (tables / color).

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | OK |
| 1 | Generic error |
| 2 | Usage error (e.g. missing `--confirm`) |
| 3 | Not found |
| 4 | Forbidden / rate-limited |
| 5 | Network / timeout |
| 7 | Attachment / file IO error (send --file) |

## Entity shapes

### Guild
```json
{ "id": "…", "name": "…", "icon": "…" | null, "owner": true|false }
```

### Channel
```json
{ "id": "…", "name": "…", "guild_id": "…" | null, "type": 0|5|15, "topic": "…" | null, "parent_id": "…" | null, "position": 0 }
```

### Message (agent-facing)
```json
{ "message_id": "…", "channel_id": "…", "guild_id": "…" | null,
  "author_id": "…", "author": "…", "timestamp": "RFC3339", "content": "…",
  "attachments": ["…"] | null }
```

### DM channel
```json
{ "id": "…", "label": "user#disc" | "a, b", "type": 1|3, "recipient_count": 1 }
```

## Notes

- `guild_id` is `null` for DMs; display names use `COALESCE(g.name,'DM')`.
- Snowflakes are strings; message cursors compare lexically.
- `schema_version` is fixed at `"1"` unless a breaking change.
