// @ts-nocheck
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const stylesSource = readFileSync(
  resolve(process.cwd(), "crates/ajax-web/web/src/styles.css"),
  "utf8",
);

describe("global styles", () => {
  it("keeps the app shell wide enough for the raw terminal", () => {
    expect(stylesSource).toMatch(/--shell:\s*640px/);
  });
});
