#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="$REPO_ROOT/.ajax-dev-web"
REPO_BASENAME="$(basename "$REPO_ROOT")"
MAIN_WORKTREE="${AJAX_STABLE_MAIN_WORKTREE:-$HOME/.ajax-dev/worktrees/${REPO_BASENAME}-main}"

PROFILE="dev"
HOST="0.0.0.0"
PORT=""
INSTALL=1
FOREGROUND=0
WORKTREE=""

usage() {
  cat <<'EOF'
Usage: scripts/dev-web-restart.sh [OPTIONS]

Default (Test in Stable): fetch origin/main from the host clone, force-sync a
dedicated detached main worktree to origin/main (branch tip, not a release
tag), run npm web:build from that tree, install ajax-cli into ~/.cargo/bin
(unless --no-install), reinstall client agent hooks when agent_hooks.rs
changed, stop the previous managed web server for the selected profile, and
start ajax-cli web as launchd job com.ajax.web.<profile>. Never resets the host
checkout.

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

Environment:
  AJAX_STABLE_MAIN_WORKTREE  Dedicated main worktree for Test in Stable
                             (default: ~/.ajax-dev/worktrees/<repo-basename>-main)

Background mode runs under launchd job com.ajax.web.<profile> (ajax web service ...).
Background logs: ~/Library/Logs/ajax-web-<profile>.log
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

if [[ -z "$PORT" ]]; then
  case "$PROFILE" in
    stable) PORT="8787" ;;
    *) PORT="8788" ;;
  esac
fi

if [[ -n "$WORKTREE" && "$PROFILE" == "stable" ]]; then
  echo "refusing --worktree deploy for profile=stable" >&2
  exit 2
fi

PID_FILE="$RUN_DIR/${PROFILE}-web.pid"
LOG_FILE="$RUN_DIR/${PROFILE}-web.log"
TMUX_SESSION="ajax-web-${PROFILE}"
LAUNCHD_LABEL="com.ajax.web.${PROFILE}"
SERVICE_LOG="$HOME/Library/Logs/ajax-web-${PROFILE}.log"
SLOT_BIN_DIR="$RUN_DIR/bin"
SLOT_BIN="$SLOT_BIN_DIR/ajax-cli"
SLOT_BIN_PREV="$SLOT_BIN_DIR/ajax-cli.prev"
CARGO_BIN_DIR="$RUN_DIR/cargo-bin"
CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin/ajax-cli"
CARGO_BIN_PREV="$CARGO_BIN_DIR/ajax-cli.prev"

RESTART_LOCK="$RUN_DIR/${PROFILE}-restart.lock"

release_restart_lock() {
  rm -rf "$RESTART_LOCK"
}

acquire_restart_lock() {
  local holder
  mkdir -p "$RUN_DIR"
  if ! mkdir "$RESTART_LOCK" 2>/dev/null; then
    holder="$(cat "$RESTART_LOCK/pid" 2>/dev/null || true)"
    if [[ -n "$holder" ]] && kill -0 "$holder" 2>/dev/null; then
      echo "restart already in progress for ${PROFILE} (pid $holder)" >&2
      exit 1
    fi
    echo "warning: stale restart lock $RESTART_LOCK (pid ${holder:-unknown} not running); taking over" >&2
    rm -rf "$RESTART_LOCK"
    if ! mkdir "$RESTART_LOCK" 2>/dev/null; then
      echo "restart already in progress for ${PROFILE}" >&2
      exit 1
    fi
  fi
  echo "$$" >"$RESTART_LOCK/pid"
  trap release_restart_lock EXIT
}

acquire_restart_lock

ensure_main_worktree() {
  echo "Fetching origin/main from $REPO_ROOT ..."
  git -C "$REPO_ROOT" fetch origin main:refs/remotes/origin/main
  if [[ ! -e "$MAIN_WORKTREE/.git" ]]; then
    mkdir -p "$(dirname "$MAIN_WORKTREE")"
    echo "Creating dedicated main worktree at $MAIN_WORKTREE ..."
    git -C "$REPO_ROOT" worktree add --detach "$MAIN_WORKTREE" origin/main
  fi
}

