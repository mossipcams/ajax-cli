import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  CHANGED_FILE_LOC_FAIL_AT,
  CHANGED_FILE_LOC_WARN_AT,
  WARN_AT,
  FAIL_AT,
  PR_LOC_FAIL_AT,
  PR_LOC_WARN_AT,
  countLines,
  evaluateChangedFiles,
  evaluateChangedLoc,
  evaluateFileLoc,
  evaluatePrLoc,
  evaluateStagedFiles,
  formatAnnotation,
  inspectStagedFileLoc,
  isScannedSourcePath,
  parseNumstat,
  parseNameOnlyList,
} from "./check-file-loc.mjs";

test("countLines matches newline semantics", () => {
  assert.equal(countLines(""), 0);
  assert.equal(countLines("one"), 1);
  assert.equal(countLines("one\n"), 1);
  assert.equal(countLines("one\ntwo"), 2);
});

test("isScannedSourcePath accepts repo source files only", () => {
  assert.equal(isScannedSourcePath("crates/ajax-core/src/lib.rs"), true);
  assert.equal(isScannedSourcePath("crates/ajax-web/web/src/App.tsx"), true);
  assert.equal(isScannedSourcePath("crates/ajax-web/web/dist/app.js"), false);
  assert.equal(isScannedSourcePath("Cargo.lock"), false);
});

test("evaluateFileLoc warns at the warning threshold", () => {
  const [finding] = evaluateFileLoc("crates/foo.rs", WARN_AT);
  assert.equal(finding.level, "warning");
});

test("evaluateFileLoc fails at the hard limit", () => {
  const [finding] = evaluateFileLoc("crates/foo.rs", FAIL_AT);
  assert.equal(finding.level, "error");
});

test("evaluateFileLoc prefers error over warning at the hard limit", () => {
  assert.equal(evaluateFileLoc("crates/foo.rs", FAIL_AT + 50).length, 1);
  assert.equal(evaluateFileLoc("crates/foo.rs", FAIL_AT + 50)[0].level, "error");
});

test("evaluateChangedFiles reports only scanned changed files", () => {
  const findings = evaluateChangedFiles(
    ["crates/foo.rs", "Cargo.lock", "crates/ajax-web/web/dist/app.js"],
    () => FAIL_AT,
  );

  assert.equal(findings.length, 1);
  assert.equal(findings[0].path, "crates/foo.rs");
  assert.equal(findings[0].level, "error");
});

test("formatAnnotation includes the file path", () => {
  const annotation = formatAnnotation({
    level: "error",
    path: "crates/foo.rs",
    message: "too big",
  });
  assert.match(annotation, /^::error file=crates\/foo\.rs,line=1::/);
});

test("parseNameOnlyList trims git diff output", () => {
  assert.deepEqual(parseNameOnlyList("a.rs\n\nb.rs\n"), ["a.rs", "b.rs"]);
});

test("parseNumstat reads additions and deletions", () => {
  assert.deepEqual(parseNumstat("3\t2\tcrates/foo.rs\n-\t-\timage.png\n"), [
    { path: "crates/foo.rs", additions: 3, deletions: 2 },
  ]);
});

test("parseNumstat uses the destination path for git renames", () => {
  assert.deepEqual(
    parseNumstat(
      "4\t1\tcrates/ajax-web/web/src/features/{session => chat}/ChatSurface.tsx\n10\t0\told/SessionChat.tsx => new/ChatSurface.tsx\n",
    ),
    [
      {
        path: "crates/ajax-web/web/src/features/chat/ChatSurface.tsx",
        additions: 4,
        deletions: 1,
      },
      { path: "new/ChatSurface.tsx", additions: 10, deletions: 0 },
    ],
  );
});

test("changed file LOC warns and fails at its thresholds", () => {
  assert.equal(
    evaluateChangedLoc("crates/foo.rs", CHANGED_FILE_LOC_WARN_AT).level,
    "warning",
  );
  assert.equal(
    evaluateChangedLoc("crates/foo.rs", CHANGED_FILE_LOC_FAIL_AT).level,
    "error",
  );
});

test("PR LOC warns and fails at its thresholds", () => {
  assert.equal(evaluatePrLoc(PR_LOC_WARN_AT).level, "warning");
  assert.equal(evaluatePrLoc(PR_LOC_FAIL_AT).level, "error");
});

test("evaluateStagedFiles can skip PR aggregate LOC during merge commits", () => {
  const entries = [{ path: "crates/foo.rs", additions: PR_LOC_FAIL_AT, deletions: 0 }];
  const withAggregate = evaluateStagedFiles(entries, () => 1);
  const withoutAggregate = evaluateStagedFiles(entries, () => 1, {
    skipPrAggregate: true,
  });

  assert.ok(withAggregate.some((f) => f.path === "PR"));
  assert.ok(!withoutAggregate.some((f) => f.path === "PR"));
});

test("Husky pre-commit runs the staged LOC check", () => {
  assert.match(
    readFileSync(".husky/pre-commit", "utf8"),
    /node scripts\/check-file-loc\.mjs --staged/,
  );
});

test("staged LOC inspection reads indexed file contents", async () => {
  const result = await inspectStagedFileLoc({
    runGit: async (args) =>
      args[0] === "diff" ? "2\t1\tcrates/foo.rs\n" : "one\ntwo\n",
    readIndex: async () => "one\ntwo\n",
  });

  assert.deepEqual(result.files, ["crates/foo.rs"]);
  assert.deepEqual(result.warnings, []);
  assert.deepEqual(result.errors, []);
});
