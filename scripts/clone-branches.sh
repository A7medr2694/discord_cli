#!/usr/bin/env bash
# =============================================================================
# clone-branches.sh
#
# Clones every remote branch of every discord-cli repo already cloned in
# .tmp/ into .tmp/branches/<owner-repo>/<branch>.
#
# Usage:
#   ./clone-branches.sh                 # clone all branches of all repos
#   ./clone-branches.sh ayn2op          # only repos whose dir name contains arg
#   TARGET_DIR=/some/path ./clone-branches.sh   # override output dir
# =============================================================================
set -euo pipefail

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${TARGET_DIR:-$BASE_DIR/.tmp/branches}"
FILTER="${1:-}"

command -v git >/dev/null 2>&1 || { echo "[ERROR] git not found on PATH." >&2; exit 1; }

mkdir -p "$TARGET"

count=0
for repo in "$BASE_DIR"/.tmp/*/; do
  [ -d "$repo/.git" ] || continue
  name="$(basename "$repo")"
  if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then
    continue
  fi

  url="$(git -C "$repo" config --get remote.origin.url 2>/dev/null || true)"
  [ -n "$url" ] || continue

  echo "=== $name ==="
  branches="$(git ls-remote --heads "$url" 2>/dev/null | awk '{print $2}' | sed 's|^refs/heads/||' || true)"
  if [ -z "$branches" ]; then
    echo "  (no remote branches)"
    continue
  fi

  while IFS= read -r branch; do
    [ -n "$branch" ] || continue
    safe="${branch//\//_}"
    dest="$TARGET/$name/$safe"
    if [ -d "$dest/.git" ]; then
      echo "  [skip]  $branch"
    else
      echo "  [clone] $branch"
      if git clone --quiet --depth 1 --branch "$branch" "$url" "$dest"; then
        count=$((count + 1))
      else
        echo "  [FAIL]  $branch"
      fi
    fi
  done <<< "$branches"
done

echo
echo "Done. $count branch clone(s) in: $TARGET"
