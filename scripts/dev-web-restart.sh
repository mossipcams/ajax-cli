#!/usr/bin/env bash
# Sync/install ajax-cli and restart web in tmux.
#
# Default (no --worktree): force-sync local main to origin/main, install from
# main into ~/.cargo/bin, restart the selected profile.
#
# --worktree PATH (Test in Dev): build that worktree as-is (including dirty
# files), install into .ajax-dev-web/bin so stable's cargo bin is untouched,
# then restart only the selected profile (normally dev / 8788). Never runs
# git reset/clean/checkout against the task worktree.
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
WORKTREE=""

usage() {
  cat <<'EOF'
Usage: scripts/dev-web-restart.sh [OPTIONS]

Default: fetch and force-sync the local main worktree to origin/main, install
ajax-cli from that worktree (unless --no-install), stop the previous managed
web server for the selected profile, and start ajax-cli web in a durable tmux
session.

With --worktree PATH: skip git sync, build/install from PATH (uncommitted
changes included), install into .ajax-dev-web/bin, and restart only the
selected profile. Does not modify the task worktree.

Options:
  --foreground       Run the server in the foreground (do not detach)
  --no-install       Skip cargo install
  --worktree PATH    Build/install from this Ajax-managed worktree (no git sync)
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
      echo "error: --no-pull is removed; omit --worktree to sync to origin/main" >&2
      exit 2
      ;;
    --no-install)
      INSTALL=0
      shift
      ;;
    --worktree)
      WORKTREE="${2:?--worktree requires a value}"
      shift 2
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

# Test in Dev must never target stable.
if [[ -n "$WORKTREE" && "$PROFILE" == "stable" ]]; then
  echo "refusing --worktree deploy for profile=stable" >&2
  exit 2
fi

PID_FILE="$RUN_DIR/${PROFILE}-web.pid"
LOG_FILE="$RUN_DIR/${PROFILE}-web.log"
TMUX_SESSION="ajax-web-${PROFILE}"
SLOT_BIN_DIR="$RUN_DIR/bin"
SLOT_BIN="$SLOT_BIN_DIR/ajax-cli"
SLOT_BIN_PREV="$SLOT_BIN_DIR/ajax-cli.prev"

sync_main() {
  echo "Fetching origin/main ..."
  git -C "$REPO_ROOT" fetch origin main:refs/remotes/origin/main
  echo "Force-syncing local main worktree to origin/main ..."
  git --git-dir="$GIT_DIR" --work-tree="$ROOT" reset --hard origin/main
  git --git-dir="$GIT_DIR" --work-tree="$ROOT" clean -fd
}

SOURCE_ROOT="$ROOT"
if [[ -n "$WORKTREE" ]]; then
  if [[ ! -d "$WORKTREE" ]]; then
    echo "worktree path does not exist: $WORKTREE" >&2
    exit 1
  fi
  SOURCE_ROOT="$(cd "$WORKTREE" && pwd)"
  echo "AJAX_DEV_DEPLOY_PHASE=building"
  echo "Test in Dev: building from worktree $SOURCE_ROOT (no git sync)"
else
  sync_main
fi

mkdir -p "$RUN_DIR"

BIN_CMD=(ajax-cli)
USE_SLOT_BIN=0

if [[ "$INSTALL" -eq 1 ]]; then
  if [[ -n "$WORKTREE" ]]; then
    echo "Building frontend in $SOURCE_ROOT ..."
    npm --prefix "$SOURCE_ROOT" run web:build

    mkdir -p "$SLOT_BIN_DIR"
    if [[ -x "$SLOT_BIN" ]]; then
      cp -f "$SLOT_BIN" "$SLOT_BIN_PREV"
    fi

    echo "Installing ajax-cli from $SOURCE_ROOT into $RUN_DIR ..."
    cargo install --path "$SOURCE_ROOT/crates/ajax-cli" --locked --root "$RUN_DIR" --force
    if [[ ! -x "$SLOT_BIN" ]]; then
      echo "slot binary missing after install: $SLOT_BIN" >&2
      exit 1
    fi
    BIN_CMD=("$SLOT_BIN")
    USE_SLOT_BIN=1
  else
    echo "Installing ajax-cli from $ROOT ..."
    cargo install --path "$ROOT/crates/ajax-cli" --locked
  fi
elif [[ -n "$WORKTREE" && -x "$SLOT_BIN" ]]; then
  BIN_CMD=("$SLOT_BIN")
  USE_SLOT_BIN=1
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

