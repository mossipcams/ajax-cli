#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${AJAX_TIS_DETACHED:-}" ]]; then
  export AJAX_TIS_DETACHED=1
  exec perl -e 'use POSIX (); POSIX::setsid(); exec @ARGV or die "exec failed: $!"' -- "$0" "$@"
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESTART="$ROOT/scripts/dev-web-restart.sh"
LOG="$ROOT/.ajax-dev-web/test-in-stable.log"
SESSION="ajax-test-in-stable"

if [[ $# -eq 0 ]]; then
  set -- --profile stable
fi

mkdir -p "$(dirname "$LOG")"
exec </dev/null >>"$LOG" 2>&1

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for Test in Stable" >&2
  exit 1
fi
if [[ ! -x "$RESTART" ]]; then
  echo "missing restart script: $RESTART" >&2
  exit 1
fi
if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "a Test in Stable run is already in progress (tmux session $SESSION)" >&2
  exit 1
fi

: >"$LOG"
CMD="PATH=$(printf %q "$PATH") $(printf '%q ' "$RESTART" "$@")"
tmux new-session -d -s "$SESSION" -c "$ROOT" \
  "$CMD 2>&1 | tee -a $(printf %q "$LOG")"

echo "Test in Stable started in tmux session $SESSION"
echo "  Log: $LOG"
