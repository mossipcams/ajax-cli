// @ts-expect-error ponytail: test-only filesystem loader; @types/node is not in web:check scope
import { readFileSync, statSync } from "node:fs";
// @ts-expect-error ponytail: test-only filesystem loader; @types/node is not in web:check scope
import { dirname, join } from "node:path";

/** Measured baseline captured in T0/T3 — update only after an intentional CSS change. */
export const BASELINE = {
  /** Sum of styles.css + imported module bytes after T5 measured optimization.
   * Re-measured when Effort / Fast chips sat above the model list's iOS
   * overflow hit target so taps could change the selection.
   * Re-measured when model-list scroll hit targets were restored (#1022).
   * Re-measured again after pass-2 tool disclosure styling.
   * Re-measured after waiting-pill glyph fix (mossipcams/ajax-cli#1020).
   * Re-measured after chat elicitation and output-content owned modules.
   * Re-measured after composer hotbar CSS and session tap-dismiss shell height
   * (mossipcams/ajax-cli#1032).
   * Re-measured after stacked composer hotbar above full-width message row.
   * Re-measured after home-indicator inset moved into textarea row (#1034).
   * Re-measured after flush-bottom composer and trailing hotbar action cluster.
   * Re-measured after session closed-keyboard CSS lock on app-viewport (#1032).
   * Re-measured after closed-keyboard session overflow chain 100lvh stretch.
   * Re-measured after closed-keyboard session band uses lvh minus home inset. */
  sourceStylesCssBytes: 108_076,
  builtAppCssBytes: 93_342,
  builtAppCssGzipBytes: 16_395,
  classSelectorLines: 600,
  hasSelectors: 18,
} as const;

export const STYLES_MANIFEST_REL = "styles.css";

export const STYLES_SOURCE_MODULE_RELS = [
  "styles.css",
  "styles/foundation.css",
  "styles/app-shell.css",
  "styles/settings.css",
  "styles/chat.css",
  "styles/chat/activity.css",
  "styles/chat/composer.css",
  "styles/chat/conversation.css",
  "styles/chat/markdown.css",
  "styles/chat/model.css",
  "styles/chat/output-content.css",
  "styles/chat/permissions.css",
  "styles/chat/elicitation.css",
  "styles/chat/queued.css",
  "styles/chat/scrolling.css",
  "styles/chat/status.css",
  "styles/chat/surface.css",
  "styles/task-workspace.css",
  "styles/task-workspace/sheets.css",
  "styles/app-shell-continuation.css",
  "styles/app-shell/interact.css",
  "styles/app-shell/layout.css",
  "styles/app-shell/motion.css",
  "styles/app-shell/narrow.css",
  "styles/app-shell/nav.css",
  "styles/app-shell/page-lead.css",
  "styles/app-shell/primitives.css",
  "styles/app-shell/skeleton.css",
  "styles/task.css",
  "styles/task/detail.css",
  "styles/task/list.css",
  "styles/task/meta.css",
  "styles/task/new-task.css",
  "styles/task/test-in-dev.css",
  "styles/terminal.css",
  "styles/app-shell-layout.css",
  "styles/app-shell/shell-layout.css",
  "styles/diff-review.css",
] as const;

export const STYLES_CSS_DIRECT_TEST_READERS = [] as const;

/** Locked cascade section order — original HEAD order; update only when a wave moves rules. */
export const LOCKED_MAJOR_SECTIONS = [
  "Base element resets",
  "TOP CHROME — sticky header stack with iOS safe-area",
  "CONNECTION STATUS — only shouts when something is wrong",
  "RESULT PANEL",
  "SESSION ORCHESTRATION CHAT",
  "Permission panel",
  "Agent form elicitation",
  "Live head / status",
  "Scrolling",
  "Jump to live",
  "Conversation",
  "Markdown inside agent prose",
  "Queued follow-up",
  "Activity grid",
  "Reasoning",
  "Plan",
  "Tool cards",
  "Diff",
  "Context pressure",
  "Sheets",
  "PAGE LEAD / UPDATE BANNER",
  "MAIN LAYOUT — route-scroll is the only normal vertical scroll owner.",
  "EMPTY STATE (App + TaskList)",
  "STATUS DOT (TaskList + TaskCard)",
  "ACTION BUTTONS (ActionBar + TaskCard \"Open\")",
  "PILL BUTTONS (connection / result / sheet / settings / pane)",
  "INTERACT PANEL shell (TaskDetail) — flat hairline strip, CLI-style",
  "BOTTOM NAV",
  "MOTION",
  "SKELETON (loading placeholder)",
  "NARROW PHONES (shared chrome)",
  "TASK LIST (dashboard)",
  "TaskTerminal",
  "TEST IN DEV PANEL",
  "DETAIL HEADER",
  "META DETAILS",
  "SHELL LAYOUT — app-viewport, app-shell, app-main",
  "DIFF REVIEW",
  "TAILWIND THEME",
] as const;

