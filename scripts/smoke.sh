#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMPDIR_ROOT="${TMPDIR:-/tmp}"
WORKDIR="$(mktemp -d "${TMPDIR_ROOT%/}/ajax-smoke.XXXXXX")"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

if [[ -z "${AJAX_BIN:-}" ]]; then
  cargo build --release -p ajax-cli --manifest-path "$ROOT/Cargo.toml"
  BIN="$ROOT/target/release/ajax"
else
  BIN="$AJAX_BIN"
fi

if [[ ! -x "$BIN" ]]; then
  echo "ajax binary is not executable: $BIN" >&2
  exit 2
fi

FAKE_BIN="$WORKDIR/bin"
REPO="$WORKDIR/repos/web"
STATE="$WORKDIR/state/ajax.db"
CONFIG="$WORKDIR/config.toml"
BACKUP="$WORKDIR/state-backup.json"
WORKTREE="$REPO/.ajax-worktrees/fix-login"
mkdir -p "$FAKE_BIN" "$REPO" "$WORKTREE"

cat >"$CONFIG" <<EOF_CONFIG
[[repos]]
name = "web"
path = "$REPO"
default_branch = "main"

[[test_commands]]
repo = "web"
command = "true"
EOF_CONFIG

cat >"$FAKE_BIN/workmux" <<'EOF_WORKMUX'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  add|open|merge|remove)
    exit 0
    ;;
  *)
    echo "unexpected workmux command: $*" >&2
    exit 2
    ;;
esac
EOF_WORKMUX

cat >"$FAKE_BIN/tmux" <<'EOF_TMUX'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  attach-session|switch-client|new-window)
    exit 0
    ;;
  list-sessions)
    printf 'ajax-web-fix-login\n'
    ;;
  list-windows)
    printf 'worktrunk\t%s\n' "$AJAX_SMOKE_WORKTREE"
    ;;
  capture-pane)
    printf 'idle\n'
    ;;
  *)
    echo "unexpected tmux command: $*" >&2
    exit 2
    ;;
esac
EOF_TMUX

cat >"$FAKE_BIN/git" <<'EOF_GIT'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  *" status --porcelain=v1 --branch"*)
    printf '## ajax/fix-login\n'
    ;;
  *" merge-base --is-ancestor "*)
    exit 0
    ;;
  "diff --stat main...ajax/fix-login")
    printf ' smoke.rs | 1 +\n'
    ;;
  *)
    echo "unexpected git command: $*" >&2
    exit 2
    ;;
esac
EOF_GIT

cat >"$FAKE_BIN/codex" <<'EOF_CODEX'
#!/usr/bin/env bash
set -euo pipefail
printf '{"type":"started"}\n'
printf '{"type":"completed"}\n'
EOF_CODEX

chmod +x "$FAKE_BIN/workmux" "$FAKE_BIN/tmux" "$FAKE_BIN/git" "$FAKE_BIN/codex"

export PATH="$FAKE_BIN:$PATH"
export AJAX_CONFIG="$CONFIG"
export AJAX_STATE="$STATE"
export AJAX_SMOKE_WORKTREE="$WORKTREE"

ajax() {
  "$BIN" "$@"
}

echo "+ ajax doctor"
ajax doctor >/dev/null
echo "+ ajax repos"
ajax repos >/dev/null
echo "+ ajax tasks"
ajax tasks >/dev/null
echo "+ ajax new"
ajax new --repo web --title "fix login" --agent codex --execute >/dev/null
echo "+ ajax open"
ajax open web/fix-login --execute >/dev/null
echo "+ ajax check"
ajax check web/fix-login --execute >/dev/null
echo "+ ajax diff"
ajax diff web/fix-login --execute >/dev/null
echo "+ ajax merge"
ajax merge web/fix-login --execute --yes >/dev/null
echo "+ ajax reconcile"
ajax reconcile >/dev/null
echo "+ ajax clean"
ajax clean web/fix-login --execute --yes >/dev/null
echo "+ ajax cockpit"
ajax cockpit --json >/dev/null
echo "+ ajax state export"
ajax state export --output "$BACKUP" >/dev/null

test -s "$BACKUP"
echo "ajax smoke workflow passed"
