#!/usr/bin/env bash
# Pull latest main, install ajax-cli from this workspace, restart dev web.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="$ROOT/.ajax-dev-web"
PID_FILE="$RUN_DIR/dev-web.pid"
LOG_FILE="$RUN_DIR/dev-web.log"

PROFILE="dev"
HOST="0.0.0.0"
PORT="8788"
PULL=1
INSTALL=1
FOREGROUND=0

usage() {
  cat <<'EOF'
Usage: scripts/dev-web-restart.sh [OPTIONS]

Pull latest origin/main into the current branch, install ajax-cli from this
repo (unless --no-install), stop the previous dev web server on the chosen
port, and start ajax-cli web with the dev profile.

Options:
  --foreground       Run the server in the foreground (do not detach)
  --no-pull          Skip `git fetch` / `git pull origin main`
  --no-install       Skip `cargo install --path crates/ajax-cli --locked`
  --host HOST        Bind address (default: 0.0.0.0)
  --port PORT        Listen port (default: 8788 for dev profile)
  --profile NAME     Ajax profile (default: dev)
  -h, --help         Show this help

After a background start, logs: .ajax-dev-web/dev-web.log
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --foreground)
      FOREGROUND=1
      shift
      ;;
    --no-pull)
      PULL=0
      shift
      ;;
    --no-install)
      INSTALL=0
      shift
      ;;
    --host)
      HOST="${2:?--host requires a value}"
      shift 2
      ;;
    --port)
      PORT="${2:?--port requires a value}"
      shift 2
      ;;
    --profile)
      PROFILE="${2:?--profile requires a value}"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "$RUN_DIR"

pull_from_main() {
  if ! git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "not a git repository: $ROOT" >&2
    exit 1
  fi
  if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
    echo "refusing to pull: working tree has uncommitted changes" >&2
    git -C "$ROOT" status --short >&2
    exit 1
  fi
  echo "Fetching origin/main ..."
  git -C "$ROOT" fetch origin main
  echo "Pulling origin/main into $(git -C "$ROOT" branch --show-current) ..."
  git -C "$ROOT" pull origin main --no-rebase
}

if [[ "$PULL" -eq 1 ]]; then
  pull_from_main
fi

if [[ "$INSTALL" -eq 1 ]]; then
  echo "Installing ajax-cli from $ROOT ..."
  cargo install --path "$ROOT/crates/ajax-cli" --locked
fi

stop_listener() {
  local port="$1"
  local pids
  pids="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)"
  if [[ -z "$pids" ]]; then
    return 0
  fi
  echo "Stopping listener(s) on port $port: $pids"
  # shellcheck disable=SC2086
  kill $pids 2>/dev/null || true
  sleep 1
  pids="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)"
  if [[ -n "$pids" ]]; then
    echo "Force-stopping listener(s) on port $port: $pids" >&2
    # shellcheck disable=SC2086
    kill -9 $pids 2>/dev/null || true
    sleep 1
  fi
}

stop_pid_file() {
  if [[ ! -f "$PID_FILE" ]]; then
    return 0
  fi
  local old_pid
  old_pid="$(cat "$PID_FILE")"
  if [[ -n "$old_pid" ]] && kill -0 "$old_pid" 2>/dev/null; then
    echo "Stopping previous dev web (pid $old_pid) ..."
    kill "$old_pid" 2>/dev/null || true
    sleep 1
    if kill -0 "$old_pid" 2>/dev/null; then
      kill -9 "$old_pid" 2>/dev/null || true
    fi
  fi
  rm -f "$PID_FILE"
}

stop_pid_file
stop_listener "$PORT"

if ! command -v ajax-cli >/dev/null 2>&1; then
  echo "ajax-cli not found on PATH after install" >&2
  exit 1
fi

CMD=(ajax-cli --profile "$PROFILE" web --host "$HOST" --port "$PORT")

if [[ "$FOREGROUND" -eq 1 ]]; then
  echo "Starting ${CMD[*]} (foreground) ..."
  exec "${CMD[@]}"
fi

echo "Starting ${CMD[*]} (background) ..."
: >"$LOG_FILE"
nohup "${CMD[@]}" >>"$LOG_FILE" 2>&1 &
NEW_PID=$!
echo "$NEW_PID" >"$PID_FILE"

sleep 1
if ! kill -0 "$NEW_PID" 2>/dev/null; then
  echo "dev web failed to start; see $LOG_FILE" >&2
  tail -20 "$LOG_FILE" >&2 || true
  exit 1
fi

echo "Dev web running (pid $NEW_PID)"
echo "  URL:  https://127.0.0.1:$PORT"
echo "  Log:  $LOG_FILE"
