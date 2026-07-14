// Reproduce mid-token buffer garble while scrolled on the Web PWA terminal.
// Emits known fixed-width MARK lines, scrolls into history, then stresses
// follow-output and keyboard-open resize. Markers must stay contiguous.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  terminalFrames,
  waitForTerminalSocket,
} from "./fixtures";

type TerminalProbe = {
  cols(): number;
  rows(): number;
  viewportY(): number;
  lines(): string[];
};

const gridCanvas = (page: Page) =>
  terminalPanel(page).locator("canvas:not([aria-hidden='true'])");

const newOutputButton = (page: Page) =>
  page.getByRole("button", { name: "New output ↓" });

async function saveDiagnosticScreenshot(page: Page, path: string) {
  if (!process.env.CI) await page.screenshot({ path, fullPage: true });
}

async function emitTerminalOutput(page: Page, text: string) {
  const delivered = await page.evaluate((payload) => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{
          url?: string;
          emitMessage(data: string): void;
        }>;
      }
    ).__terminalSockets;
    const socket = [...sockets]
      .reverse()
      .find((item) => typeof item.url === "string" && item.url.includes("/terminal"));
    if (!socket) return false;
    socket.emitMessage(payload);
    return true;
  }, text);
  expect(delivered).toBe(true);
}

async function enableTerminalProbe(page: Page) {
  await page.addInitScript(() => {
    Object.defineProperty(window, "__ajaxTerminalProbeEnable", {
      value: true,
      configurable: true,
    });
  });
}

async function readProbe(page: Page): Promise<{
  cols: number;
  rows: number;
  viewportY: number;
  lines: string[];
}> {
  return page.evaluate(() => {
    const probe = (window as unknown as { __ajaxTerminalProbe?: TerminalProbe })
      .__ajaxTerminalProbe;
    if (!probe) {
      return { cols: 0, rows: 0, viewportY: 0, lines: [] };
    }
    return {
      cols: probe.cols(),
      rows: probe.rows(),
      viewportY: probe.viewportY(),
      lines: probe.lines(),
    };
  });
}

/** One screen-width row: left-padded index + MARK_NNNN that never soft-wraps. */
function markerLine(index: number, cols: number): string {
  const mark = `MARK_${String(index).padStart(4, "0")}`;
  const body = `${String(index).padStart(4, "0")} ${mark}`;
  if (body.length >= cols) {
    return mark.slice(0, Math.max(1, cols - 1));
  }
  return body.padEnd(cols - 1, ".");
}

function markerScrollback(from: number, count: number, cols: number): string {
  let out = "";
  for (let i = from; i < from + count; i += 1) {
    out += `${markerLine(i, cols)}\r\n`;
  }
  return out;
}