start_web() {
  local bin_path="$1"
  local cmd_display=("$bin_path" --profile "$PROFILE" web --host "$HOST" --port "$PORT")

  if [[ "$FOREGROUND" -eq 1 ]]; then
    echo "Starting ${cmd_display[*]} (foreground) ..."
    exec "$bin_path" --profile "$PROFILE" web --host "$HOST" --port "$PORT"
  fi

  if ! command -v tmux >/dev/null 2>&1; then
    echo "tmux is required for background web restarts (nohup exits when the launcher session ends)" >&2
    exit 1
  fi

  echo "Starting ${cmd_display[*]} (tmux session $TMUX_SESSION) ..."
  : >"$LOG_FILE"
  # ponytail: tmux keeps the server alive; nohup from agent/CI shells still dies.
  # Ceiling: requires tmux. Upgrade: launchd plist if we need login-boot without tmux.
  tmux new-session -d -s "$TMUX_SESSION" -c "$ROOT" \
    "AJAX_WEB_RESTART_SCRIPT=$(printf %q "$RESTART_SCRIPT") AJAX_WEB_RESTART_PROFILE=$(printf %q "$PROFILE") AJAX_WEB_RESTART_PORT=$(printf %q "$PORT") $(printf %q "$bin_path") --profile $(printf %q "$PROFILE") web --host $(printf %q "$HOST") --port $(printf %q "$PORT") 2>&1 | tee -a $(printf %q "$LOG_FILE"); echo EXIT:\$? >> $(printf %q "$LOG_FILE")"

  sleep 1
  NEW_PID="$(lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -t 2>/dev/null | head -1 || true)"
  if [[ -z "$NEW_PID" ]] || ! kill -0 "$NEW_PID" 2>/dev/null; then
    return 1
  fi
  echo "$NEW_PID" >"$PID_FILE"
  return 0
}

restore_previous_slot_bin() {
  if [[ "$USE_SLOT_BIN" -eq 1 && -x "$SLOT_BIN_PREV" ]]; then
    echo "Restoring previous dev slot binary ..."
    mv -f "$SLOT_BIN_PREV" "$SLOT_BIN"
    return 0
  fi
  return 1
}

if [[ "$USE_SLOT_BIN" -eq 0 ]] && ! command -v ajax-cli >/dev/null 2>&1; then
  echo "ajax-cli not found on PATH after install" >&2
  exit 1
fi

RESTART_SCRIPT="$ROOT/scripts/dev-web-restart.sh"
export AJAX_WEB_RESTART_SCRIPT="$RESTART_SCRIPT"
export AJAX_WEB_RESTART_PROFILE="$PROFILE"
export AJAX_WEB_RESTART_PORT="$PORT"

BIN_PATH="${BIN_CMD[0]}"
if [[ "$USE_SLOT_BIN" -eq 0 ]]; then
  BIN_PATH="$(command -v ajax-cli)"
fi

# Build finished successfully before we touch the running process.
if [[ -n "$WORKTREE" ]]; then
  echo "AJAX_DEV_DEPLOY_PHASE=restarting"
fi
stop_tmux_session
stop_pid_file
stop_listener "$PORT"

if ! start_web "$BIN_PATH"; then
  echo "${PROFILE} web failed to start; see $LOG_FILE" >&2
  tail -20 "$LOG_FILE" >&2 || true
  tmux kill-session -t "$TMUX_SESSION" 2>/dev/null || true
  if restore_previous_slot_bin; then
    echo "Retrying previous ${PROFILE} web binary ..."
    stop_tmux_session
    stop_pid_file
    if start_web "$SLOT_BIN"; then
      echo "${PROFILE} web restored previous artifact (pid $(cat "$PID_FILE"), tmux $TMUX_SESSION)"
      echo "  URL:  https://127.0.0.1:$PORT"
      echo "  Log:  $LOG_FILE"
      exit 1
    fi
  fi
  exit 1
fi

# Health check against the local listener (not the Cloudflare URL).
if command -v curl >/dev/null 2>&1; then
  if ! curl -skf --max-time 5 "https://127.0.0.1:${PORT}/api/health" >/dev/null; then
    echo "${PROFILE} web started but /api/health failed; see $LOG_FILE" >&2
    tail -20 "$LOG_FILE" >&2 || true
    if restore_previous_slot_bin; then
      stop_tmux_session
      stop_pid_file
      start_web "$SLOT_BIN" || true
    fi
    exit 1
  fi
fi

echo "${PROFILE} web running (pid $(cat "$PID_FILE"), tmux $TMUX_SESSION)"
echo "  URL:  https://127.0.0.1:$PORT"
echo "  Log:  $LOG_FILE"
