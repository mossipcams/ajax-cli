import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizeDistBlankLines,
  reconcileDistContents,
  shouldNormalizeDistAsset,
} from "./web-build-check.mjs";

test("normalizeDistBlankLines treats whitespace-only lines as empty", () => {
  const withTrailingSpaces = "a\n   \nb";
  const withEmptyBlankLine = "a\n\nb";

  assert.equal(normalizeDistBlankLines(withTrailingSpaces), withEmptyBlankLine);
  assert.equal(
    normalizeDistBlankLines(withTrailingSpaces),
    normalizeDistBlankLines(withEmptyBlankLine),
  );
});

test("normalizeDistBlankLines preserves trailing spaces on content lines", () => {
  const withContentTrailingSpaces = "code   \nnext";
  const withoutContentTrailingSpaces = "code\nnext";

  assert.equal(normalizeDistBlankLines(withContentTrailingSpaces), withContentTrailingSpaces);
  assert.notEqual(
    normalizeDistBlankLines(withContentTrailingSpaces),
    normalizeDistBlankLines(withoutContentTrailingSpaces),
  );
});

test("shouldNormalizeDistAsset applies to all tracked dist shell assets", () => {
  assert.equal(shouldNormalizeDistAsset("app.js"), true);
  assert.equal(shouldNormalizeDistAsset("app.css"), true);
  assert.equal(shouldNormalizeDistAsset("index.html"), true);
  assert.equal(shouldNormalizeDistAsset("terminal.js"), true);
  assert.equal(shouldNormalizeDistAsset("ghostty-vt.wasm"), false);
});

test("reconcileDistContents restores HEAD bytes when only blank-line whitespace differs", () => {
  const head = "a\n   \nb";
  const rebuilt = "a\n\nb";

  assert.equal(reconcileDistContents(rebuilt, head), head);
});

test("reconcileDistContents keeps a real content diff as normalized rebuild", () => {
  const head = "version-one\n   \n";
  const rebuilt = "version-two\n\n";

  assert.equal(reconcileDistContents(rebuilt, head), "version-two\n\n");
  assert.notEqual(reconcileDistContents(rebuilt, head), head);
});
