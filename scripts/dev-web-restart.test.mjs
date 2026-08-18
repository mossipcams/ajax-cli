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
    /cargo install --path "\$ROOT\/crates\/ajax-cli" --locked --force/,
  );
});

test("worktree (Test in Dev) path also force-installs into the slot", () => {
  assert.match(
    script,
    /cargo install --path "\$SOURCE_ROOT\/crates\/ajax-cli" --locked --root "\$RUN_DIR" --force/,
  );
});

test("stale pid file after tmux stop is non-fatal and continues to start_web", () => {
  // Regression for 2026-08-18 Test in Stable abort: ajax-web-stable tmux was
  // already killed, then stop_pid_file refused pid 92911 (not ajax-cli web)
  // and exit 1 before start_web ran, leaving :8787 empty.
  assert.match(
    script,
    /stop_tmux_session\nstop_pid_file\nstop_listener "\$PORT"\n\nif ! start_web "\$BIN_PATH"/,
  );

  assert.match(
    script,
    /warning: stale pid file \$PID_FILE \(pid \$old_pid is not ajax-cli web/,
  );
  assert.doesNotMatch(
    script,
    /not an ajax-cli web process[\s\S]*exit 1/,
  );
  assert.match(script, /removing pid file without stopping that process/);
});

test("still stops living ajax-cli web pid-file processes", () => {
  assert.match(script, /Stopping previous \$\{PROFILE\} web \(pid \$old_pid\)/);
  assert.match(script, /kill "\$old_pid"/);
});

test("stable path snapshots cargo bin before install and can roll back on start failure", () => {
  assert.match(script, /CARGO_BIN_PREV="\$SLOT_BIN_DIR\/ajax-cli\.cargo\.prev"/);
  assert.match(
    script,
    /cp -f "\$\(command -v ajax-cli\)" "\$CARGO_BIN_PREV"/,
  );
  assert.match(script, /restore_previous_binary\(\)/);
  assert.match(
    script,
    /Restoring previous cargo-installed ajax-cli binary/,
  );
  assert.match(script, /if start_web "\$RESTORE_BIN"/);
});

test("Test in Dev slot-bin rollback remains wired", () => {
  assert.match(script, /SLOT_BIN_PREV="\$SLOT_BIN_DIR\/ajax-cli\.prev"/);
  assert.match(script, /cp -f "\$SLOT_BIN" "\$SLOT_BIN_PREV"/);
  assert.match(script, /Restoring previous dev slot binary/);
});
