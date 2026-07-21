#!/usr/bin/env bash
# Per-task worktree setup for ajax-cli (intended as managed-repo `bootstrap`).
#
# Ensures Node 22 (CI pin), installs JS deps + husky via `npm ci`, and — when
# present — restores local ajax-model-router dispatch symlinks into scripts/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]]; then
  # shellcheck disable=SC1091
  . "${NVM_DIR:-$HOME/.nvm}/nvm.sh"
  nvm install 22
  nvm use 22
elif ! command -v node >/dev/null 2>&1; then
  echo "task-bootstrap: need nvm or node on PATH" >&2
  exit 1
fi

node_major="$(node -p "process.versions.node.split('.')[0]")"
if [[ "$node_major" != "22" ]]; then
  echo "task-bootstrap: refusing Node $(node -v); need major 22 (CI pin)" >&2
  exit 1
fi

npm ci

router="${AJAX_MODEL_ROUTER:-$HOME/Desktop/Projects/ajax-model-router}"
installer="$router/scripts/install-symlinks"
if [[ -x "$installer" ]]; then
  "$installer" --target "$ROOT" --force
fi
