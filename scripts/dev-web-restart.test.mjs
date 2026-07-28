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
