#!/usr/bin/env bash
# Runtime restart: restart only the currently installed web control plane.
#
# Detached from the live listener (same pattern as test-in-stable.sh) so the
# server process is not a child of the tmux session that gets killed mid-restart.
set -euo pipefail

if [[ -z "${AJAX_RUNTIME_RESTART_DETACHED:-}" ]]; then
  export AJAX_RUNTIME_RESTART_DETACHED=1
  exec perl -e 'use POSIX (); POSIX::setsid(); exec @ARGV or die "exec failed: $!"' -- "$0" "$@"
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESTART="$ROOT/scripts/dev-web-restart.sh"
LOG="$ROOT/.ajax-dev-web/runtime-restart.log"
SESSION="ajax-runtime-restart"

if [[ $# -eq 0 ]]; then
  set -- --profile dev
fi

mkdir -p "$(dirname "$LOG")"
exec </dev/null >>"$LOG" 2>&1

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for runtime restart" >&2
  exit 1
fi
if [[ ! -x "$RESTART" ]]; then
  echo "missing restart script: $RESTART" >&2
  exit 1
fi
if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "a runtime restart is already in progress (tmux session $SESSION)" >&2
  exit 1
fi

: >"$LOG"
CMD="PATH=$(printf %q "$PATH") $(printf '%q ' "$RESTART" --restart-only "$@")"
tmux new-session -d -s "$SESSION" -c "$ROOT" \
  "$CMD 2>&1 | tee -a $(printf %q "$LOG")"

echo "Runtime restart started in tmux session $SESSION"
echo "  Log: $LOG"
