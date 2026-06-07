#!/usr/bin/env bash
set -euo pipefail

SCRIPT_SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/dev-web-restart.sh"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  [[ "$actual" == "$expected" ]] || fail "$message: expected '$expected', got '$actual'"
}

test_force_syncs_bare_repo_main_worktree() {
  local remote="$TMP_ROOT/remote.git"
  local seed="$TMP_ROOT/seed"
  local bare="$TMP_ROOT/bare"
  local main_worktree="$TMP_ROOT/main"
  local fake_bin="$TMP_ROOT/bin"

  git init --bare "$remote" >/dev/null
  git init -b main "$seed" >/dev/null
  git -C "$seed" config user.email test@example.com
  git -C "$seed" config user.name Test
  echo remote-v1 >"$seed/version.txt"
  git -C "$seed" add version.txt
  git -C "$seed" commit -m remote-v1 >/dev/null
  git -C "$seed" remote add origin "$remote"
  git -C "$seed" push -u origin main >/dev/null

  git clone --bare "$remote" "$bare" >/dev/null
  git -C "$bare" worktree add "$main_worktree" main >/dev/null
  git -C "$main_worktree" config user.email test@example.com
  git -C "$main_worktree" config user.name Test
  echo local-commit >"$main_worktree/local.txt"
  git -C "$main_worktree" add local.txt
  git -C "$main_worktree" commit -m local-only >/dev/null
  echo dirty >"$main_worktree/version.txt"
  echo untracked >"$main_worktree/untracked.txt"

  echo remote-v2 >"$seed/version.txt"
  git -C "$seed" add version.txt
  git -C "$seed" commit -m remote-v2 >/dev/null
  git -C "$seed" push origin main >/dev/null

  mkdir -p "$fake_bin"
  cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  cat >"$fake_bin/ajax-cli" <<'EOF'
#!/usr/bin/env bash
sleep 5
EOF
  chmod +x "$fake_bin/cargo" "$fake_bin/ajax-cli"
  mkdir -p "$bare/scripts"
  cp "$SCRIPT_SOURCE" "$bare/scripts/dev-web-restart.sh"

  PATH="$fake_bin:$PATH" "$bare/scripts/dev-web-restart.sh" --port 45991 >/dev/null

  assert_eq "$(git -C "$bare" rev-parse refs/remotes/origin/main)" \
    "$(git -C "$main_worktree" rev-parse HEAD)" \
    "main worktree should match origin/main"
  assert_eq "remote-v2" "$(cat "$main_worktree/version.txt")" \
    "tracked files should match origin/main"
  [[ ! -e "$main_worktree/local.txt" ]] || fail "local-only commit file should be removed"
  [[ ! -e "$main_worktree/untracked.txt" ]] || fail "untracked file should be removed"
}

test_refuses_to_kill_unmanaged_listener() {
  local fixture="$TMP_ROOT/listener-fixture"
  local fake_bin="$fixture/bin"
  local listener_pid
  local output
  local status
  local port=45992

  mkdir -p "$fixture/scripts" "$fake_bin"
  cp "$SCRIPT_SOURCE" "$fixture/scripts/dev-web-restart.sh"
  git -C "$fixture" init -b main >/dev/null
  git -C "$fixture" config user.email test@example.com
  git -C "$fixture" config user.name Test
  git -C "$fixture" add scripts/dev-web-restart.sh
  git -C "$fixture" commit -m fixture >/dev/null

  cat >"$fake_bin/ajax-cli" <<'EOF'
#!/usr/bin/env bash
sleep 5
EOF
  cat >"$fake_bin/lsof" <<'EOF'
#!/usr/bin/env bash
echo "$UNMANAGED_PID"
EOF
  chmod +x "$fake_bin/ajax-cli" "$fake_bin/lsof"

  python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1 &
  listener_pid=$!
  sleep 1

  set +e
  output="$(UNMANAGED_PID="$listener_pid" PATH="$fake_bin:$PATH" "$fixture/scripts/dev-web-restart.sh" \
    --no-pull --no-install --port "$port" 2>&1)"
  status=$?
  set -e

  [[ "$status" -ne 0 ]] || fail "script should refuse an unmanaged listener"
  kill -0 "$listener_pid" 2>/dev/null || fail "unmanaged listener should remain alive"
  [[ "$output" == *"refusing to stop unmanaged listener"* ]] ||
    fail "script should explain why it refused the listener"
  kill "$listener_pid"
}

test_force_syncs_bare_repo_main_worktree
echo "PASS: force-syncs bare repo main worktree"
test_refuses_to_kill_unmanaged_listener
echo "PASS: refuses to kill unmanaged listener"
