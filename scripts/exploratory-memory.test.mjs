import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");

function writeMemoryFixtures({ memoryDelta, run, observations }) {
  const tmp = mkdtempSync(join(tmpdir(), "expl-memory-"));
  const resultsDir = join(tmp, "results");
  const memoryFile = join(tmp, "memory.json");
  mkdirSync(resultsDir, { recursive: true });
  writeFileSync(join(resultsDir, "memory-delta.json"), JSON.stringify(memoryDelta));
  writeFileSync(
    join(resultsDir, "findings.json"),
    JSON.stringify({ version: 1, findings: [] }),
  );
  writeFileSync(join(resultsDir, "observations.json"), JSON.stringify(observations));
  writeFileSync(join(resultsDir, "run.json"), JSON.stringify(run));
  return { resultsDir, memoryFile };
}

test("update-memory accepts object areasVisited and repoSha from real agent output", () => {
  const { resultsDir, memoryFile } = writeMemoryFixtures({
    memoryDelta: {
      version: 1,
      areasVisited: [
        { area: "cockpit", visits: 2, notes: "opened inbox" },
        { area: "new-task", visits: 1 },
      ],
      dullActions: [],
      confirmedFindingFingerprints: [],
      recommendedFocusNextRun: ["terminal", "diff-review"],
      notes: "gha run 31620356116 shape",
    },
    run: { version: 1, repoSha: "deadbeefcafebabe" },
    observations: {
      version: 1,
      observations: [
        { summary: "Settings sheet slow to open", area: "settings" },
        { summary: "Task card visible after failed create", area: "new-task" },
      ],
    },
  });

  const result = spawnSync(
    process.execPath,
    ["scripts/exploratory/update-memory.mjs"],
    {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        AJAX_EXPLORATORY_RESULTS: resultsDir,
        AJAX_EXPLORATORY_MEMORY: memoryFile,
      },
    },
  );
  assert.equal(result.status, 0, result.stderr);

  const memory = JSON.parse(readFileSync(memoryFile, "utf8"));
  assert.equal(memory.lastRunSha, "deadbeefcafebabe");
  assert.ok(memory.areas.cockpit.visits >= 1);
  assert.ok(memory.areas["new-task"].visits >= 1);
  assert.equal(memory.areas["[object Object]"], undefined);
  assert.ok(memory.observations.length >= 2);
  assert.deepEqual(memory.runs.at(-1).recommendedFocus, ["terminal", "diff-review"]);
  assert.ok(memory.runs.at(-1).observations >= 2);
});

test("update-memory underexplored prompt skips stale object keys", async () => {
  const { FINDING_AREAS } = await import("./exploratory/lib.mjs");
  const memory = {
    areas: {
      cockpit: { visits: 0, lastVisitedAt: null },
      "[object Object]": { visits: 99, lastVisitedAt: null },
    },
  };
  const underexplored = Object.entries(memory.areas ?? {})
    .filter(([area]) => FINDING_AREAS.has(area))
    .map(([area, info]) => ({ area, visits: info?.visits ?? 0 }))
    .sort((a, b) => a.visits - b.visits)
    .slice(0, 5);
  assert.ok(underexplored.every((item) => item.area !== "[object Object]"));
  assert.equal(underexplored[0].area, "cockpit");
});
