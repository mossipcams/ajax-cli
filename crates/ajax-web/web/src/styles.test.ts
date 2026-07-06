// @ts-nocheck
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const stylesSource = readFileSync(
  resolve(process.cwd(), "crates/ajax-web/web/src/styles.css"),
  "utf8",
);

describe("global styles", () => {
  it("runs the expanded desktop terminal overlay edge-to-edge", () => {
    // The ⛶ fullscreen overlay owns the whole viewport: no gutter around the
    // fixed layer, and no panel border/radius reading as a stray frame at the
    // screen edges. (The desktop overlay is the rule with `inset: 0`; the
    // mobile takeover anchors with top/left instead.)
    expect(stylesSource).toMatch(
      /html\.terminal-expanded \.task-detail \.terminal-primary\s*\{[^}]*inset:\s*0;[^}]*padding:\s*0/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded \.task-detail \.terminal-primary \.terminal-panel\s*\{[^}]*border:\s*none/,
    );
    expect(stylesSource).toMatch(
      /html\.terminal-expanded \.task-detail \.terminal-primary \.terminal-panel\s*\{[^}]*border-radius:\s*0/,
    );
  });
});
