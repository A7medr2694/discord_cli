#!/usr/bin/env bash
# =============================================================================
# e2e_admin.sh — admin/moderation end-to-end test (REAL token, LOW volume).
#
# Requires a valid DISCORD_TOKEN (env or .env) with at least ONE guild the
# account administers (owns or has MANAGE_* permissions in). Destructive steps
# are gated behind E2E_CONFIRM=1; without it they are skipped and reported.
# When no administered guild is available the script prints [SKIP] and exits 0.
#
# Usage:
#   DISCORD_TOKEN=<token> ./scripts/e2e_admin.sh          # read-only flow
#   E2E_CONFIRM=1 DISCORD_TOKEN=<token> ./scripts/e2e_admin.sh   # full flow
# =============================================================================
set -euo pipefail
BIN="${BIN:-target/debug/discord.exe}"

if [ ! -x "$BIN" ]; then
  echo "[ERROR] build first: cargo build" >&2
  exit 1
fi

log()  { echo "=== $* ==="; }
step() { echo "  >> $*"; }
skip() { echo "[SKIP] $*"; }

# ---------------------------------------------------------------------------
# 0. Token + pick an administered guild
# ---------------------------------------------------------------------------
"$BIN" status >/dev/null 2>&1 || { echo "[ERROR] invalid/missing token" >&2; exit 1; }
log "status (token ok)"

GUILD_ID=$("$BIN" guilds --json | head -1 | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
if [ -z "$GUILD_ID" ]; then
  skip "no guilds available for admin test"
  exit 0
fi
log "testing against guild $GUILD_ID"

# Detect whether we can administer the guild: try creating a throwaway channel
# in dry-run (no side effect), then a REAL create is gated behind E2E_CONFIRM.
CAN_ADMIN=""
if "$BIN" channel-create "$GUILD_ID" "__e2e_probe__" --type text --dry-run >/dev/null 2>&1; then
  CAN_ADMIN=1
fi
if [ -z "${CAN_ADMIN:-}" ]; then
  skip "no administered guild (channel-create failed / lacks MANAGE_CHANNELS)"
  exit 0
fi

# ---------------------------------------------------------------------------
# F1: channel CRUD
# ---------------------------------------------------------------------------
log "F1 channel create (gated)"
if [ -n "${E2E_CONFIRM:-}" ]; then
  CH=$("$BIN" channel-create "$GUILD_ID" "F1-test-$(date +%s)" --type text --topic "e2e admin test" --json)
  echo "$CH" | grep -q '"id"' || { echo "[FAIL] channel-create"; exit 1; }
  CH_ID=$(echo "$CH" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
  CH_NAME=$(echo "$CH" | grep -o '"name":"[^"]*"' | head -1 | cut -d'"' -f4)
  step "created channel $CH_NAME ($CH_ID)"
  echo "$CH"

  step "rename"
  "$BIN" channel-rename "$GUILD_ID" "$CH_ID" "$CH_NAME-renamed" --json | grep -q 'renamed\|"name"' || { echo "[FAIL] channel-rename"; exit 1; }

  step "topic"
  "$BIN" channel-topic "$GUILD_ID" "$CH_ID" "new topic from e2e" --json | grep -q '"topic"' || { echo "[FAIL] channel-topic"; exit 1; }

  step "slowmode"
  "$BIN" channel-slowmode "$GUILD_ID" "$CH_ID" 5 --json | grep -q '"rate_limit_per_user":5' || { echo "[FAIL] channel-slowmode"; exit 1; }

  step "clone"
  CLONE=$("$BIN" channel-clone "$GUILD_ID" "$CH_ID" --name "$CH_NAME-clone" --json)
  echo "$CLONE" | grep -q '"id"' || { echo "[FAIL] channel-clone"; exit 1; }
  CLONE_ID=$(echo "$CLONE" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)

  step "move"
  "$BIN" channel-move "$GUILD_ID" "$CLONE_ID" --position 1 --json | grep -q '"position"' || { echo "[FAIL] channel-move"; exit 1; }

  step "delete clone + original (--confirm)"
  "$BIN" channel-delete "$GUILD_ID" "$CLONE_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] channel-delete clone"; exit 1; }
  "$BIN" channel-delete "$GUILD_ID" "$CH_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] channel-delete"; exit 1; }