async function openTaskTerminal(page: Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await enableTerminalProbe(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await expect(gridCanvas(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await expect
    .poll(async () => (await readProbe(page)).cols, { timeout: 10_000 })
    .toBeGreaterThan(0);
}

async function swipeIntoScrollback(page: Page) {
  const canvas = gridCanvas(page);
  const box = await canvas.boundingBox();
  if (!box) throw new Error("terminal canvas has no bounding box");

  await canvas.dragTo(canvas, {
    sourcePosition: { x: box.width / 2, y: box.height * 0.8 },
    targetPosition: { x: box.width / 2, y: box.height * 0.2 },
  });

  const host = terminalPanel(page).locator(".terminal-host");
  await host.evaluate((el) => {
    for (let i = 0; i < 12; i += 1) {
      el.dispatchEvent(
        new WheelEvent("wheel", {
          deltaY: -3,
          deltaMode: WheelEvent.DOM_DELTA_LINE,
          bubbles: true,
          cancelable: true,
        }),
      );
    }
  });
}

function assertMarkersContiguous(lines: string[], from: number, count: number) {
  const joined = lines.join("\n");
  const missing: string[] = [];
  const split: string[] = [];
  for (let i = from; i < from + count; i += 1) {
    const mark = `MARK_${String(i).padStart(4, "0")}`;
    if (!joined.includes(mark)) {
      missing.push(mark);
      continue;
    }
    // Mid-token split: MARK appears broken across a newline (MA\nRK_... etc).
    const broken = new RegExp(
      `M(?:\\n|\\r)ARK_${String(i).padStart(4, "0")}|MARK(?:\\n|\\r)_${String(i).padStart(4, "0")}|MARK_${String(i).padStart(4, "0").slice(0, 2)}(?:\\n|\\r)${String(i).padStart(4, "0").slice(2)}`,
    );
    if (broken.test(joined)) split.push(mark);
  }
  expect(missing, `missing markers; sample=${JSON.stringify(lines.slice(0, 8))}`).toEqual([]);
  expect(split, `split markers; sample=${JSON.stringify(lines.slice(0, 8))}`).toEqual([]);
}

test.describe("terminal scroll garble repro (mobile PWA)", () => {
  test("markers stay contiguous after output while scrolled up", async ({ page }) => {
    await openTaskTerminal(page);
    const { cols } = await readProbe(page);
    expect(cols).toBeGreaterThanOrEqual(40);

    await emitTerminalOutput(page, markerScrollback(0, 200, cols));
    await expect(newOutputButton(page)).not.toBeVisible();

    await swipeIntoScrollback(page);
    await emitTerminalOutput(page, markerScrollback(200, 40, cols));
    await expect(newOutputButton(page)).toBeVisible({ timeout: 10_000 });

    const after = await readProbe(page);
    assertMarkersContiguous(after.lines, 0, 240);
    await saveDiagnosticScreenshot(
      page,
      "crates/ajax-web/web/e2e/artifacts/garble-output-while-scrolled.png",
    );
  });

  test("markers stay contiguous after keyboard-open resize while scrolled", async ({
    page,
  }) => {
    await openTaskTerminal(page);
    const before = await readProbe(page);
    expect(before.cols).toBeGreaterThanOrEqual(40);

    await emitTerminalOutput(page, markerScrollback(0, 200, before.cols));
    await swipeIntoScrollback(page);

    // Simulate iOS soft keyboard: class + visualViewport shrink + resize events.
    await page.evaluate(() => {
      document.documentElement.classList.add("keyboard-open");
      document.documentElement.style.setProperty(
        "--app-height",
        `${window.innerHeight - 336}px`,
      );
      window.dispatchEvent(new Event("resize"));
      window.visualViewport?.dispatchEvent(new Event("resize"));
    });

    // Let refit / crop settle, then close keyboard (flush withheld PTY resize).
    await page.waitForTimeout(200);
    await page.evaluate(() => {
      document.documentElement.classList.remove("keyboard-open");
      document.documentElement.style.setProperty("--app-height", `${window.innerHeight}px`);
      window.dispatchEvent(new Event("resize"));
      window.visualViewport?.dispatchEvent(new Event("resize"));
    });
    await page.waitForTimeout(200);

    // More output while still away from bottom.
    await emitTerminalOutput(page, markerScrollback(200, 20, before.cols));
    await expect
      .poll(async () => {
        const lines = (await readProbe(page)).lines.join("\n");
        return lines.includes("MARK_0200");
      }, { timeout: 5_000 })
      .toBe(true);

    const after = await readProbe(page);
    assertMarkersContiguous(after.lines, 0, 220);
    await saveDiagnosticScreenshot(
      page,
      "crates/ajax-web/web/e2e/artifacts/garble-keyboard-while-scrolled.png",
    );
  });

  test("CSI redraw while scrolled does not shatter marker rows", async ({ page }) => {
    await openTaskTerminal(page);
    const { cols, rows } = await readProbe(page);
    expect(cols).toBeGreaterThanOrEqual(40);

    await emitTerminalOutput(page, markerScrollback(0, 120, cols));
    await swipeIntoScrollback(page);

    // Mimic a fullscreen TUI repaint (Claude Code): home + clear + redraw.
    let frame = "\x1b[H\x1b[2J";
    for (let r = 0; r < Math.min(rows, 20); r += 1) {
      frame += `\x1b[${r + 1};1H${markerLine(9000 + r, cols)}`;
    }
    await emitTerminalOutput(page, frame);
    await page.waitForTimeout(50);

    // Keyboard freeze mid-scroll, then release (resize flush).
    await page.evaluate(() => {
      document.documentElement.classList.add("keyboard-open");
      document.documentElement.style.setProperty(
        "--app-height",
        `${window.innerHeight - 336}px`,
      );
      window.dispatchEvent(new Event("resize"));
      window.visualViewport?.dispatchEvent(new Event("resize"));
    });
    await page.waitForTimeout(100);
    await page.evaluate(() => {
      document.documentElement.classList.remove("keyboard-open");
      document.documentElement.style.setProperty("--app-height", `${window.innerHeight}px`);
      window.dispatchEvent(new Event("resize"));
      window.visualViewport?.dispatchEvent(new Event("resize"));
    });
    await page.waitForTimeout(200);

    const after = await readProbe(page);
    const joined = after.lines.join("\n");
    // Markers must not shatter mid-token across newlines.
    expect(joined).not.toMatch(/MAR\nK_/);
    expect(joined).not.toMatch(/MARK\n_/);
    await saveDiagnosticScreenshot(
      page,
      "crates/ajax-web/web/e2e/artifacts/garble-csi-redraw-while-scrolled.png",
    );
  });

  test("long soft-wrapped Claude-like paths show wrap column (hypothesis 3)", async ({
    page,
  }) => {
    await openTaskTerminal(page);
    const { cols } = await readProbe(page);

    // Intentionally longer than cols: proves narrow-fit soft wrap vs garble.
    const longPath =
      "crates/ajax-core/src/registry/sqlite.rs crates/ajax-tui/src/lib.rs crates/ajax-web/src/runtime.rs";
    await emitTerminalOutput(page, `${longPath}\r\n… +16 lines (ctrl+o to expand)\r\n`);
    await expect
      .poll(async () => {
        const lines = (await readProbe(page)).lines.join("\n");
        return lines.includes("ctrl+o") || lines.includes("ajax-core");
      }, { timeout: 5_000 })
      .toBe(true);

    const after = await readProbe(page);
    const joined = after.lines.join("\n");
    const softWrapped = after.lines.some(
      (line) =>
        (line.includes("li") && !line.includes("lib.rs")) ||
        (line.includes("aja") && !line.includes("ajax-core")) ||
        (line.includes("(ct") && !line.includes("ctrl+o")),
    );

    await saveDiagnosticScreenshot(
      page,
      "crates/ajax-web/web/e2e/artifacts/garble-softwrap-hypothesis.png",
    );

    // Diagnostic: record wrap behavior for the plan ledger; do not fail the
    // suite solely because fit geometry soft-wraps long paths.
    const softwrapInfo = {
      cols,
      softWrapped,
      sample: after.lines.filter((l) => l.length > 0).slice(0, 12),
      hasCtrl: joined.includes("ctrl+o"),
      hasLib: joined.includes("lib.rs"),
      ctrlSplit: after.lines.some((l) => /ct$/.test(l)) && after.lines.some((l) => /^rl\+o/.test(l)),
      libSplit: after.lines.some((l) => /\/li$/.test(l)) && after.lines.some((l) => /^b\.rs/.test(l)),
      metrics: await page.evaluate(() => {
        const host = document.querySelector(".terminal-host") as HTMLElement | null;
        const canvas = host?.querySelector("canvas:not([aria-hidden='true'])") as HTMLCanvasElement | null;
        const termRoot = canvas?.parentElement as HTMLElement | null;
        const probe = (window as unknown as { __ajaxTerminalProbe?: TerminalProbe }).__ajaxTerminalProbe;
        if (!host || !canvas || !probe) return null;
        const transform = termRoot?.style.transform ?? "";
        const scaleMatch = transform.match(/scale\(([\d.]+)\)/);
        const fitScale = scaleMatch ? Number(scaleMatch[1]) : 1;
        return {
          hostClientWidth: host.clientWidth,
          hostScrollWidth: host.scrollWidth,
          hostScrollLeft: host.scrollLeft,
          canvasWidth: canvas.width,
          canvasClientWidth: canvas.clientWidth,
          canvasStyleWidth: canvas.style.width,
          fitScale,
          visualCanvasWidth: canvas.clientWidth * fitScale,
          cols: probe.cols(),
          rows: probe.rows(),
        };
      }),
    };
    console.log("SOFTWRAP_DIAG", JSON.stringify(softwrapInfo));
    test.info().annotations.push({
      type: "softwrap",
      description: JSON.stringify(softwrapInfo),
    });
    expect(cols).toBeGreaterThanOrEqual(80);
    expect(joined.length).toBeGreaterThan(0);

    // Agent-sized layout: logical cols stay at 80+; scale-to-fit avoids mid-token
    // soft wrap at ~43 cols ("… cra" / "tes/…").
    expect(softwrapInfo.sample.some((l) => / cra$/.test(l))).toBe(false);
    expect(softwrapInfo.sample.some((l) => /^tes\//.test(l))).toBe(false);

    const resizeCols = (await terminalFrames(page))
      .filter((frame) => (frame as { type?: string }).type === "resize")
      .map((frame) => (frame as { cols?: number }).cols ?? 0);
    expect(resizeCols.some((c) => c >= 80)).toBe(true);

    // Load-bearing: visual canvas (after CSS scale) must fit the host clip.
    const metrics = softwrapInfo.metrics;
    expect(metrics).not.toBeNull();
    expect(metrics!.hostScrollLeft).toBe(0);
    expect(metrics!.cols).toBeGreaterThanOrEqual(80);
    if (metrics!.fitScale < 1) {
      expect(metrics!.fitScale).toBeGreaterThan(0);
      expect(metrics!.visualCanvasWidth).toBeLessThanOrEqual(metrics!.hostClientWidth + 2);
    } else if (metrics!.canvasClientWidth > 0 && metrics!.hostClientWidth > 0) {
      expect(metrics!.canvasClientWidth).toBeLessThanOrEqual(metrics!.hostClientWidth + 2);
    }
  });
});
