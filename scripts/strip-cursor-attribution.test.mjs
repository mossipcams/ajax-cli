import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = join(dirname(fileURLToPath(import.meta.url)), "strip-cursor-attribution");

function strip(input) {
  return execFileSync(script, { encoding: "utf8", input });
}

test("strips Cursor footer and co-author lines", () => {
  const input = [
    "## Summary",
    "Fix the thing.",
    "",
    "Made with Cursor",
    "Made-with: Cursor",
    "Co-authored-by: Cursor <cursoragent@cursor.com>",
    "Co-authored-by: Matt <matt@example.com>",
    "",
  ].join("\n");

  assert.equal(
    strip(input),
    [
      "## Summary",
      "Fix the thing.",
      "",
      "Co-authored-by: Matt <matt@example.com>",
      "",
    ].join("\n"),
  );
});

test("leaves unrelated text unchanged", () => {
  const input = "Made with Love\nCo-authored-by: CursorPad <x@y.z>\n";
  assert.equal(strip(input), input);
});