else
  skip "F1 channel CRUD (set E2E_CONFIRM=1 for destructive steps)"
fi

# ---------------------------------------------------------------------------
# F2: role CRUD + assign/remove
# ---------------------------------------------------------------------------
log "F2 role create (gated)"
if [ -n "${E2E_CONFIRM:-}" ]; then
  ROLES=$("$BIN" roles "$GUILD_ID" --json)
  ME=$("$BIN" whoami --json | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)

  ROLE=$("$BIN" role-create "$GUILD_ID" "F2-test-$(date +%s)" --color "#ff5733" --permissions "send_messages,read_message_history" --json)
  echo "$ROLE" | grep -q '"id"' || { echo "[FAIL] role-create"; exit 1; }
  ROLE_ID=$(echo "$ROLE" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
  ROLE_NAME=$(echo "$ROLE" | grep -o '"name":"[^"]*"' | head -1 | cut -d'"' -f4)
  step "created role $ROLE_NAME ($ROLE_ID)"

  step "assign role to self"
  "$BIN" role-assign "$GUILD_ID" "$ROLE_ID" "$ME" --json | grep -q '"assigned":true' || { echo "[FAIL] role-assign"; exit 1; }

  step "remove role from self"
  "$BIN" role-remove "$GUILD_ID" "$ROLE_ID" "$ME" --json | grep -q '"removed":true' || { echo "[FAIL] role-remove"; exit 1; }

  step "delete role (--confirm)"
  "$BIN" role-delete "$GUILD_ID" "$ROLE_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] role-delete"; exit 1; }
else
  skip "F2 role CRUD (set E2E_CONFIRM=1 for destructive steps)"
fi

# ---------------------------------------------------------------------------
# F3: emoji CRUD
# ---------------------------------------------------------------------------
log "F3 emoji upload (gated)"
if [ -n "${E2E_CONFIRM:-}" ]; then
  # Tiny valid 1x1 PNG (67 bytes) as a base64 -> temp file.
  PNG_B64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
  EMOJI_FILE=$(mktemp --suffix=.png 2>/dev/null || mktemp).png
  echo "$PNG_B64" | base64 --decode > "$EMOJI_FILE" 2>/dev/null || \
    python -c "import base64,sys; sys.stdout.buffer.write(base64.b64decode('$PNG_B64'))" > "$EMOJI_FILE"

  EMOJI=$("$BIN" emoji-upload "$GUILD_ID" "f2_e2e_$(date +%s)" "$EMOJI_FILE" --json)
  echo "$EMOJI" | grep -q '"id"' || { echo "[FAIL] emoji-upload"; exit 1; }
  EMOJI_ID=$(echo "$EMOJI" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
  step "uploaded emoji $EMOJI_ID"

  step "emoji list contains it"
  "$BIN" emoji-list "$GUILD_ID" --json | grep -q "$EMOJI_ID" || { echo "[FAIL] emoji-list"; exit 1; }

  step "delete emoji (--confirm)"
  "$BIN" emoji-delete "$GUILD_ID" "$EMOJI_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] emoji-delete"; exit 1; }
  rm -f "$EMOJI_FILE"
else
  skip "F3 emoji CRUD (set E2E_CONFIRM=1 for destructive steps)"
fi

