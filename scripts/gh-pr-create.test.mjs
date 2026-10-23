// Regression coverage for the merged/closed PR short-circuit in scripts/gh-pr-create.
//
// Issue #1267: `gh pr view <branch>` matches any state, so a previously merged
// or closed PR for the same branch name made the script short-circuit and
// never reach `gh pr create`. The fix narrows the existence check to OPEN
// pull requests only. These tests build an isolated fake `bin` on PATH with
// stub `git` and `gh` executables that record every invocation, so the
// regression scenario can be reproduced without touching the real repo.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = join(dirname(fileURLToPath(import.meta.url)), "gh-pr-create");

const GIT_STUB = `#!/usr/bin/env bash
echo "git $*" >> "$LOG_FILE"
case "$1" in
  branch) echo "feat/demo/thing" ;;
  fetch) : ;;
  status) : ;;
  log) echo "abc123 Some commit" ;;
  push) : ;;
  *) : ;;
esac
`;

// The gh stub records every invocation. For `gh pr view <branch>` it returns
// success with a URL (matching the pre-fix short-circuit path). For
// `gh pr list --head <branch> --state open` it returns either an empty
// payload (regression scenario) or a URL (open PR already exists). The
// REGRESSION env var is set by the test to pick the scenario.
const GH_STUB = `#!/usr/bin/env bash
echo "gh $*" >> "$LOG_FILE"
case "$2" in
  view)
    if [[ "$3" == https://* ]]; then
      echo "Simple PR body"
    else
      echo '{"url":"https://github.com/owner/repo/pull/123","body":"Simple PR body"}'
    fi
    ;;
  list)
    if [[ "\${REGRESSION:-0}" == "1" ]]; then
      :
    else
      echo "https://github.com/owner/repo/pull/123"
    fi
    ;;
  create)
    echo "https://github.com/owner/repo/pull/456"
    ;;
  edit) : ;;
  *) : ;;
esac
`;

function setupStubs(t, regression) {
  const dir = mkdtempSync(join(tmpdir(), "gh-pr-create-test-"));
  const binDir = join(dir, "bin");
  const logFile = join(dir, "log.txt");
  mkdirSync(binDir, { recursive: true });
  writeFileSync(join(binDir, "git"), GIT_STUB, { mode: 0o755 });
  writeFileSync(join(binDir, "gh"), GH_STUB, { mode: 0o755 });
  t.after(() => {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // best-effort cleanup
    }
  });
  return { binDir, logFile };
}

function runScript({ binDir, logFile, regression }) {
  return execFileSync("bash", [script], {
    env: {
      ...process.env,
      PATH: `${binDir}:${process.env.PATH}`,
      LOG_FILE: logFile,
      REGRESSION: regression ? "1" : "0",
    },
    encoding: "utf8",
  });
}

test("creates a new PR when only a merged or closed PR exists for the branch", (t) => {
  const ctx = setupStubs(t, true);
  const output = runScript({ ...ctx, regression: true });
  const log = readFileSync(ctx.logFile, "utf8");

  assert.match(
    log,
    /^gh pr create\b/m,
    `expected the script to invoke \`gh pr create\` after the fix, log was:\n${log}`,
  );
  assert.match(
    output,
    /https:\/\/github\.com\/owner\/repo\/pull\/456/,
    `expected the new PR URL in the script output, got:\n${output}`,
  );
  assert.match(
    output,
    /Created PR: feat\(demo\): thing/,
    `expected the "Created PR:" line in the script output, got:\n${output}`,
  );
});

test("prints the existing URL and skips create when an open PR exists", (t) => {
  const ctx = setupStubs(t, false);
  const output = runScript({ ...ctx, regression: false });
  const log = readFileSync(ctx.logFile, "utf8");

  assert.doesNotMatch(
    log,
    /^gh pr create\b/m,
    `expected the script NOT to invoke \`gh pr create\` when an open PR exists, log was:\n${log}`,
  );
  assert.match(
    output,
    /https:\/\/github\.com\/owner\/repo\/pull\/123/,
    `expected the existing PR URL in the script output, got:\n${output}`,
  );
  assert.doesNotMatch(
    output,
    /Created PR:/,
    `expected no "Created PR:" line when reusing an existing open PR, got:\n${output}`,
  );
});
