import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  LOCKED_MAJOR_SECTIONS,
  STYLES_MANIFEST_REL,
  STYLES_SOURCE_MODULE_RELS,
  localStylesheetImports,
  majorCascadeSectionMarkers,
  readOrderedStylesSource,
  readStylesManifest,
  resolveStylesModuleRel,
  stylesheetAtRulesInOrder,
  stylesheetImportStatements,
} from "@/shared/lib/styleSources";

const webSrcRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

describe("web CSS architecture — cascade order lock", () => {
  const manifest = readStylesManifest(webSrcRoot);
  const ordered = readOrderedStylesSource(webSrcRoot);

  it("keeps manifest @imports before :root and @theme inline last among top-level at-rules", () => {
    const imports = stylesheetImportStatements(manifest);
    const rootIndex = ordered.indexOf(":root");
    const themeIndex = ordered.indexOf("@theme inline");

    expect(imports).toEqual([
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
    expect(ordered.indexOf(imports[0])).toBeLessThan(rootIndex);
    expect(themeIndex).toBeGreaterThan(rootIndex);
    expect(ordered.lastIndexOf("@theme inline")).toBe(themeIndex);
  });

  it("preserves the major section divider order in ordered stylesheet source", () => {
    expect(majorCascadeSectionMarkers(ordered)).toEqual([...LOCKED_MAJOR_SECTIONS]);
  });

  it("keeps SETTINGS VIEW between RESULT PANEL and SESSION ORCHESTRATION CHAT", () => {
    const resultPanel = ordered.indexOf("/* RESULT PANEL");
    const settingsView = ordered.indexOf("/* SETTINGS VIEW");
    const sessionChat = ordered.indexOf("/* SESSION ORCHESTRATION CHAT");
    expect(resultPanel).toBeGreaterThan(-1);
    expect(settingsView).toBeGreaterThan(resultPanel);
    expect(sessionChat).toBeGreaterThan(settingsView);
  });

  it("ends with the Tailwind token bridge after feature CSS", () => {
    const markers = majorCascadeSectionMarkers(ordered);
    expect(markers.at(-1)).toBe("TAILWIND THEME");
    expect(stylesheetAtRulesInOrder(ordered).at(-1)).toBe("@theme inline");
    expect(ordered.trimEnd().endsWith("}")).toBe(true);
  });

  it("anchors first and last declarations around the locked cascade", () => {
    expect(manifest).toMatch(
      /^\/\* Ajax Cockpit — global stylesheet[\s\S]*@import "tailwindcss\/utilities" layer\(utilities\);/,
    );
    expect(ordered).toMatch(
      /@theme inline \{\n {2}--color-paper: var\(--paper\);[\s\S]*--color-ok: var\(--ok\);\n\}\s*$/,
    );
  });
});

describe("web CSS architecture — manifest ownership", () => {
  it("leaves styles.css as manifest plus Tailwind bridge only (no feature rules)", () => {
    const manifest = readStylesManifest(webSrcRoot);
    const localImports = localStylesheetImports(manifest);
    const withoutOwnedContent = manifest
      .replace(/^\/\* Ajax Cockpit[\s\S]*?\*\/\s*/m, "")
      .replace(/^@import[^\n]+\n/gm, "")
      .replace(/\/\* TAILWIND THEME[\s\S]*?@theme inline \{[\s\S]*\}\s*$/, "")
      .trim();

    expect(localImports.map((importPath) => resolveStylesModuleRel(STYLES_MANIFEST_REL, importPath))).toEqual([
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
    expect(withoutOwnedContent).toBe("");
    expect(manifest).toContain("@theme inline");
  });

  it("keeps leaf feature modules free of local @import statements", () => {
    const aggregators = new Set([
      STYLES_MANIFEST_REL,
      "styles/chat.css",
      "styles/task-workspace.css",
      "styles/app-shell-continuation.css",
      "styles/task.css",
      "styles/app-shell-layout.css",
    ]);
    const leafViolations = STYLES_SOURCE_MODULE_RELS.filter((moduleRel) => {
      if (aggregators.has(moduleRel)) return false;
      return localStylesheetImports(readFileSync(join(webSrcRoot, moduleRel), "utf8")).length > 0;
    });
    expect(leafViolations).toEqual([]);
  });
});