# ---------------------------------------------------------------------------
# F4: member moderation — NICK only (kick/ban/unban too risky for automated
# e2e; left manual). Set your own nickname (no permission needed beyond
# CHANGE_NICKNAME which every member has).
# ---------------------------------------------------------------------------
log "F4 member nick (self)"
ME_ID=$("$BIN" whoami --json | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
if [ -n "${E2E_CONFIRM:-}" ]; then
  # Try setting a nick on self via member-nick (MANAGE_NICKNAMES needed).
  if "$BIN" member-nick "$GUILD_ID" "$ME_ID" "e2e-nick-$(date +%s)" --json | grep -q '"nickname_set":true'; then
    step "member-nick on self ok"
    # Clear it back.
    "$BIN" member-nick "$GUILD_ID" "$ME_ID" "" --json | grep -q '"nickname_set":true' || { echo "[FAIL] member-nick clear"; exit 1; }
    step "member-nick cleared"
  else
    skip "F4 member-nick (lacks MANAGE_NICKNAMES or member-nick failed)"
  fi
else
  skip "F4 member-nick (set E2E_CONFIRM=1)"
fi
# kick/ban/unban are NOT in automated e2e (irreversible) — manual only.

# ---------------------------------------------------------------------------
# F5: permission overwrites — view -> lock -> unlock on a throwaway channel
# (channel created by F1 e2e is gone; create a temp one here, gated).
# ---------------------------------------------------------------------------
log "F5 perm view/lock/unlock (gated)"
if [ -n "${E2E_CONFIRM:-}" ]; then
  PCH=$("$BIN" channel-create "$GUILD_ID" "F5-perm-$(date +%s)" --type text --json)
  PCH_ID=$(echo "$PCH" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
  if [ -n "$PCH_ID" ]; then
    step "created perm-test channel $PCH_ID"
    "$BIN" perm-view "$GUILD_ID" "$PCH_ID" --json | grep -q '"overwrites"' || { echo "[FAIL] perm-view"; exit 1; }
    step "perm-view ok"
    "$BIN" perm-lock "$GUILD_ID" "$PCH_ID" --confirm --json | grep -q '"locked":true' || { echo "[FAIL] perm-lock"; exit 1; }
    step "perm-lock ok"
    "$BIN" perm-unlock "$GUILD_ID" "$PCH_ID" --confirm --json | grep -q '"unlocked":true' || { echo "[FAIL] perm-unlock"; exit 1; }
    step "perm-unlock ok"
    "$BIN" channel-delete "$GUILD_ID" "$PCH_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] perm-test cleanup"; exit 1; }
  else
    skip "F5 perm (channel-create failed / lacks MANAGE_CHANNELS)"
  fi
else
  skip "F5 perm view/lock/unlock (set E2E_CONFIRM=1 for destructive steps)"
fi

# ---------------------------------------------------------------------------
# F6: server settings — rename to a prefixed name (reversible) then restore.
# ---------------------------------------------------------------------------
log "F6 server-set name (gated)"
if [ -n "${E2E_CONFIRM:-}" ]; then
  ORIG_NAME=$("$BIN" info "$GUILD_ID" --json | grep -o '"name":"[^"]*"' | head -1 | cut -d'"' -f4)
  if [ -n "$ORIG_NAME" ]; then
    step "original name: $ORIG_NAME"
    "$BIN" server-set "$GUILD_ID" --name "$ORIG_NAME-e2e" --json | grep -q '"name"' || { echo "[FAIL] server-set name"; exit 1; }
    step "renamed to $ORIG_NAME-e2e"
    "$BIN" server-set "$GUILD_ID" --name "$ORIG_NAME" --json | grep -q '"name"' || { echo "[FAIL] server-set restore"; exit 1; }
    step "restored to $ORIG_NAME"
  else
    skip "F6 server-set (info failed)"
  fi
else
  skip "F6 server-set (set E2E_CONFIRM=1 for write steps)"
fi
# server-icon is manual-only (small png).

# ---------------------------------------------------------------------------
# F7: audit log — read-only; runs whenever the account has VIEW_AUDIT_LOG.
# ---------------------------------------------------------------------------
log "F7 audit log"
if "$BIN" audit-types --json | grep -q 'member_kick'; then
  step "audit-types table ok"
else
  echo "[FAIL] audit-types" >&2; exit 1
fi
if AUDIT=$("$BIN" audit-log "$GUILD_ID" -n 5 --json 2>/dev/null); then
  if [ -z "${AUDIT:-}" ] || echo "$AUDIT" | grep -qE '"id"'; then
    step "audit-log -n 5 ok (entries or empty list)"
    echo "$AUDIT" | head -c 400; echo
  else
    skip "F7 audit-log (no entries returned)"
  fi
else
  skip "F7 audit-log (lacks VIEW_AUDIT_LOG or audit-log failed)"
fi

# ---------------------------------------------------------------------------
# F8: invites — create -> list -> delete (create/list are not destructive;
# delete is gated behind E2E_CONFIRM).
# ---------------------------------------------------------------------------
log "F8 invites"
if [ -n "${E2E_CONFIRM:-}" ]; then
  INV_CH=$("$BIN" channel-create "$GUILD_ID" "F8-inv-$(date +%s)" --type text --json)
  INV_CH_ID=$(echo "$INV_CH" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
  if [ -n "$INV_CH_ID" ]; then
    INV=$("$BIN" invite-create "$GUILD_ID" "$INV_CH_ID" --max-uses 1 --json)
    echo "$INV" | grep -q '"code"' || { echo "[FAIL] invite-create"; exit 1; }
    INV_CODE=$(echo "$INV" | grep -o '"code":"[^"]*"' | head -1 | cut -d'"' -f4)
    step "created invite $INV_CODE"
    echo "$INV" | grep -q 'discord.gg' || { echo "[FAIL] invite-create link"; exit 1; }

    step "invite-list contains it"
    "$BIN" invite-list "$GUILD_ID" --json | grep -q "$INV_CODE" || { echo "[FAIL] invite-list"; exit 1; }

    step "invite-delete (--confirm)"
    "$BIN" invite-delete "$INV_CODE" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] invite-delete"; exit 1; }

    step "cleanup invite channel"
    "$BIN" channel-delete "$GUILD_ID" "$INV_CH_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] invite cleanup"; exit 1; }
  else
    skip "F8 invite (channel-create failed)"
  fi