sync_main() {
  ensure_main_worktree
  echo "Force-syncing dedicated main worktree to origin/main ..."
  git -C "$MAIN_WORKTREE" reset --hard origin/main
  git -C "$MAIN_WORKTREE" clean -fd
  echo "main tip: $(git -C "$MAIN_WORKTREE" log -1 --oneline)"
}

restore_model_router_symlinks() {
  local router="${AJAX_MODEL_ROUTER:-}"
  local candidate installer
  if [[ -z "$router" ]]; then
    for candidate in \
      "${HOME}/Projects/ajax-model-router" \
      "/Users/matt/Projects/ajax-model-router"; do
      if [[ -x "${candidate}/scripts/install-symlinks" ]]; then
        router="$candidate"
        break
      fi
    done
  fi
  installer="${router:+$router/scripts/install-symlinks}"
  if [[ -z "$installer" || ! -x "$installer" ]]; then
    return 0
  fi
  echo "Restoring ajax-model-router symlinks in $MAIN_WORKTREE ..."
  "$installer" --target "$MAIN_WORKTREE" --force
}

HOOKS_PATH="crates/ajax-cli/src/agent_hooks.rs"
HOOKS_CHANGED=0

agent_hooks_differs() {
  local work_tree="$1"
  shift
  if ! git -C "$work_tree" diff --quiet "$@" -- "$HOOKS_PATH"; then
    return 0
  fi
  return 1
}

SOURCE_ROOT="$MAIN_WORKTREE"
if [[ -n "$WORKTREE" ]]; then
  if [[ ! -d "$WORKTREE" ]]; then
    echo "worktree path does not exist: $WORKTREE" >&2
    exit 1
  fi
  SOURCE_ROOT="$(cd "$WORKTREE" && pwd)"
  echo "AJAX_DEV_DEPLOY_PHASE=building"
  echo "Test in Dev: building from worktree $SOURCE_ROOT (no git sync)"
  if agent_hooks_differs "$SOURCE_ROOT" origin/main \
    || agent_hooks_differs "$SOURCE_ROOT" HEAD; then
    HOOKS_CHANGED=1
  fi
else
  PREV_HEAD="$(git -C "$MAIN_WORKTREE" rev-parse HEAD 2>/dev/null || true)"
  sync_main
  if [[ -z "$PREV_HEAD" ]]; then
    HOOKS_CHANGED=1
  elif agent_hooks_differs "$MAIN_WORKTREE" "$PREV_HEAD" HEAD; then
    HOOKS_CHANGED=1
  fi
  restore_model_router_symlinks
fi

mkdir -p "$RUN_DIR"

BIN_CMD=(ajax-cli)
USE_SLOT_BIN=0

rebuild_web() {
  local source_root="$1"
  local embed_rs="$source_root/crates/ajax-web/src/adapters/assets.rs"
  export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
  if [[ -s "$NVM_DIR/nvm.sh" ]]; then
    # shellcheck source=/dev/null
    . "$NVM_DIR/nvm.sh"
    nvm use 22 >/dev/null
  fi
  echo "Installing npm deps in $source_root (npm ci) ..."
  npm --prefix "$source_root" ci
  echo "Building frontend (web:build) in $source_root ..."
  npm --prefix "$source_root" run web:build
  if [[ ! -f "$embed_rs" ]]; then
    echo "missing embed source: $embed_rs" >&2
    exit 1
  fi
  touch "$embed_rs"
}

if [[ "$INSTALL" -eq 1 ]]; then
  if [[ -n "$WORKTREE" ]]; then
    rebuild_web "$SOURCE_ROOT"

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
    rebuild_web "$SOURCE_ROOT"
    mkdir -p "$CARGO_BIN_DIR"
    if [[ -x "$CARGO_BIN" ]]; then
      cp -f "$CARGO_BIN" "$CARGO_BIN_PREV"
    fi
    echo "Installing ajax-cli from $SOURCE_ROOT ..."
    cargo install --path "$SOURCE_ROOT/crates/ajax-cli" --locked --force
  fi
elif [[ -n "$WORKTREE" && -x "$SLOT_BIN" ]]; then
  BIN_CMD=("$SLOT_BIN")
  USE_SLOT_BIN=1
fi

