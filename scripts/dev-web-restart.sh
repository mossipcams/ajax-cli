#!/usr/bin/env bash
# Always force-sync local main, install ajax-cli from its worktree, restart web in tmux.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

main_worktree() {
  git -C "$REPO_ROOT" for-each-ref --format='%(worktreepath)' refs/heads/main
}

ROOT="$(main_worktree)"
if [[ -z "$ROOT" ]]; then
  echo "local main worktree not found for repository: $REPO_ROOT" >&2
  exit 1
fi
GIT_DIR="$(git -C "$REPO_ROOT" rev-parse --absolute-git-dir)"
RUN_DIR="$ROOT/.ajax-dev-web"

PROFILE="dev"
HOST="0.0.0.0"
PORT=""
INSTALL=1
FOREGROUND=0

usage() {
  cat <<'EOF'
Usage: scripts/dev-web-restart.sh [OPTIONS]

Always fetch and force-sync the local main worktree to origin/main, install
ajax-cli from that worktree (unless --no-install), stop the previous managed
web server for the selected profile, and start ajax-cli web in a durable tmux
session.

Options:
  --foreground       Run the server in the foreground (do not detach)
  --no-install       Skip `cargo install --path crates/ajax-cli --locked`
  --host HOST        Bind address (default: 0.0.0.0)
  --port PORT        Listen port (default: 8788 for dev, 8787 for stable)
  --profile NAME     Ajax profile (default: dev)
  -h, --help         Show this help

Background mode uses tmux session ajax-web-<profile>.
Logs: .ajax-dev-web/<profile>-web.log
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --foreground)
      FOREGROUND=1
      shift
      ;;
    --no-pull)
      echo "error: --no-pull is removed; this script always syncs to origin/main" >&2
      exit 2
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

# Default ports by profile when --port is omitted.
if [[ -z "$PORT" ]]; then
  case "$PROFILE" in
    stable) PORT="8787" ;;
    *) PORT="8788" ;;
  esac
fi

PID_FILE="$RUN_DIR/${PROFILE}-web.pid"
LOG_FILE="$RUN_DIR/${PROFILE}-web.log"
TMUX_SESSION="ajax-web-${PROFILE}"

sync_main() {
  echo "Fetching origin/main ..."
  git -C "$REPO_ROOT" fetch origin main:refs/remotes/origin/main
  echo "Force-syncing local main worktree to origin/main ..."
  git --git-dir="$GIT_DIR" --work-tree="$ROOT" reset --hard origin/main
  git --git-dir="$GIT_DIR" --work-tree="$ROOT" clean -fd
}

sync_main

mkdir -p "$RUN_DIR"

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
  echo "refusing to stop unmanaged listener(s) on port $port: $pids" >&2
  exit 1
}

stop_pid_file() {
  if [[ ! -f "$PID_FILE" ]]; then
    return 0
  fi
  local old_pid
  local old_command
  old_pid="$(cat "$PID_FILE")"
  if [[ -n "$old_pid" ]] && kill -0 "$old_pid" 2>/dev/null; then
    old_command="$(ps -p "$old_pid" -o command= 2>/dev/null || true)"
    if [[ "$old_command" != *ajax-cli* || "$old_command" != *web* ]]; then
      echo "refusing to stop pid-file process $old_pid; not an ajax-cli web process" >&2
      exit 1
    fi
    echo "Stopping previous ${PROFILE} web (pid $old_pid) ..."
    kill "$old_pid" 2>/dev/null || true
    sleep 1
    if kill -0 "$old_pid" 2>/dev/null; then
      kill -9 "$old_pid" 2>/dev/null || true
    fi
  fi
  rm -f "$PID_FILE"
}

stop_tmux_session() {
  if ! command -v tmux >/dev/null 2>&1; then
    return 0
  fi
  if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    echo "Stopping tmux session $TMUX_SESSION ..."
    tmux kill-session -t "$TMUX_SESSION"
    sleep 1
  fi
}

stop_tmux_session
stop_pid_file
stop_listener "$PORT"

if ! command -v ajax-cli >/dev/null 2>&1; then
  echo "ajax-cli not found on PATH after install" >&2
  exit 1
fi

CMD=(ajax-cli --profile "$PROFILE" web --host "$HOST" --port "$PORT")
RESTART_SCRIPT="$REPO_ROOT/scripts/dev-web-restart.sh"
export AJAX_WEB_RESTART_SCRIPT="$RESTART_SCRIPT"
export AJAX_WEB_RESTART_PROFILE="$PROFILE"
export AJAX_WEB_RESTART_PORT="$PORT"

if [[ "$FOREGROUND" -eq 1 ]]; then
  echo "Starting ${CMD[*]} (foreground) ..."
  exec "${CMD[@]}"
fi

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for background web restarts (nohup exits when the launcher session ends)" >&2
  exit 1
fi

echo "Starting ${CMD[*]} (tmux session $TMUX_SESSION) ..."
: >"$LOG_FILE"
# ponytail: tmux keeps the server alive; nohup from agent/CI shells still dies.
# Ceiling: requires tmux. Upgrade: launchd plist if we need login-boot without tmux.
tmux new-session -d -s "$TMUX_SESSION" -c "$ROOT" \
  "AJAX_WEB_RESTART_SCRIPT=$(printf %q "$RESTART_SCRIPT") AJAX_WEB_RESTART_PROFILE=$(printf %q "$PROFILE") AJAX_WEB_RESTART_PORT=$(printf %q "$PORT") ajax-cli --profile $(printf %q "$PROFILE") web --host $(printf %q "$HOST") --port $(printf %q "$PORT") 2>&1 | tee -a $(printf %q "$LOG_FILE"); echo EXIT:\$? >> $(printf %q "$LOG_FILE")"

sleep 1
NEW_PID="$(lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -t 2>/dev/null | head -1 || true)"
if [[ -z "$NEW_PID" ]] || ! kill -0 "$NEW_PID" 2>/dev/null; then
  echo "${PROFILE} web failed to start; see $LOG_FILE" >&2
  tail -20 "$LOG_FILE" >&2 || true
  tmux kill-session -t "$TMUX_SESSION" 2>/dev/null || true
  exit 1
fi
echo "$NEW_PID" >"$PID_FILE"

echo "${PROFILE} web running (pid $NEW_PID, tmux $TMUX_SESSION)"
echo "  URL:  https://127.0.0.1:$PORT"
echo "  Log:  $LOG_FILE"
