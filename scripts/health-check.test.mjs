import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./health-check", import.meta.url));

const stamp = (hoursAgo) =>
  new Date(Date.now() - hoursAgo * 3600_000).toISOString().replace("Z", "123Z");

test("health-check counts signatures inside the window and sets the exit code", () => {
  const dir = mkdtempSync(join(tmpdir(), "health-check-"));
  try {
    const logPath = join(dir, "ajax.log");
    writeFileSync(
      logPath,
      [
        `${stamp(200)}  WARN ajax_web: session activity report failed handle="old"`,
        `${stamp(1)}  WARN ajax_web: session activity report failed handle="new"`,
        `${stamp(1)}  INFO ajax_web: session started handle="new"`,
      ].join("\n") + "\n",
    );
    const env = { ...process.env, AJAX_LOG: logPath };

    const found = spawnSync(process.execPath, [script, "168"], { env, encoding: "utf8" });
    assert.equal(found.status, 1);
    assert.match(found.stdout, /^activity_report_retry 1$/m);
    assert.match(found.stdout, /^agent_stop_timeout 0$/m);
    assert.match(found.stdout, /^transcript 0$/m);

    const empty = spawnSync(process.execPath, [script, "0.001"], { env, encoding: "utf8" });
    assert.equal(empty.status, 0);
    assert.match(empty.stdout, /^activity_report_retry 0$/m);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
