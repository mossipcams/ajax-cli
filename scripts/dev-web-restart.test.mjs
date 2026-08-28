import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const script = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "dev-web-restart.sh"),
  "utf8",
);

test("default (Test in Stable) path rebuilds web then force-installs ajax-cli", () => {
  assert.match(script, /rebuild_web\(\)/);
  assert.match(script, /npm --prefix "\$source_root" run web:build/);
  assert.match(script, /touch "\$embed_rs"/);

  // Non-worktree install must force-overwrite so a same-version embed refresh
  // always lands in ~/.cargo/bin (parity with husky + Test in Dev).
  assert.match(
    script,
    /cargo install --path "\$SOURCE_ROOT\/crates\/ajax-cli" --locked --force/,
  );
});

test("worktree (Test in Dev) path also force-installs into the slot", () => {
  assert.match(
    script,
    /cargo install --path "\$SOURCE_ROOT\/crates\/ajax-cli" --locked --root "\$RUN_DIR" --force/,
  );
});

test("Test in Stable uses a dedicated main worktree, not the host checkout", () => {
  assert.match(script, /AJAX_STABLE_MAIN_WORKTREE/);
  assert.match(script, /\[\[ ! -e "\$MAIN_WORKTREE\/\.git" \]\]/);
  assert.match(script, /git -C "\$REPO_ROOT" worktree add --detach/);
  assert.match(
    script,
    /git -C "\$MAIN_WORKTREE" reset --hard origin\/main/,
  );
  assert.match(script, /git -C "\$MAIN_WORKTREE" clean -fd/);
  assert.doesNotMatch(
    script,
    /git --git-dir=.*--work-tree=.*reset --hard/,
  );
  assert.doesNotMatch(script, /for-each-ref.*worktreepath.*refs\/heads\/main/);
  assert.doesNotMatch(
    script,
    /git --git-dir="\$GIT_DIR" --work-tree="\$REPO_ROOT" reset --hard/,
  );
});

test("pid files and logs stay on the host clone", () => {
  assert.match(script, /RUN_DIR="\$REPO_ROOT\/\.ajax-dev-web"/);
  assert.match(script, /PID_FILE="\$RUN_DIR/);
  assert.match(script, /LOG_FILE="\$RUN_DIR/);
});

test("stale pid file after tmux stop warns and continues", () => {
  assert.match(script, /warning: stale pid file/);
  assert.match(script, /warning: pid file .* points at non-web process/);
  assert.doesNotMatch(script, /refusing to stop pid-file process/);
});

test("failed stable start restores previous ~/.cargo/bin/ajax-cli snapshot", () => {
  assert.match(script, /CARGO_BIN_PREV/);
  assert.match(script, /restore_previous_cargo_bin/);
  assert.match(script, /Restoring previous ~\/\.cargo\/bin\/ajax-cli/);
});

test("first-time dedicated worktree reinstalls agent hooks", () => {
  assert.match(script, /\[\[ -z "\$PREV_HEAD" \]\]/);
  assert.match(script, /HOOKS_CHANGED=1/);
});

test("build and install complete before stop_tmux_session", () => {
  const installBlock = script.indexOf('if [[ "$INSTALL" -eq 1 ]]; then');
  const stopBlock = script.indexOf("stop_tmux_session");
  assert.ok(installBlock >= 0 && stopBlock > installBlock);
});

test("restart-only skips fetch, build, and install", () => {
  const restartOnlyBlock = script.indexOf('if [[ "$RESTART_ONLY" -eq 1 ]]; then');
  const fetchPatch = script.indexOf('runtime_status_patch "fetching"');
  assert.ok(restartOnlyBlock >= 0);
  assert.ok(
    restartOnlyBlock < fetchPatch,
    "restart-only branch must precede fetch/build path",
  );
  assert.match(script, /Restart-only: skipping fetch\/build\/install/);
  assert.doesNotMatch(
    script.slice(restartOnlyBlock, script.indexOf("elif [[ -n \"$WORKTREE\" ]]")),
    /rebuild_web/,
  );
});

test("tmux targeting uses exact ajax-web profile session only", () => {
  assert.match(script, /TMUX_SESSION="ajax-web-\$\{PROFILE\}"/);
  assert.match(script, /tmux kill-session -t "\$TMUX_SESSION"/);
  assert.match(script, /tmux new-session -d -s "\$TMUX_SESSION"/);
  assert.doesNotMatch(script, /ajax-web-task/);
  assert.doesNotMatch(script, /tmux ls/);
});

test("failed deploy records rolled_back in durable status", () => {
  assert.match(script, /runtime_status_patch "rolled_back" "rolled_back" "true"/);
});

test("runtime_status_log writes JSONL only when AJAX_RUNTIME_LOG_FILE is set", () => {
  assert.match(
    script,
    /runtime_status_log\(\) \{[\s\S]*?\[\[ -n "\$line" && -n "\$\{AJAX_RUNTIME_LOG_FILE:-\}" \]\]/,
  );
  assert.match(script, /runtime_status_patch\(\) \{[\s\S]*?\[\[ -n "\$STATUS_FILE" \]\]/);
});