if [[ "$HOOKS_CHANGED" -eq 1 ]]; then
  if [[ "$INSTALL" -eq 1 ]]; then
    echo "Agent hook definitions changed; reinstalling client hooks ..."
    if [[ "$USE_SLOT_BIN" -eq 1 ]]; then
      "$SLOT_BIN" agent-hooks install
    else
      ajax-cli agent-hooks install
    fi
  else
    echo "warning: agent hook definitions changed but --no-install; skipping hook reinstall" >&2
  fi
fi

stop_listener() {
  local port="$1"
  local pids
  pids="$(/usr/sbin/lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)"
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
  if [[ -z "$old_pid" ]]; then
    echo "warning: stale empty pid file $PID_FILE; removing" >&2
    rm -f "$PID_FILE"
    return 0
  fi
  if ! kill -0 "$old_pid" 2>/dev/null; then
    echo "warning: stale pid file $PID_FILE (pid $old_pid not running); removing" >&2
    rm -f "$PID_FILE"
    return 0
  fi
  old_command="$(ps -p "$old_pid" -o command= 2>/dev/null || true)"
  if [[ "$old_command" != *ajax-cli* || "$old_command" != *web* ]]; then
    echo "warning: pid file $PID_FILE points at non-web process $old_pid; removing" >&2
    rm -f "$PID_FILE"
    return 0
  fi
  echo "Stopping previous ${PROFILE} web (pid $old_pid) ..."
  kill "$old_pid" 2>/dev/null || true
  sleep 1
  if kill -0 "$old_pid" 2>/dev/null; then
    kill -9 "$old_pid" 2>/dev/null || true
  fi
  rm -f "$PID_FILE"
}

stop_launchd_job() {
  launchctl bootout "gui/$(id -u)/$LAUNCHD_LABEL" >/dev/null 2>&1 || true
}

stop_tmux_session() {
  if ! command -v tmux >/dev/null 2>&1; then
    return 0
  fi
  if tmux has-session -t "=$TMUX_SESSION" 2>/dev/null; then
    echo "Stopping tmux session $TMUX_SESSION ..."
    tmux kill-session -t "=$TMUX_SESSION"
    sleep 1
  fi
}

start_web() {
  local bin_path="$1"
  local cmd_display=("$bin_path" --profile "$PROFILE" web --host "$HOST" --port "$PORT")

  if [[ "$FOREGROUND" -eq 1 ]]; then
    echo "Starting ${cmd_display[*]} (foreground) ..."
    release_restart_lock
    exec "$bin_path" --profile "$PROFILE" web --host "$HOST" --port "$PORT"
  fi

  echo "Starting ${cmd_display[*]} (launchd job $LAUNCHD_LABEL) ..."
  mkdir -p "$(dirname "$SERVICE_LOG")"
  echo "=== dev-web-restart $(date '+%F %T') starting ${PROFILE} ===" >>"$SERVICE_LOG"
  if ! (cd "$REPO_ROOT" && "$bin_path" --profile "$PROFILE" web service install --host "$HOST" --port "$PORT"); then
    echo "launchd install failed for $LAUNCHD_LABEL" >&2
    return 1
  fi

  sleep 2
  NEW_PID="$(/usr/sbin/lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -t 2>/dev/null | /usr/bin/head -1 || true)"
  if [[ -z "$NEW_PID" ]] || ! kill -0 "$NEW_PID" 2>/dev/null; then
    return 1
  fi
  echo "$NEW_PID" >"$PID_FILE"
  return 0
}

restore_previous_binary() {
  if [[ "$USE_SLOT_BIN" -eq 1 && -x "$SLOT_BIN_PREV" ]]; then
    echo "Restoring previous dev slot binary ..."
    mv -f "$SLOT_BIN_PREV" "$SLOT_BIN"
    RESTORE_BIN="$SLOT_BIN"
    return 0
  fi
  if [[ "$USE_SLOT_BIN" -eq 0 && -x "$CARGO_BIN_PREV" ]]; then
    local cargo_bin
    cargo_bin="$(command -v ajax-cli 2>/dev/null || echo "$HOME/.cargo/bin/ajax-cli")"
    echo "Restoring previous cargo-installed ajax-cli binary ..."
    cp -f "$CARGO_BIN_PREV" "$cargo_bin"
    RESTORE_BIN="$cargo_bin"
    return 0
  fi
  return 1
}

