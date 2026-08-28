#!/usr/bin/env bash
# Runtime update: deploy origin/main to stable using safe-deploy mechanics.
#
# Same detached-wrapper pattern as test-in-stable.sh; never makes the live
# listener a child of kill-session.
set -euo pipefail

if [[ -z "${AJAX_RUNTIME_UPDATE_DETACHED:-}" ]]; then
  export AJAX_RUNTIME_UPDATE_DETACHED=1
  exec perl -e 'use POSIX (); POSIX::setsid(); exec @ARGV or die "exec failed: $!"' -- "$0" "$@"
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESTART="$ROOT/scripts/dev-web-restart.sh"
LOG="$ROOT/.ajax-dev-web/runtime-update.log"
SESSION="ajax-runtime-update"

if [[ $# -eq 0 ]]; then
  set -- --profile stable --port 8787
fi

mkdir -p "$(dirname "$LOG")"
exec </dev/null >>"$LOG" 2>&1

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for runtime update" >&2
  exit 1
fi
if [[ ! -x "$RESTART" ]]; then
  echo "missing restart script: $RESTART" >&2
  exit 1
fi
if tmux has-session -t "ajax-test-in-stable" 2>/dev/null; then
  echo "Test in Stable is already running; refuse concurrent cargo install" >&2
  exit 1
fi
if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "a runtime update is already in progress (tmux session $SESSION)" >&2
  exit 1
fi

: >"$LOG"
CMD="PATH=$(printf %q "$PATH") $(printf '%q ' "$RESTART" "$@")"
tmux new-session -d -s "$SESSION" -c "$ROOT" \
  "$CMD 2>&1 | tee -a $(printf %q "$LOG")"

echo "Runtime update started in tmux session $SESSION"
echo "  Log: $LOG"
