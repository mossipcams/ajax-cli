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

fail() {
  echo "$*" >&2
  exit 1
}

assert_json_contains() {
  local json="$1"
  local expected="$2"
  local label="$3"
  if [[ "$json" != *"$expected"* ]]; then
    printf 'expected %s to contain %s\nfull json:\n%s\n' "$label" "$expected" "$json" >&2
    exit 1
  fi
}

assert_output_contains() {
  local output="$1"
  local expected="$2"
  local label="$3"
  if [[ "$output" != *"$expected"* ]]; then
    printf 'expected %s to contain %s\nfull output:\n%s\n' "$label" "$expected" "$output" >&2
    exit 1
  fi
}

assert_log_contains() {
  local expected="$1"
  if ! grep -Fq "$expected" "$AJAX_SMOKE_COMMAND_LOG"; then
    printf 'expected command log to contain %s\nfull log:\n' "$expected" >&2
    cat "$AJAX_SMOKE_COMMAND_LOG" >&2
    exit 1
  fi
}

write_fake_tools() {
  mkdir -p "$FAKE_BIN"

  cat >"$FAKE_BIN/tmux" <<'EOF_TMUX'
#!/usr/bin/env bash
set -euo pipefail

printf 'tmux %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"

case "${1:-}" in
  new-session)
    if [[ -n "${AJAX_SMOKE_FAIL_AFTER_WORKTREE:-}" ]]; then
      echo "simulated tmux startup failure" >&2
      exit 42
    fi
    printf '1\n' > "$AJAX_SMOKE_SUBSTRATE_DIR/tmux-session"
    printf '1\n' > "$AJAX_SMOKE_SUBSTRATE_DIR/worktrunk-window"
    exit 0
    ;;
  new-window)
    printf '1\n' > "$AJAX_SMOKE_SUBSTRATE_DIR/worktrunk-window"
    exit 0
    ;;
  kill-window)
    rm -f "$AJAX_SMOKE_SUBSTRATE_DIR/worktrunk-window"
    exit 0
    ;;
  kill-session)
    rm -f "$AJAX_SMOKE_SUBSTRATE_DIR/tmux-session" "$AJAX_SMOKE_SUBSTRATE_DIR/worktrunk-window"
    exit 0
    ;;
  attach-session|switch-client|select-window|send-keys)
    exit 0
    ;;
  list-sessions)
    if [[ -f "$AJAX_SMOKE_SUBSTRATE_DIR/tmux-session" ]]; then
      printf 'ajax-web-fix-login\n'
    fi
    ;;
  list-windows)
    if [[ -f "$AJAX_SMOKE_SUBSTRATE_DIR/worktrunk-window" ]]; then
      printf 'worktrunk\t%s\n' "$AJAX_SMOKE_WORKTREE"
    fi
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

printf 'git %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"

case "$*" in
  *" worktree add "*)
    mkdir -p "$AJAX_SMOKE_WORKTREE"
    printf '1\n' > "$AJAX_SMOKE_SUBSTRATE_DIR/worktree"
    printf '1\n' > "$AJAX_SMOKE_SUBSTRATE_DIR/branch"
    ;;
  *" worktree remove "*)
    rm -rf "$AJAX_SMOKE_WORKTREE"
    rm -f "$AJAX_SMOKE_SUBSTRATE_DIR/worktree"
    ;;
  *" branch -d ajax/fix-login"|*" branch -D ajax/fix-login")
    rm -f "$AJAX_SMOKE_SUBSTRATE_DIR/branch"
    ;;
  *" switch main")
    exit 0
    ;;
  *" merge --ff-only ajax/fix-login")
    printf '1\n' > "$AJAX_SMOKE_SUBSTRATE_DIR/merged"
    ;;
  *" status --porcelain=v1 --branch"*)
    if [[ ! -f "$AJAX_SMOKE_SUBSTRATE_DIR/worktree" ]]; then
      echo "fatal: not a git repository: $AJAX_SMOKE_WORKTREE" >&2
      exit 128
    fi
    if [[ -f "$AJAX_SMOKE_SUBSTRATE_DIR/branch" ]]; then
      printf '## ajax/fix-login\n'
    else
      printf '## main\n'
    fi
    ;;
  *" merge-base --is-ancestor "*)
    if [[ -f "$AJAX_SMOKE_SUBSTRATE_DIR/merged" ]]; then
      exit 0
    fi
    exit 1
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
printf 'codex %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"
printf '{"type":"started"}\n'
printf '{"type":"completed"}\n'
EOF_CODEX

  chmod +x "$FAKE_BIN/tmux" "$FAKE_BIN/git" "$FAKE_BIN/codex"
}

