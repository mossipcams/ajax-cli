import { describe, it, expect } from "vitest";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import {
  BASELINE,
  STYLES_SOURCE_MODULE_RELS,
  countClassSelectorLines,
  countHasSelectors,
  localStylesheetImports,
  parseStylesheetGraph,
  readOrderedStylesSource,
  readStylesManifest,
  resolveStylesModuleRel,
  stylesFeatureGroup,
  stylesImportGraph,
  stylesManifestReachCounts,
  totalStylesSourceBytes,
} from "@/shared/lib/styleSources";

const webRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const webSrcRoot = join(webRoot, "src");
const builtAppCssPath = join(webRoot, "dist/app.css");

describe("web CSS architecture — stylesheet graph", () => {
  it("boots through app.html → main.tsx → styles.css manifest with owned CSS modules", () => {
    const graph = parseStylesheetGraph({
      appHtml: readFileSync(join(webRoot, "app.html"), "utf8"),
      mainTsx: readFileSync(join(webSrcRoot, "app/main.tsx"), "utf8"),
      viteConfig: readFileSync(join(webRoot, "vite.config.mts"), "utf8"),
      stylesSource: readStylesManifest(webSrcRoot),
    });
    expect(graph.entryHtml).toBe("app.html");
    expect(graph.jsEntries).toEqual(["/src/app/main.tsx"]);
    expect(graph.cssEntries).toEqual(["../styles.css"]);
    expect(graph.cssImports).toEqual([
      '@import "tailwindcss/utilities" layer(utilities);',
      '@import "./styles/foundation.css";',
      '@import "./styles/app-shell.css";',
      '@import "./styles/settings.css";',
      '@import "./styles/chat.css";',
      '@import "./styles/task-workspace.css";',
      '@import "./styles/app-shell-continuation.css";',
      '@import "./styles/task.css";',
      '@import "./styles/app-shell-layout.css";',
      '@import "./styles/diff-review.css";',
    ]);
    expect(graph.cssSourceModules).toEqual([...STYLES_SOURCE_MODULE_RELS]);
    const cssFiles: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".css")) cssFiles.push(path);
      }
    };
    walk(webSrcRoot);
    expect(cssFiles.sort()).toEqual(
      [...STYLES_SOURCE_MODULE_RELS].map((rel) => join(webSrcRoot, rel)).sort(),
    );
  });

  it("locks Vite to a single non-split app.css asset", () => {
    const vite = readFileSync(join(webRoot, "vite.config.mts"), "utf8");
    const graph = parseStylesheetGraph({
      appHtml: "",
      mainTsx: "",
      viteConfig: vite,
      stylesSource: "",
    });
    expect(graph.viteCssCodeSplitDisabled).toBe(true);
    expect(graph.viteCssAssetName).toBe("app.css");
    expect(vite).toMatch(/cssCodeSplit:\s*false/);
    expect(vite).toMatch(/if \(name\.endsWith\("\.css"\)\) return "app\.css"/);
  });

  it("does not link a stylesheet from app.html (CSS arrives via the JS graph)", () => {
    const html = readFileSync(join(webRoot, "app.html"), "utf8");
    expect(html).not.toMatch(/<link[^>]*rel="stylesheet"/);
    expect(html).toContain('src="/src/app/main.tsx"');
  });

  it("emits exactly one dist/*.css named app.css after build", () => {
    expect(existsSync(builtAppCssPath)).toBe(true);
    const cssFiles = readdirSync(join(webRoot, "dist")).filter((name) =>
      name.endsWith(".css"),
    );
    expect(cssFiles).toEqual(["app.css"]);
  });

  it("ships built app.css bytes matching the T0 baseline", () => {
    const css = readFileSync(builtAppCssPath, "utf8");
    expect(statSync(builtAppCssPath).size).toBe(BASELINE.builtAppCssBytes);
    expect(css).toContain(".app-shell");
    expect(css).toContain("@layer utilities");
  });
});

describe("web CSS architecture — one app.css ownership", () => {
  it("imports each owned module exactly once from the manifest graph", () => {
    const reach = stylesManifestReachCounts(webSrcRoot);
    for (const moduleRel of STYLES_SOURCE_MODULE_RELS) {
      expect(reach.get(moduleRel)).toBe(1);
    }
    expect(reach.size).toBe(STYLES_SOURCE_MODULE_RELS.length);
  });

  it("keeps manifest @imports limited to top-level feature entrypoints", () => {
    const manifestImports = localStylesheetImports(readStylesManifest(webSrcRoot)).map(
      (importPath) => resolveStylesModuleRel("styles.css", importPath),
    );
    expect(manifestImports).toEqual([
      "styles/foundation.css",
      "styles/app-shell.css",
      "styles/settings.css",
      "styles/chat.css",
      "styles/task-workspace.css",
      "styles/app-shell-continuation.css",
      "styles/task.css",
      "styles/app-shell-layout.css",
      "styles/diff-review.css",
    ]);
  });

  it("does not let feature modules import sibling feature CSS", () => {
    const violations: string[] = [];
    for (const [moduleRel, imports] of stylesImportGraph(webSrcRoot)) {
      if (moduleRel === "styles.css") continue;
      const owner = stylesFeatureGroup(moduleRel);
      for (const importRel of imports) {
        const importOwner = stylesFeatureGroup(importRel);
        if (importOwner !== owner) {
          violations.push(`${moduleRel} -> ${importRel} (${owner} -> ${importOwner})`);
        }
      }
    }
    expect(violations).toEqual([]);
  });
});

describe("web CSS architecture — baseline ledger", () => {
  it("matches recorded source/built/gzip sizes and selector counts", () => {
    const css = readOrderedStylesSource(webSrcRoot);
    expect({
      sourceStylesCssBytes: totalStylesSourceBytes(webSrcRoot),
      builtAppCssBytes: statSync(builtAppCssPath).size,
      builtAppCssGzipBytes: gzipSync(readFileSync(builtAppCssPath)).byteLength,
      classSelectorLines: countClassSelectorLines(css),
      hasSelectors: countHasSelectors(css),
      stylesCssDirectTestReaders: 0,
    }).toEqual({
      sourceStylesCssBytes: BASELINE.sourceStylesCssBytes,
      builtAppCssBytes: BASELINE.builtAppCssBytes,
      builtAppCssGzipBytes: BASELINE.builtAppCssGzipBytes,
      classSelectorLines: BASELINE.classSelectorLines,
      hasSelectors: BASELINE.hasSelectors,
      stylesCssDirectTestReaders: 0,
    });
  });
});
