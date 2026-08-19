import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import {
  BASELINE,
  LOCKED_MAJOR_SECTIONS,
  STYLES_CSS_DIRECT_TEST_READERS,
  STYLES_SOURCE_MODULE_RELS,
  countClassSelectorLines,
  countHasSelectors,
  majorCascadeSectionMarkers,
  parseStylesheetGraph,
  readOrderedStylesSource,
  readStylesManifest,
  stylesheetImportStatements,
  totalStylesSourceBytes,
} from "./styleSources";

const webRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const webSrcRoot = join(webRoot, "src");
const builtAppCssPath = join(webRoot, "dist/app.css");

function testsReadingStylesCssDirectly(): string[] {
  const readers: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.test\.(ts|tsx)$/.test(entry.name)) {
        const rel = relative(webSrcRoot, path);
        if (rel.startsWith("shared/lib/styleSources")) continue;
        const source = readFileSync(path, "utf8");
        if (/readFileSync\([^)]*styles\.css[^)]*\)/.test(source)) {
          readers.push(rel);
        }
      }
    }
  };
  walk(webSrcRoot);
  return readers.sort();
}

function testsUsingRawStylesManifestForBehavior(): string[] {
  const readers: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.test\.(ts|tsx)$/.test(entry.name)) {
        const rel = relative(webSrcRoot, path);
        if (
          rel.startsWith("shared/lib/styleSources") ||
          rel === "styles.architecture.test.ts" ||
          rel === "styles/architecture.test.ts"
        ) {
          continue;
        }
        const source = readFileSync(path, "utf8");
        if (
          /readStylesManifest\(/.test(source) &&
          !/readOrderedStylesSource\(/.test(source)
        ) {
          readers.push(rel);
        }
      }
    }
  };
  walk(webSrcRoot);
  return readers.sort();
}

describe("styleSources helpers", () => {
  it("reads the canonical styles.css manifest path", () => {
    const css = readStylesManifest(webSrcRoot);
    expect(css.startsWith("/* Ajax Cockpit")).toBe(true);
    expect(join(webSrcRoot, "styles.css").endsWith("src/styles.css")).toBe(true);
  });

  it("counts class-selector lines and :has() selectors against the T0 baseline", () => {
    const css = readOrderedStylesSource(webSrcRoot);
    expect(countClassSelectorLines(css)).toBe(BASELINE.classSelectorLines);
    expect(countHasSelectors(css)).toBe(BASELINE.hasSelectors);
  });

  it("extracts manifest @import statements in cascade order", () => {
    expect(stylesheetImportStatements(readStylesManifest(webSrcRoot))).toEqual([
      '@import "tailwindcss/utilities" layer(utilities);',
      '@import "./styles/foundation.css";',
      '@import "./styles/app-shell.css";',
      '@import "./styles/settings.css";',
      '@import "./styles/session.css";',
      '@import "./styles/app-shell-continuation.css";',
      '@import "./styles/task.css";',
      '@import "./styles/app-shell-layout.css";',
      '@import "./styles/diff-review.css";',
    ]);
  });

  it("lists major cascade section markers in ordered source order", () => {
    expect(majorCascadeSectionMarkers(readOrderedStylesSource(webSrcRoot))).toEqual([
      ...LOCKED_MAJOR_SECTIONS,
    ]);
  });

  it("finds the manifest and T2-owned CSS modules under web/src", () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".css")) {
          files.push(relative(webSrcRoot, path));
        }
      }
    };
    walk(webSrcRoot);
    expect(files.sort()).toEqual([...STYLES_SOURCE_MODULE_RELS].sort());
  });

  it("records tests that read styles.css via readFileSync", () => {
    expect(testsReadingStylesCssDirectly()).toEqual([...STYLES_CSS_DIRECT_TEST_READERS]);
  });

  it("does not treat the physical styles.css manifest as the style API in feature tests", () => {
    expect(testsUsingRawStylesManifestForBehavior()).toEqual([]);
  });

  it("reports baseline byte sizes for source modules and built CSS", () => {
    expect(totalStylesSourceBytes(webSrcRoot)).toBe(BASELINE.sourceStylesCssBytes);
    expect(statSync(builtAppCssPath).size).toBe(BASELINE.builtAppCssBytes);
    expect(gzipSync(readFileSync(builtAppCssPath)).byteLength).toBe(
      BASELINE.builtAppCssGzipBytes,
    );
  });

  it("parses the stylesheet graph from on-disk shell entrypoints", () => {
    expect(
      parseStylesheetGraph({
        appHtml: readFileSync(join(webRoot, "app.html"), "utf8"),
        mainTsx: readFileSync(join(webSrcRoot, "app/main.tsx"), "utf8"),
        viteConfig: readFileSync(join(webRoot, "vite.config.mts"), "utf8"),
        stylesSource: readStylesManifest(webSrcRoot),
      }),
    ).toEqual({
      entryHtml: "app.html",
      jsEntries: ["/src/app/main.tsx"],
      cssEntries: ["../styles.css"],
      cssImports: [
        '@import "tailwindcss/utilities" layer(utilities);',
        '@import "./styles/foundation.css";',
        '@import "./styles/app-shell.css";',
        '@import "./styles/settings.css";',
        '@import "./styles/session.css";',
        '@import "./styles/app-shell-continuation.css";',
        '@import "./styles/task.css";',
        '@import "./styles/app-shell-layout.css";',
        '@import "./styles/diff-review.css";',
      ],
      cssSourceModules: [...STYLES_SOURCE_MODULE_RELS],
      viteCssCodeSplitDisabled: true,
      viteCssAssetName: "app.css",
    });
  });
});
