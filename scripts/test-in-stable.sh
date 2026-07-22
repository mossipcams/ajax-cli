#!/usr/bin/env bash
# Test in Stable: run the stable rebuild+restart detached from the web server.
#
# The button's server process lives in tmux session ajax-web-stable with its
# stdout piped to `tee`. Running dev-web-restart.sh as a child of that process
# means the restart kills its own logging pipe (tmux kill-session -> tee dies ->
# next echo takes SIGPIPE) before it ever reaches start_web, leaving stable
# down. So hand the work to a separate tmux session that nothing in the restart
# path kills, and never write to the inherited stdout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESTART="$ROOT/scripts/dev-web-restart.sh"
LOG="$ROOT/.ajax-dev-web/test-in-stable.log"
SESSION="ajax-test-in-stable"

if [[ $# -eq 0 ]]; then
  set -- --profile stable
fi

mkdir -p "$(dirname "$LOG")"
# Drop the caller's stdio: it is the dying server's tee pipe.
exec </dev/null >>"$LOG" 2>&1

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for Test in Stable" >&2
  exit 1
fi
if [[ ! -x "$RESTART" ]]; then
  echo "missing restart script: $RESTART" >&2
  exit 1
fi
# A live session means a rebuild is already in flight; stomping it would run two
# cargo installs against the same target dir.
if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "a Test in Stable run is already in progress (tmux session $SESSION)" >&2
  exit 1
fi

: >"$LOG"
# ponytail: PATH is pinned because the tmux server daemon may predate this
# shell and carry a leaner PATH than the one cargo/npm/git need.
CMD="PATH=$(printf %q "$PATH") $(printf '%q ' "$RESTART" "$@")"
tmux new-session -d -s "$SESSION" -c "$ROOT" \
  "$CMD 2>&1 | tee -a $(printf %q "$LOG")"

echo "Test in Stable started in tmux session $SESSION"
echo "  Log: $LOG"