export function countClassSelectorLines(css: string): number {
  return css.split("\n").filter((line) => /^\s*\.[a-zA-Z_-]/.test(line)).length;
}

export function countHasSelectors(css: string): number {
  return css.split("\n").filter((line) => line.includes(":has(")).length;
}

// Major section dividers: banner comments ending with --- in cascade order.
export function majorCascadeSectionMarkers(css: string): string[] {
  return [...css.matchAll(/^\/\* ([^\n]+?) -{3,} \*\/\s*$/gm)].map((match) =>
    match[1].trim(),
  );
}

export function stylesheetImportStatements(css: string): string[] {
  return [...css.matchAll(/^@import[^\n]+/gm)].map((match) => match[0].trim());
}

export function stylesheetAtRulesInOrder(css: string): string[] {
  const rules: string[] = [];
  if (css.match(/^\/\*[\s\S]*?\*\//)?.[0]) {
    rules.push("file-header-comment");
  }
  for (const statement of stylesheetImportStatements(css)) {
    rules.push(statement);
  }
  if (css.includes(":root")) rules.push(":root");
  if (css.includes("@theme inline")) rules.push("@theme inline");
  return rules;
}

export type StylesheetGraph = {
  entryHtml: string;
  jsEntries: string[];
  cssEntries: string[];
  cssImports: string[];
  cssSourceModules: string[];
  viteCssCodeSplitDisabled: boolean;
  viteCssAssetName: string;
};

export function parseStylesheetGraph(input: {
  appHtml: string;
  mainTsx: string;
  viteConfig: string;
  stylesSource: string;
}): StylesheetGraph {
  const jsEntries = [...input.appHtml.matchAll(/src="([^"]+\.tsx)"/g)].map((m) => m[1]);
  const cssEntries = [...input.mainTsx.matchAll(/import\s+"([^"]+\.css)"/g)].map((m) => m[1]);
  const cssCodeSplitDisabled = /cssCodeSplit:\s*false/.test(input.viteConfig);
  const cssAssetName =
    input.viteConfig.match(/if \(name\.endsWith\("\.css"\)\) return "([^"]+)"/)?.[1] ??
    "";

  return {
    entryHtml: "app.html",
    jsEntries,
    cssEntries,
    cssImports: stylesheetImportStatements(input.stylesSource),
    cssSourceModules: [...STYLES_SOURCE_MODULE_RELS],
    viteCssCodeSplitDisabled: cssCodeSplitDisabled,
    viteCssAssetName: cssAssetName,
  };
}

export function readStylesManifest(webSrcRoot: string): string {
  return readFileSync(join(webSrcRoot, STYLES_MANIFEST_REL), "utf8");
}