configure_journey() {
  local name="$1"
  JOURNEY_DIR="$WORKDIR/$name"
  FAKE_BIN="$JOURNEY_DIR/bin"
  REPO="$JOURNEY_DIR/repos/web"
  STATE="$JOURNEY_DIR/state/ajax.db"
  CONFIG="$JOURNEY_DIR/config.toml"
  BACKUP="$JOURNEY_DIR/state-backup.json"
  WORKTREE="$JOURNEY_DIR/repos/web__worktrees/ajax-fix-login"
  AJAX_SMOKE_SUBSTRATE_DIR="$JOURNEY_DIR/substrate"
  AJAX_SMOKE_COMMAND_LOG="$JOURNEY_DIR/commands.log"

  mkdir -p "$REPO" "$AJAX_SMOKE_SUBSTRATE_DIR" "$(dirname "$STATE")"
  : > "$AJAX_SMOKE_COMMAND_LOG"

  cat >"$CONFIG" <<EOF_CONFIG
[[repos]]
name = "web"
path = "$REPO"
default_branch = "main"

[[test_commands]]
repo = "web"
command = "true"
EOF_CONFIG

  write_fake_tools

  export PATH="$FAKE_BIN:$ORIGINAL_PATH"
  export AJAX_CONFIG="$CONFIG"
  export AJAX_STATE="$STATE"
  export AJAX_SMOKE_WORKTREE="$WORKTREE"
  export AJAX_SMOKE_SUBSTRATE_DIR
  export AJAX_SMOKE_COMMAND_LOG
}

ajax() {
  "$BIN" "$@"
}

run_happy_path_journey() {
  configure_journey "happy"

  echo "+ ajax doctor"
  ajax doctor >/dev/null
  echo "+ ajax repos"
  ajax repos >/dev/null
  echo "+ ajax tasks"
  local tasks
  tasks="$(ajax tasks --json)"
  assert_json_contains "$tasks" '"tasks": []' "initial tasks"

  echo "+ ajax new"
  ajax new --repo web --title "fix login" --agent codex --execute >/dev/null
  tasks="$(ajax tasks --json)"
  assert_json_contains "$tasks" '"qualified_handle": "web/fix-login"' "tasks after new"
  assert_json_contains "$tasks" '"lifecycle_status": "Active"' "tasks after new"

  echo "+ ajax open"
  ajax open web/fix-login --execute >/dev/null
  assert_log_contains "tmux select-window -t ajax-web-fix-login:worktrunk"

  echo "+ ajax supervise --task"
  ajax supervise --task web/fix-login --prompt "finish task" --json >/dev/null
  tasks="$(ajax tasks --json)"
  assert_json_contains "$tasks" '"lifecycle_status": "Reviewable"' "tasks after agent completion"

  echo "+ ajax merge"
  ajax merge web/fix-login --execute --yes >/dev/null
  tasks="$(ajax tasks --json)"
  assert_json_contains "$tasks" '"lifecycle_status": "Merged"' "tasks after merge"
  assert_log_contains "git -C $REPO switch main"
  assert_log_contains "git -C $REPO merge --ff-only ajax/fix-login"

  echo "+ ajax clean"
  ajax clean web/fix-login --execute --yes >/dev/null
  tasks="$(ajax tasks --json)"
  assert_json_contains "$tasks" '"tasks": []' "tasks after clean"
  assert_log_contains "tmux kill-session -t ajax-web-fix-login"
  assert_log_contains "git -C $REPO worktree remove $WORKTREE"
  assert_log_contains "git -C $REPO branch -d ajax/fix-login"

  echo "+ ajax cockpit"
  local cockpit
  cockpit="$(ajax cockpit --json)"
  assert_json_contains "$cockpit" '"tasks": []' "cockpit after clean"

  echo "+ ajax state export"
  ajax state export --output "$BACKUP" >/dev/null
  test -s "$BACKUP"
}

run_recovery_journey() {
  configure_journey "recovery"

  echo "+ ajax new with simulated partial failure"
  export AJAX_SMOKE_FAIL_AFTER_WORKTREE=1
  local failure_log="$JOURNEY_DIR/new-failure.log"
  if ajax new --repo web --title "fix login" --agent codex --execute >"$failure_log" 2>&1; then
    fail "expected ajax new to fail after worktree creation"
  fi
  unset AJAX_SMOKE_FAIL_AFTER_WORKTREE

  assert_output_contains "$(cat "$failure_log")" "simulated tmux startup failure" "partial failure output"
  tasks="$(ajax tasks --json)"
  assert_json_contains "$tasks" '"qualified_handle": "web/fix-login"' "recovery tasks"
  assert_json_contains "$tasks" '"lifecycle_status": "Error"' "recovery tasks"
  assert_json_contains "$tasks" '"needs_attention": true' "recovery tasks"
  assert_log_contains "git -C $REPO worktree add -b ajax/fix-login $WORKTREE main"
  assert_log_contains "tmux new-session -d -s ajax-web-fix-login -n worktrunk -c $WORKTREE"

  echo "+ ajax recovery state export"
  ajax state export --output "$BACKUP" >/dev/null
  test -s "$BACKUP"

  local duplicate_export
  if duplicate_export="$(ajax state export --output "$BACKUP" 2>&1)"; then
    fail "expected duplicate state export to fail"
  fi
  assert_output_contains "$duplicate_export" "state export target already exists" "duplicate export"
}

ORIGINAL_PATH="$PATH"

run_happy_path_journey
run_recovery_journey

echo "ajax smoke workflow passed"
