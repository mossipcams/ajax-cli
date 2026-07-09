import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
const webRoot = join(testDir, "..");
const repoRoot = join(webRoot, "../../..");

describe("terminal ownership contract", () => {
  it("TERMINAL.md documents ownership and anti-patterns", () => {
    const body = readFileSync(join(webRoot, "TERMINAL.md"), "utf8");

    expect(body).toContain("viewport.ts");
    expect(body).toContain("terminalGeometry.ts");
    expect(body).toContain("terminalRefit.ts");
    expect(body).toContain("terminalGestures.ts");
    expect(body).toContain("terminalOutputPolicy.ts");
    expect(body).toContain("terminalConnection.ts");
    expect(body).toContain("TerminalRawView.svelte");
    expect(
      body.includes("FlushPending") || body.includes("one-shot"),
    ).toBe(true);
    expect(
      body.includes("failing test") || body.includes("Playwright"),
    ).toBe(true);
    expect(
      body.includes("Live/snapshot/composer") || body.includes("Live"),
    ).toBe(true);
  });

  it("architecture.md points at TERMINAL.md", () => {
    const body = readFileSync(join(repoRoot, "architecture.md"), "utf8");
    expect(
      body.includes("crates/ajax-web/web/TERMINAL.md") ||
        body.includes("`TERMINAL.md`"),
    ).toBe(true);
  });
});