function expandLocalStylesheetImports(
  cssPath: string,
  seen = new Set<string>(),
): string {
  const absolutePath = join(cssPath);
  if (seen.has(absolutePath)) return "";
  seen.add(absolutePath);

  const css = readFileSync(absolutePath, "utf8");
  const cssDir = dirname(absolutePath);
  let expanded = "";
  let lastIndex = 0;
  const importRe = /^@import\s+([^;]+);\s*$/gm;
  let match: RegExpExecArray | null;

  while ((match = importRe.exec(css)) !== null) {
    expanded += css.slice(lastIndex, match.index);
    const importPath = match[1].trim().replace(/^["']|["']$/g, "");
    if (importPath.startsWith("./")) {
      expanded += expandLocalStylesheetImports(
        join(cssDir, importPath.slice(2)),
        seen,
      );
    } else {
      expanded += `${match[0]}\n`;
    }
    lastIndex = match.index + match[0].length;
  }

  expanded += css.slice(lastIndex);
  return expanded;
}

/** Expand local @import statements in manifest order; keep package imports as statements. */
export function readOrderedStylesSource(webSrcRoot: string): string {
  return expandLocalStylesheetImports(join(webSrcRoot, STYLES_MANIFEST_REL));
}

export function totalStylesSourceBytes(webSrcRoot: string): number {
  return STYLES_SOURCE_MODULE_RELS.reduce(
    (total, rel) => total + statSync(join(webSrcRoot, rel)).size,
    0,
  );
}

/** Local `./…` @import targets declared in one stylesheet file. */
export function localStylesheetImports(css: string): string[] {
  return stylesheetImportStatements(css)
    .map((statement) => statement.match(/@import\s+["'](\.\/[^"']+)["']/)?.[1])
    .filter((importPath): importPath is string => importPath !== undefined);
}

export function resolveStylesModuleRel(
  importerRel: string,
  importPath: string,
): string {
  const normalizedImporter = importerRel.replace(/\\/g, "/");
  const importerDir = dirname(normalizedImporter);
  return join(importerDir, importPath.slice(2)).replace(/\\/g, "/");
}

const STYLES_FEATURE_GROUPS = {
  foundation: new Set(["styles/foundation.css"]),
  settings: new Set(["styles/settings.css"]),
  session: new Set([
    "styles/chat.css",
    "styles/chat/activity.css",
    "styles/chat/composer.css",
    "styles/chat/conversation.css",
    "styles/chat/permissions.css",
    "styles/chat/elicitation.css",
    "styles/chat/status.css",
    "styles/chat/markdown.css",
    "styles/chat/model.css",
    "styles/chat/output-content.css",
    "styles/chat/queued.css",
    "styles/chat/scrolling.css",
    "styles/chat/surface.css",
  ]),
  taskWorkspace: new Set([
    "styles/task-workspace.css",
    "styles/task-workspace/sheets.css",
  ]),
  task: new Set([
    "styles/task.css",
    "styles/task/detail.css",
    "styles/task/list.css",
    "styles/task/meta.css",
    "styles/task/new-task.css",
    "styles/task/test-in-dev.css",
    "styles/terminal.css",
  ]),
  appShell: new Set([
    "styles/app-shell.css",
    "styles/app-shell-continuation.css",
    "styles/app-shell-layout.css",
    "styles/app-shell/interact.css",
    "styles/app-shell/layout.css",
    "styles/app-shell/motion.css",
    "styles/app-shell/narrow.css",
    "styles/app-shell/nav.css",
    "styles/app-shell/page-lead.css",
    "styles/app-shell/primitives.css",
    "styles/app-shell/shell-layout.css",
    "styles/app-shell/skeleton.css",
  ]),
  diffReview: new Set(["styles/diff-review.css"]),
} as const;

const STYLES_FEATURE_GROUP_BY_MODULE = new Map<string, string>(
  Object.entries(STYLES_FEATURE_GROUPS).flatMap(([group, modules]) =>
    [...modules].map((moduleRel) => [moduleRel, group]),
  ),
);

export function stylesFeatureGroup(moduleRel: string): string {
  return STYLES_FEATURE_GROUP_BY_MODULE.get(moduleRel) ?? "unknown";
}

export function stylesImportGraph(webSrcRoot: string): Map<string, string[]> {
  const graph = new Map<string, string[]>();
  for (const moduleRel of STYLES_SOURCE_MODULE_RELS) {
    const css = readFileSync(join(webSrcRoot, moduleRel), "utf8");
    graph.set(
      moduleRel,
      localStylesheetImports(css).map((importPath) =>
        resolveStylesModuleRel(moduleRel, importPath),
      ),
    );
  }
  return graph;
}

/** Visit counts starting from the manifest; each owned module must appear once. */
export function stylesManifestReachCounts(webSrcRoot: string): Map<string, number> {
  const counts = new Map<string, number>();
  const visit = (moduleRel: string) => {
    counts.set(moduleRel, (counts.get(moduleRel) ?? 0) + 1);
    const css = readFileSync(join(webSrcRoot, moduleRel), "utf8");
    for (const importPath of localStylesheetImports(css)) {
      visit(resolveStylesModuleRel(moduleRel, importPath));
    }
  };
  visit(STYLES_MANIFEST_REL);
  return counts;
}
