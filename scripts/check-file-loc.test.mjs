import assert from "node:assert/strict";
import test from "node:test";

import {
  WARN_AT,
  FAIL_AT,
  countLines,
  evaluateChangedFiles,
  evaluateFileLoc,
  formatAnnotation,
  isScannedSourcePath,
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
