import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizeDistBlankLines,
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

test("shouldNormalizeDistAsset applies only to dist JS and CSS bundles", () => {
  assert.equal(shouldNormalizeDistAsset("app.js"), true);
  assert.equal(shouldNormalizeDistAsset("app.css"), true);
  assert.equal(shouldNormalizeDistAsset("index.html"), false);
  assert.equal(shouldNormalizeDistAsset("terminal.js"), false);
});
