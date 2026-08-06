#!/usr/bin/env bash
# =============================================================================
# e2e.sh — end-to-end smoke test for discord-cli (REAL token, LOW volume).
#
# Requires a valid DISCORD_TOKEN (env or .env). Runs the core read/write flow
# against a real account. Destructive commands are avoided or --confirm-gated.
#
# Usage:
#   DISCORD_TOKEN=<token> ./scripts/e2e.sh
#   DISCORD_TOKEN=<token> ./scripts/e2e.sh --skip-send
# =============================================================================
set -euo pipefail
BIN="${BIN:-target/debug/discord.exe}"
SKIP_SEND="${1:-}"

if [ ! -x "$BIN" ]; then
  echo "[ERROR] build first: cargo build" >&2
  exit 1
fi

echo "=== 1. status (validate token) ==="
"$BIN" status || { echo "[FAIL] status"; exit 1; }
echo "  OK"

echo "=== 2. whoami (profile) ==="
"$BIN" whoami --json || { echo "[FAIL] whoami"; exit 1; }

echo "=== 3. guilds (list servers) ==="
"$BIN" guilds --json || { echo "[FAIL] guilds"; exit 1; }

echo "=== 4. dms (list DMs) ==="
"$BIN" dms --json || { echo "[WARN] dms (may be empty)"; }

echo "=== 5. channels (first guild) ==="
FIRST_GUILD=$("$BIN" guilds --json | head -1 | grep -o '"id":"[0-9]*"' | head -1 | cut -d'"' -f4)
if [ -n "$FIRST_GUILD" ]; then
  "$BIN" channels "$FIRST_GUILD" --json || echo "[WARN] channels"
else
  echo "[SKIP] no guilds"
fi

if [ "$SKIP_SEND" != "--skip-send" ]; then
  echo "=== 6. send --dry-run (preview, no actual send) ==="
  "$BIN" send 0 --text "e2e dry run" --dry-run || { echo "[FAIL] dry-run"; exit 1; }
  echo "  (dry-run only — no message actually sent)"
else
  echo "[SKIP] send"
fi

echo "=== 7. sync-all --limit 20 (bounded, local archive) ==="
"$BIN" sync-all --limit 20 || echo "[WARN] sync-all (may need channels)"

echo
echo "=== E2E PASS ==="