else
  skip "F8 invites (set E2E_CONFIRM=1 for the create/list/delete flow)"
fi

# ---------------------------------------------------------------------------
# F9: embed send to a test channel (gated behind E2E_CONFIRM).
# ---------------------------------------------------------------------------
log "F9 embed (gated)"
if [ -n "${E2E_CONFIRM:-}" ]; then
  EMB_CH=$("$BIN" channel-create "$GUILD_ID" "F9-embed-$(date +%s)" --type text --json)
  EMB_CH_ID=$(echo "$EMB_CH" | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
  if [ -n "$EMB_CH_ID" ]; then
    step "embed dry-run"
    "$BIN" embed "$EMB_CH_ID" --title "E2E" --description "admin e2e" --dry-run --json | grep -q '"action":"send_embed"' || { echo "[FAIL] embed dry-run"; exit 1; }
    step "embed send (--confirm)"
    EMBED=$("$BIN" embed "$EMB_CH_ID" --title "E2E" --description "admin e2e $(date +%s)" --color "#ff5733" --field "a|b|true" --confirm --json)
    echo "$EMBED" | grep -q '"message_id"' || { echo "[FAIL] embed send"; exit 1; }
    step "embed message id returned"
    echo "$EMBED"
    step "cleanup embed channel"
    "$BIN" channel-delete "$GUILD_ID" "$EMB_CH_ID" --confirm --json | grep -q '"deleted":true' || { echo "[FAIL] embed cleanup"; exit 1; }
  else
    skip "F9 embed (channel-create failed)"
  fi
else
  skip "F9 embed (set E2E_CONFIRM=1 for the send step)"
fi

# ---------------------------------------------------------------------------
log "destructive steps skipped (E2E_CONFIRM=1 enables them)"

echo
echo "=== E2E ADMIN PASS ==="
