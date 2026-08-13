#!/usr/bin/env bash
# Start isolated ajax-cli web for exploratory CI. Writes pid + log under the instance dir.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTANCE="$ROOT/target/exploratory-instance"
ENV_JSON="$INSTANCE/env.json"
LOG="$INSTANCE/server.log"
PID_FILE="$INSTANCE/server.pid"

if [[ ! -f "$ENV_JSON" ]]; then
  echo "missing $ENV_JSON — run prepare-instance.mjs first" >&2
  exit 1
fi

CONFIG="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).config)" "$ENV_JSON")"
STATE="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).state)" "$ENV_JSON")"
WORKTREE_ROOT="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).worktreeRoot)" "$ENV_JSON")"
PORT="$(node -e "console.log(JSON.parse(require('fs').readFileSync(process.argv[1],'utf8')).AJAX_EXPLORATORY_PORT)" "$ENV_JSON")"

BIN="${AJAX_EXPLORATORY_BIN:-$ROOT/target/release/ajax-cli}"
if [[ ! -x "$BIN" ]]; then
  echo "ajax-cli binary not found/executable: $BIN" >&2
  exit 1
fi

mkdir -p "$(dirname "$STATE")" "$WORKTREE_ROOT"
: >"$LOG"

STUBS="$ROOT/scripts/exploratory/agent-stubs"
export PATH="$STUBS:$PATH"

nohup "$BIN" \
  --config "$CONFIG" \
  --state "$STATE" \
  --worktree-root "$WORKTREE_ROOT" \
  web --host 127.0.0.1 --port "$PORT" \
  >"$LOG" 2>&1 &
echo $! >"$PID_FILE"
echo "started ajax-cli web pid=$(cat "$PID_FILE") port=$PORT log=$LOG"
