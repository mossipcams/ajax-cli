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

  it("sizes expanded mobile terminal to the visual viewport horizontal band", () => {
    expect(stylesSource).toMatch(
      /html\.terminal-expanded \.task-detail \.terminal-primary\s*\{[^}]*left:\s*var\(--app-left,\s*0px\)/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded \.task-detail \.terminal-primary\s*\{[^}]*width:\s*var\(--app-width,\s*100vw\)/,
    );
    expect(stylesSource).not.toMatch(
      /html\.terminal-expanded \.task-detail \.terminal-primary\s*\{[^}]*right:\s*0/,
    );
  });
});
