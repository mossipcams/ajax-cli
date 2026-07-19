import { describe, it, expect } from "vitest";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
const webRoot = join(testDir, "..");
const repoRoot = join(webRoot, "../../..");

const OLD_PATHS = [
  "crates/ajax-web/web/src/components/TerminalRawView.svelte",
  "crates/ajax-web/web/src/components/TerminalSurfaceSelector.svelte",
  "crates/ajax-web/web/src/components/XtermTerminalView.svelte",
  "crates/ajax-web/web/src/components/TerminalRawView.test.ts",
  "crates/ajax-web/web/src/components/TerminalSurfaceSelector.test.ts",
  "crates/ajax-web/web/src/components/XtermTerminalView.test.ts",
  "crates/ajax-web/web/src/terminalPreload.ts",
  "crates/ajax-web/web/src/terminalPreload.test.ts",
  "crates/ajax-web/web/src/terminalSurfaceSetting.ts",
  "crates/ajax-web/web/src/terminalSurfaceSetting.test.ts",
  "crates/ajax-web/web/src/terminalGestures.ts",
  // terminalGeometry.ts/terminalRefit.ts (+tests) were legacy Ghostty-era
  // names, but the 2026-07 web-architecture-alignment plan reintroduced those
  // paths as the current geometry/refit owners, so they are no longer legacy.
  "crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts",
  "crates/ajax-web/web/src/terminalOutputPolicy.ts",
  "crates/ajax-web/web/src/terminalOutputPolicy.test.ts",
  "crates/ajax-web/web/src/terminalLayoutPolicy.ts",
  "crates/ajax-web/web/src/terminalLayoutPolicy.test.ts",
  "crates/ajax-web/web/src/terminalZeroLag.ts",
  "crates/ajax-web/web/src/terminalZeroLag.test.ts",
  "crates/ajax-web/web/src/terminalClipboard.ts",
  "crates/ajax-web/web/src/terminalClipboard.test.ts",
  "crates/ajax-web/web/src/terminalSelection.test.ts",
  "crates/ajax-web/web/src/terminalTouchScroll.test.ts",
  "crates/ajax-web/web/e2e/terminal-scroll.test.ts",
  "crates/ajax-web/web/e2e/terminal-scroll-garble.test.ts",
  "crates/ajax-web/web/e2e/terminal-zero-lag.test.ts",
  "crates/ajax-web/web/e2e/fullscreen-refit.test.ts",
  "scripts/ios-terminal-smoke.mjs",
  "crates/ajax-web/web/dist/ghostty-vt.wasm",
] as const;

function readRepoFile(relativePath: string): string {
  return readFileSync(join(repoRoot, relativePath), "utf8");
}

function collectSymbolViolations(
  relativePath: string,
  symbols: readonly string[],
): string[] {
  const body = readRepoFile(relativePath);
  return symbols
    .filter((symbol) => body.includes(symbol))
    .map((symbol) => `${relativePath}: contains ${symbol}`);
}

describe("legacy terminal removal hygiene", () => {
  it("removes obsolete Ghostty and Surface V2 paths and wiring", () => {
    const violations: string[] = [];

    for (const relativePath of OLD_PATHS) {
      if (existsSync(join(repoRoot, relativePath))) {
        violations.push(`path still exists: ${relativePath}`);
      }
    }

    const packageJson = readRepoFile("package.json");
    if (packageJson.includes('"ghostty-web"')) {
      violations.push('package.json: contains dependency "ghostty-web"');
    }

    violations.push(
      ...collectSymbolViolations(
        "crates/ajax-web/web/src/features/task/TaskDetail.tsx",
        ["TerminalSurfaceSelector"],
      ),
      ...collectSymbolViolations("crates/ajax-web/web/src/app/App.tsx", [
        "terminalPreload",
      ]),
      ...collectSymbolViolations(
        "crates/ajax-web/web/src/features/settings/SettingsView.tsx",
        ["surfaceV2", "Terminal Surface V2"],
      ),
      ...collectSymbolViolations("crates/ajax-web/web/src/features/settings/diagnostics.ts", [
        "surfaceV2",
        "Terminal Surface V2",
      ]),
      ...collectSymbolViolations("crates/ajax-web/web/vite.config.mts", [
        "ghostty-vt.wasm",
        "copyGhosttyWasm",
        "TerminalRawView.svelte",
        "XtermTerminalView.svelte",
      ]),
      ...collectSymbolViolations("crates/ajax-web/src/runtime.rs", [
        "/ghostty-vt.wasm",
        "axum_ghostty_wasm",
      ]),
      ...collectSymbolViolations("crates/ajax-web/src/adapters/assets.rs", [
        "ghostty-vt.wasm",
        "ghostty_wasm",
      ]),
      ...collectSymbolViolations("architecture.md", [
        "TerminalSurfaceSelector.svelte",
        "TerminalRawView.svelte",
        "XtermTerminalView.svelte",
        "Surface V2",
        "ghostty-web",
      ]),
      ...collectSymbolViolations("crates/ajax-web/web/TERMINAL.md", [
        "TerminalRawView.svelte",
        "XtermTerminalView.svelte",
        "TerminalSurfaceSelector.svelte",
        "terminalSurfaceSetting.ts",
        "Terminal Surface V2",
        "Surface V2",
      ]),
    );

    expect(violations).toEqual([]);
  });

  // The named-path list above only covers the terminal-era Svelte components, so
  // other `.svelte` files can reappear without failing anything — during the
  // 2026-07 cleanup `TaskDetail.svelte` and `TestInDevPanel.svelte` came back
  // into the working tree and every suite still passed, because nothing imports
  // them and the toolchain no longer looks at `.svelte` at all. This is the
  // catch-all: the React migration is complete, so the extension must not exist.
  it("keeps the web source tree free of any Svelte component", () => {
    const webSrc = join(webRoot, "src");
    const found: string[] = [];

    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        if (entry.name === "node_modules" || entry.name === "dist") continue;
        const full = join(dir, entry.name);
        if (entry.isDirectory()) walk(full);
        else if (entry.name.endsWith(".svelte")) found.push(relative(repoRoot, full));
      }
    };
    walk(webSrc);

    expect(found).toEqual([]);
  });
});
