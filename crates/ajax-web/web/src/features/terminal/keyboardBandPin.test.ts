import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readOrderedStylesSource(join(here, "../.."));

function taskTerminalStylesSection(): string {
  const start = stylesSource.indexOf("/* TaskTerminal");
  const end = stylesSource.indexOf("/* TAILWIND THEME");
  if (start < 0 || end <= start) return "";
  return stylesSource.slice(start, end);
}

/** Exact height-based visualViewport band pin (flush above iOS keyboard). */
const HEIGHT_PIN_TOP = /top:\s*var\(--app-top,\s*var\(--app-band-top,\s*0px\)\)/;
const HEIGHT_PIN_HEIGHT =
  /height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/;
const HEIGHT_PIN_MAX_HEIGHT =
  /max-height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/;

/** Forbidden: 100lvh bottom math that gaps above the soft keyboard on Safari. */
const FORBIDDEN_LVH_BOTTOM =
  /bottom:\s*max\(\s*0px,\s*calc\(\s*100lvh\s*-\s*var\(--app-top/;

function stripCssComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

function expectHeightBandPin(ruleBody: string, options: { requireMaxHeight?: boolean } = {}) {
  const body = stripCssComments(ruleBody);
  expect(body).toMatch(/position:\s*fixed/);
  expect(body).toMatch(HEIGHT_PIN_TOP);
  expect(body).toMatch(HEIGHT_PIN_HEIGHT);
  if (options.requireMaxHeight !== false) {
    expect(body).toMatch(HEIGHT_PIN_MAX_HEIGHT);
  }
  expect(body).not.toMatch(FORBIDDEN_LVH_BOTTOM);
  expect(body).not.toMatch(/bottom:\s*max\(/);
  expect(body).not.toMatch(/height:\s*auto/);
  expect(body).not.toMatch(/max-height:\s*none/);
}

describe("keyboard band height pin contract", () => {
  it("pins inline task-detail with visualViewport height (not 100lvh bottom)", () => {
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";
    const rule =
      mobileBlock.match(
        /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expectHeightBandPin(rule);
  });

  it("forbids a nested session-page keyboard-open position:fixed pin (#877)", () => {
    const mobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";
    expect(mobileBlock).not.toMatch(
      /html\.keyboard-open\s+\.session-page\.session-chat\s*\{[^}]*position:\s*fixed/,
    );
  });

  it("shrinks session-thread with flex 1 1 0% so the column stays inside app-viewport", () => {
    const rule =
      stylesSource.match(/\.session-thread\s*\{([^}]*)\}/)?.[1] ?? "";
    const body = stripCssComments(rule);
    expect(body).toMatch(/flex:\s*1\s+1\s+0%/);
    expect(body).toMatch(/min-height:\s*0/);
    expect(body).not.toMatch(/flex:\s*1\s+1\s+80%/);
  });

  it("pins app-viewport with visualViewport height (not 100lvh bottom)", () => {
    const rule =
      stylesSource.match(/html\.keyboard-open\s+\.app-viewport\s*\{([^}]*)\}/)?.[1] ?? "";
    expectHeightBandPin(rule);
  });

  it("pins fullscreen layer with visualViewport height (not 100lvh bottom)", () => {
    const rule = stylesSource.match(/\.fullscreen-layer\s*\{([^}]*)\}/)?.[1] ?? "";
    // FullscreenLayer does not set max-height; height alone is enough.
    expectHeightBandPin(rule, { requireMaxHeight: false });
  });

  it("pins expanded terminal panel with visualViewport height (not 100lvh bottom)", () => {
    const rule =
      taskTerminalStylesSection().match(
        /html\.terminal-expanded\s+\.terminal-panel\.is-expanded\s*\{([\s\S]*?)\n {2}\}/,
      )?.[1] ?? "";
    expectHeightBandPin(rule);
  });

  it("forbids 100lvh bottom band math anywhere in pin surfaces", () => {
    for (const source of [stylesSource, taskTerminalStylesSection()]) {
      expect(stripCssComments(source)).not.toMatch(FORBIDDEN_LVH_BOTTOM);
    }
  });

  it("drops the fullscreen safe-area hotbar pad while the keyboard is open", () => {
    const ruleBody =
      taskTerminalStylesSection().match(
        /html\.keyboard-open\s+\.terminal-panel\.is-expanded\s+\.terminal-keys\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    const body = stripCssComments(ruleBody);
    expect(body).toMatch(/padding-bottom:\s*6px/);
    expect(body).not.toMatch(/env\(safe-area-inset-bottom\)/);
  });

  // Embedded shell is dist/app.css (include_bytes!), not the src/styles.css source.
  it("ships the expanded keyboard-open hotbar pad override in dist/app.css", () => {
    const distCss = readFileSync(join(here, "../../../dist/app.css"), "utf8");
    expect(distCss).toMatch(
      /html\.keyboard-open\s+\.terminal-panel\.is-expanded[^{]*\.terminal-keys[^{]*\{[^}]*padding-bottom:6px/,
    );
  });

  it("does not ship a nested session keyboard-open position:fixed pin in dist/app.css", () => {
    const distCss = readFileSync(join(here, "../../../dist/app.css"), "utf8");
    expect(distCss).not.toMatch(
      /html\.keyboard-open[^{]*\.session-page\.session-chat[^{]*\{[^}]*position:fixed/,
    );
  });

  it("bypasses app-viewport fixed pin when session owns viewport (#877)", () => {
    const rule =
      stylesSource.match(
        /html\[data-session-viewport="owned"\]\.keyboard-open\s+\.app-viewport\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    const body = stripCssComments(rule);
    expect(body).toMatch(/position:\s*static/);
    expect(body).toMatch(/height:\s*100%/);
    expect(body).not.toMatch(/position:\s*fixed/);
    expect(body).not.toMatch(/height:\s*auto/);
  });

  it("keyboard-open session constrains overflow chain to visualViewport height (#1122)", () => {
    const css = stripCssComments(stylesSource);
    const chainRule =
      css.match(
        /html\[data-session-viewport="owned"\]\.keyboard-open,\s*html\[data-session-viewport="owned"\]\.keyboard-open body,\s*html\[data-session-viewport="owned"\]\.keyboard-open #app\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(chainRule).toMatch(
      /height:\s*var\(--app-height,\s*100dvh\)/,
    );
    expect(chainRule).toMatch(
      /max-height:\s*var\(--app-height,\s*100dvh\)/,
    );

    const appViewportRule =
      css.match(
        /html\[data-session-viewport="owned"\]\.keyboard-open \.app-viewport\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(appViewportRule).toMatch(/height:\s*100%/);
    expect(appViewportRule).toMatch(/max-height:\s*100%/);
  });

  it("closed-keyboard session lock stretches overflow chain to lvh minus home inset (#1034)", () => {
    const css = stripCssComments(stylesSource);
    const sessionBandHeight =
      /calc\(100lvh\s*-\s*env\(safe-area-inset-bottom,\s*0px\)\)/;
    const overflowChainRule =
      css.match(
        /html\[data-session-viewport="owned"\]:not\(\.keyboard-open\),\s*html\[data-session-viewport="owned"\]:not\(\.keyboard-open\) body,\s*html\[data-session-viewport="owned"\]:not\(\.keyboard-open\) #app\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(overflowChainRule).toMatch(new RegExp(`height:\\s*${sessionBandHeight.source}`));

    const appViewportRule =
      css.match(
        /html\[data-session-viewport="owned"\]:not\(\.keyboard-open\) \.app-viewport\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(appViewportRule).toMatch(new RegExp(`--app-band-height:\\s*${sessionBandHeight.source}`));
    expect(appViewportRule).not.toMatch(/100dvh/);

    const sessionRouteScroll =
      css.match(
        /\[data-testid="route-scroll"\]:has\(\[data-outlet="session"\]\)\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(sessionRouteScroll).toMatch(/padding-bottom:\s*0/);
  });
});