restore_previous_cargo_bin() {
  if [[ "$USE_SLOT_BIN" -eq 0 && -x "$CARGO_BIN_PREV" ]]; then
    echo "Restoring previous ~/.cargo/bin/ajax-cli ..."
    cp -f "$CARGO_BIN_PREV" "$CARGO_BIN"
    return 0
  fi
  return 1
}

if [[ "$USE_SLOT_BIN" -eq 0 ]] && ! command -v ajax-cli >/dev/null 2>&1; then
  echo "ajax-cli not found on PATH after install" >&2
  exit 1
fi

RESTART_SCRIPT="$REPO_ROOT/scripts/dev-web-restart.sh"
export AJAX_WEB_RESTART_SCRIPT="$RESTART_SCRIPT"
export AJAX_WEB_RESTART_PROFILE="$PROFILE"
export AJAX_WEB_RESTART_PORT="$PORT"

BIN_PATH="${BIN_CMD[0]}"
if [[ "$USE_SLOT_BIN" -eq 0 ]]; then
  BIN_PATH="$(command -v ajax-cli)"
fi

if [[ -n "$WORKTREE" ]]; then
  echo "AJAX_DEV_DEPLOY_PHASE=restarting"
fi
OLD_PID=""
if [[ -f "$PID_FILE" ]]; then
  OLD_PID="$(cat "$PID_FILE" 2>/dev/null || true)"
fi
if [[ -z "$OLD_PID" ]]; then
  OLD_PID="$(/usr/sbin/lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -t 2>/dev/null | /usr/bin/head -1 || true)"
fi
stop_tmux_session
stop_launchd_job
stop_pid_file
stop_listener "$PORT"

if ! start_web "$BIN_PATH"; then
  echo "${PROFILE} web failed to start; see $SERVICE_LOG" >&2
  tail -20 "$SERVICE_LOG" >&2 || true
  stop_launchd_job
  if restore_previous_binary; then
    echo "Retrying previous ${PROFILE} web binary ..."
    stop_tmux_session
    stop_launchd_job
    stop_pid_file
    if start_web "$RESTORE_BIN"; then
      echo "${PROFILE} web restored previous artifact (pid $(cat "$PID_FILE"), launchd $LAUNCHD_LABEL)"
      echo "  URL:  https://127.0.0.1:$PORT"
      echo "  Log:  $SERVICE_LOG"
      exit 1
    fi
  elif restore_previous_cargo_bin; then
    echo "Retrying previous ${PROFILE} web binary ..."
    stop_tmux_session
    stop_launchd_job
    stop_pid_file
    if start_web "$CARGO_BIN"; then
      echo "${PROFILE} web restored previous artifact (pid $(cat "$PID_FILE"), launchd $LAUNCHD_LABEL)"
      echo "  URL:  https://127.0.0.1:$PORT"
      echo "  Log:  $SERVICE_LOG"
      exit 1
    fi
  fi
  exit 1
fi

NEW_PID="$(cat "$PID_FILE" 2>/dev/null || true)"
if [[ -n "$OLD_PID" && -n "$NEW_PID" && "$OLD_PID" == "$NEW_PID" ]]; then
  echo "${PROFILE} web restart did not replace process (still pid $OLD_PID); see $SERVICE_LOG" >&2
  exit 1
fi

if command -v curl >/dev/null 2>&1; then
  if ! curl -skf --max-time 5 "https://127.0.0.1:${PORT}/api/health" >/dev/null; then
    echo "${PROFILE} web started but /api/health failed; see $SERVICE_LOG" >&2
    tail -20 "$SERVICE_LOG" >&2 || true
    if restore_previous_binary; then
      stop_tmux_session
      stop_launchd_job
      stop_pid_file
      start_web "$RESTORE_BIN" || true
    elif restore_previous_cargo_bin; then
      stop_tmux_session
      stop_launchd_job
      stop_pid_file
      start_web "$CARGO_BIN" || true
    fi
    exit 1
  fi
fi

echo "${PROFILE} web running (pid $(cat "$PID_FILE"), launchd $LAUNCHD_LABEL)"
echo "  URL:  https://127.0.0.1:$PORT"
echo "  Log:  $SERVICE_LOG"
