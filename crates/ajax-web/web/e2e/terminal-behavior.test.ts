// Permanent iOS-WebKit terminal behavior suite. The first test pins the
// engine-neutral application-surface locator and a single task-terminal
// WebSocket opening on the task route, without asserting on engine-specific
// DOM or renderer internals.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInteractionSurface,
  terminalToolbar,
  terminalInputFrames,
  terminalResizeFrames,
  terminalSocketSummaries,
  openLatestTerminalSocket,
  closeLatestTerminalSocket,
  failLatestTerminalSocket,
  emitLatestTerminalOutput,
  waitForTerminalSocket,
  dispatchViewportEvents,
  syntheticOutwardPinchOnInteractionSurface,
  type ViewportEventKind,
} from "./fixtures";

const OPEN = 1;

async function activeTaskSocketCount(page: import("@playwright/test").Page) {
  const summaries = await terminalSocketSummaries(page);
  return summaries.filter((socket) => socket.readyState === OPEN).length;
}

async function gotoTaskRoute(page: import("@playwright/test").Page) {
  await page.goto("/app.html#/t/web%2Ffix-login");
}

async function clickTerminalSurfaceInterior(page: import("@playwright/test").Page) {
  const surface = terminalSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("terminal surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function inputFrameCount(page: import("@playwright/test").Page) {
  return (await terminalInputFrames(page)).length;
}

type TerminalSize = { cols: number; rows: number };

function hasAdjacentDuplicateSizes(frames: TerminalSize[]): boolean {
  for (let index = 1; index < frames.length; index += 1) {
    const previous = frames[index - 1];
    const current = frames[index];
    if (previous.cols === current.cols && previous.rows === current.rows) return true;
  }
  return false;
}

/** scheduleBandSettle: immediate + 2 rAF + EXPAND_REWRAP_MS (280) discreteIntent. */
const BAND_SETTLE_RESIZE_BUDGET = 4;

async function waitForResizeFrameCountStable(
  page: import("@playwright/test").Page,
  timeoutMs = 1500,
) {
  await expect
    .poll(
      async () => {
        const before = (await terminalResizeFrames(page)).length;
        await new Promise((resolve) => setTimeout(resolve, 80));
        return (await terminalResizeFrames(page)).length === before;
      },
      { timeout: timeoutMs },
    )
    .toBe(true);
}

async function openKeyboardBandForResizeTests(page: import("@playwright/test").Page) {
  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty(
      "--app-height",
      `${Math.max(0, window.innerHeight - 336)}px`,
    );
  });
  await page.setViewportSize({ width: 390, height: 508 });
  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);
  await waitForResizeFrameCountStable(page);
}

function sizesEqual(left: TerminalSize | undefined, right: TerminalSize | undefined): boolean {
  return !!left && !!right && left.cols === right.cols && left.rows === right.rows;
}

const VIEWPORT_EVENT_BURST: ViewportEventKind[] = [
  "resize",
  "orientationchange",
  "visualViewport.resize",
  "resize",
  "visualViewport.resize",
  "orientationchange",
  "resize",
  "visualViewport.resize",
];

async function openTaskTerminal(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}

const expandTerminalButton = (page: import("@playwright/test").Page) =>
  terminalSurface(page).getByRole("button", { name: "Expand terminal" });

const newOutputButton = (page: import("@playwright/test").Page) =>
  page.getByRole("button", { name: "New output ↓" });

function scrollbackChunk(from: number, count: number): string {
  let out = "";
  for (let i = from; i < from + count; i += 1) {
    out += `row ${i}\r\n`;
  }
  return out;
}

async function scrollInteractionSurfaceAway(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  await surface.evaluate((el) => {
    el.scrollTop = Math.max(0, el.scrollTop - 12 * 18);
    el.dispatchEvent(new Event("scroll"));
  });
}

async function clickInteractionSurfaceCenter(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function documentScrollPosition(page: import("@playwright/test").Page) {
  return page.evaluate(() => ({
    windowY: window.scrollY,
    documentY: document.documentElement.scrollTop,
  }));
}

async function syntheticScrollGestureOnInteractionSurface(page: import("@playwright/test").Page) {
  const surface = terminalInteractionSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("interaction surface box missing");
  // Playwright dragTo uses synthetic mouse events, not native iOS touch or momentum.
  await surface.dragTo(surface, {
    sourcePosition: { x: box.width / 2, y: box.height * 0.8 },
    targetPosition: { x: box.width / 2, y: box.height * 0.2 },
  });
}

async function longPressInteractionSurface(
  page: import("@playwright/test").Page,
  position?: { x: number; y: number },
) {
  const surface = terminalInteractionSurface(page);
  await surface.evaluate(
    async (el, pos: { x: number; y: number } | null) => {
      const rect = el.getBoundingClientRect();
      const clientX = rect.left + (pos?.x ?? rect.width / 2);
      const clientY = rect.top + (pos?.y ?? rect.height / 2);
      const touch = { clientX, clientY, identifier: 0, target: el };
      const makeTouch = (type: string, touches: typeof touch[]) => {
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperty(event, "touches", { value: touches });
        Object.defineProperty(event, "changedTouches", { value: touches });
        return event;
      };
      el.dispatchEvent(makeTouch("touchstart", [touch]));
      // Hold past LONG_PRESS_MS with headroom for CI timer delay.
      await new Promise((resolve) => setTimeout(resolve, 750));
      el.dispatchEvent(makeTouch("touchend", []));
    },
    position ?? null,
  );
}

/** Press the center of the first cell of `needle` using live xterm screen metrics. */
async function longPressTerminalText(
  page: import("@playwright/test").Page,
  needle: string,
) {
  const surface = terminalInteractionSurface(page);
  await expect
    .poll(async () =>
      page.evaluate((text) => {
        const host = document.querySelector(
          "[data-testid='task-terminal-panel'] .terminal-host",
        ) as (HTMLElement & {
          __xterm?: {
            buffer: {
              active: {
                length: number;
                getLine: (r: number) => { translateToString: (trim: boolean) => string } | undefined;
              };
            };
          };
        }) | null;
        const term = host?.__xterm;
        if (!term) return false;
        for (let row = 0; row < term.buffer.active.length; row += 1) {
          const line = term.buffer.active.getLine(row);
          if (line?.translateToString(true).includes(text)) return true;
        }
        return false;
      }, needle),
    )
    .toBe(true);

  const pos = await page.evaluate((text) => {
    const host = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-host",
    ) as (HTMLElement & {
      __xterm?: {
        cols: number;
        rows: number;
        element: HTMLElement | undefined;
        buffer: {
          active: {
            viewportY: number;
            length: number;
            getLine: (r: number) => { translateToString: (trim: boolean) => string } | undefined;
          };
        };
      };
    }) | null;
    const term = host?.__xterm;
    const surfaceEl = document.querySelector(
      "[data-testid='terminal-interaction-surface']",
    ) as HTMLElement | null;
    if (!term?.element || !surfaceEl || term.cols <= 0 || term.rows <= 0) {
      throw new Error("terminal metrics missing for long-press");
    }
    let bufferRow = -1;
    let col = -1;
    for (let row = 0; row < term.buffer.active.length; row += 1) {
      const line = term.buffer.active.getLine(row);
      if (!line) continue;
      const idx = line.translateToString(true).indexOf(text);
      if (idx >= 0) {
        bufferRow = row;
        col = idx;
        break;
      }
    }
    if (bufferRow < 0 || col < 0) throw new Error(`text not in buffer: ${text}`);

    const screenEl = term.element.querySelector(".xterm-screen") as HTMLElement | null;
    const bounds = (screenEl ?? host!).getBoundingClientRect();
    const surfaceRect = surfaceEl.getBoundingClientRect();
    const cellWidth = bounds.width / term.cols;
    const cellHeight = bounds.height / term.rows;
    const rowInView = bufferRow - term.buffer.active.viewportY;
    const clientX = bounds.left + (col + 0.5) * cellWidth;
    const clientY = bounds.top + (rowInView + 0.5) * cellHeight;
    return {
      x: clientX - surfaceRect.left,
      y: clientY - surfaceRect.top,
    };
  }, needle);

  await longPressInteractionSurface(page, pos);
}

const COPY_SELECTION_TEXT = "selectable-copy-me";

const terminalPanel = (page: import("@playwright/test").Page) =>
  page.getByTestId("task-terminal-panel");

async function installCopyClipboardSpy(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    const writes: string[] = [];
    Object.defineProperty(window, "__clipboardWrites", { value: writes, configurable: true });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text: string) => {
          writes.push(text);
        },
        readText: async () => "echo pasted",
      },
    });
  });
}

async function openTaskTerminalWithCopySpy(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await installCopyClipboardSpy(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}

async function openTaskTerminalWithCopyFailure(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await installCopyClipboardFailure(page);
  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  return surface;
}

async function installCopyClipboardFailure(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    document.execCommand = () => false;
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async () => {
          throw new Error("clipboard denied");
        },
        readText: async () => "echo pasted",
      },
    });
  });
}

const clipboardWrites = (page: import("@playwright/test").Page) =>
  page.evaluate(() => (window as unknown as { __clipboardWrites: string[] }).__clipboardWrites);

async function programTerminalSelection(
  page: import("@playwright/test").Page,
  needle: string,
) {
  const selected = await page.evaluate((text) => {
    const host = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-host",
    ) as (HTMLElement & { __xterm?: { select: (c: number, r: number, l: number) => void; getSelection: () => string; buffer: { active: { length: number; getLine: (r: number) => { translateToString: (trim: boolean) => string } | undefined } } } }) | null;
    const term = host?.__xterm;
    if (!term || typeof term.select !== "function") {
      throw new Error("task terminal xterm instance missing");
    }
    const buffer = term.buffer.active;
    for (let row = 0; row < buffer.length; row += 1) {
      const line = buffer.getLine(row);
      if (!line) continue;
      const str = line.translateToString(true);
      const col = str.indexOf(text);
      if (col >= 0) {
        term.select(col, row, text.length);
        return term.getSelection();
      }
    }
    throw new Error(`terminal text not found: ${text}`);
  }, needle);
  return selected;
}

test("task route mounts one terminal surface and opens one socket", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);

  await waitForTerminalSocket(page);

  const sockets = await terminalSocketSummaries(page);
  expect(sockets).toHaveLength(1);
});

test("delayed socket open shows Connecting then connects", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });

  const status = page.getByTestId("terminal-status");
  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(page.getByRole("button", { name: "Reconnect" })).not.toBeVisible();

  await openLatestTerminalSocket(page);

  await expect(status).toHaveAttribute("aria-hidden", "true");
  await expect(page.getByRole("button", { name: "Reconnect" })).not.toBeVisible();
});

test("socket close reconnects, server error becomes unavailable, and manual reconnect recovers", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const status = page.getByTestId("terminal-status");
  const reconnect = page.getByRole("button", { name: "Reconnect" });

  await closeLatestTerminalSocket(page);

  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(reconnect).toBeVisible();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);

  await openLatestTerminalSocket(page);
  await failLatestTerminalSocket(page, "tmux session missing");

  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(reconnect).toBeVisible();

  await reconnect.click();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(3);

  await openLatestTerminalSocket(page);

  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("navigation away closes the active socket and removes the surface", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await expect(surface).not.toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(0);
});

const PTY_OUTPUT_CORPUS_CHUNKS: Array<string | number[]> = [
  "ASCII",
  [...new TextEncoder().encode("😀")],
  [...new TextEncoder().encode("e\u0301")],
  [...new TextEncoder().encode("漢")],
  "\x1b[31mRED\x1b[0m\x1b[2K",
  "carriage\rreturn",
  "line\nbreak",
  "crlf\r\nend",
];

test("pty output corpus keeps surface connected without application errors", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});

test("reopening the task route yields one surface and one active socket", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(surface).not.toBeVisible();

  await gotoTaskRoute(page);

  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

const MULTILINE_UNICODE_CLIPBOARD = "line one\n漢字\ne\u0301";

test("printable, control, and navigation keys produce ordered PTY input", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);
  const toolbar = terminalToolbar(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("abc");
  await page.keyboard.press("Enter");

  await toolbar.getByRole("button", { name: "Tab" }).click();
  await toolbar.getByRole("button", { name: "Esc" }).click();
  await toolbar.getByRole("button", { name: "←" }).click();
  await toolbar.getByRole("button", { name: "↑" }).click();
  await toolbar.getByRole("button", { name: "↓" }).click();
  await toolbar.getByRole("button", { name: "→" }).click();

  await expect
    .poll(async () => {
      const frames = await terminalInputFrames(page);
      return frames.slice(baseline).map((frame) => frame.data);
    })
    .toEqual([
      "a",
      "b",
      "c",
      "\r",
      "\t",
      "\x1b",
      "\x1b[D",
      "\x1b[A",
      "\x1b[B",
      "\x1b[C",
    ]);
});

test("repeated printable browser events produce exact cardinality", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.press("x");
  await page.keyboard.press("x");
  await page.keyboard.press("x");

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(3);
  const frames = await terminalInputFrames(page);
  expect(frames.slice(baseline).map((frame) => frame.data)).toEqual(["x", "x", "x"]);
});

test("multiline Unicode paste preserves content in one input frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
});

test("bracketed paste wraps toolbar paste in DEC bracket mode", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, ["\x1b[?2004h"]);
  await page.evaluate(
    () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );

  const baseline = await inputFrameCount(page);
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  const bracketedText = MULTILINE_UNICODE_CLIPBOARD;
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(`\x1b[200~${bracketedText}\x1b[201~`);
});

test("clipboard fallback opens accessible paste controls when readText is unavailable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect(page.getByRole("textbox", { name: "Paste text" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
  await expect(page.getByRole("status")).toContainText(/clipboard/i);
});

test("paste fallback retains unsent multiline Unicode text when socket closes before Send", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  const input = page.getByRole("textbox", { name: "Paste text" });
  await expect(input).toBeVisible();
  await input.fill(MULTILINE_UNICODE_CLIPBOARD);

  const baseline = await inputFrameCount(page);
  await closeLatestTerminalSocket(page);

  await page.getByRole("button", { name: "Send" }).click();

  await expect(input).toBeVisible();
  await expect(input).toHaveValue(MULTILINE_UNICODE_CLIPBOARD);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
  await expect(page.getByRole("status")).toContainText(/disconnect|unavailable|reconnect/i);
  await expect(page.getByRole("button", { name: "Reconnect" })).toBeVisible();
});

test("clipboard paste retains exact text in fallback when socket is disconnected", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await closeLatestTerminalSocket(page);
  await expect(page.getByRole("button", { name: "Reconnect" })).toBeVisible();

  const baseline = await inputFrameCount(page);
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  const input = page.getByRole("textbox", { name: "Paste text" });
  await expect(input).toBeVisible();
  await expect(input).toHaveValue(MULTILINE_UNICODE_CLIPBOARD);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
  await expect(page.getByRole("status")).toContainText(/disconnect|unavailable|reconnect/i);
});

test("toolbar preserves prior terminal focus for control keys", async ({ page }) => {
  await openTaskTerminal(page);

  await page.getByRole("button", { name: "← Back" }).focus();

  await terminalToolbar(page).getByRole("button", { name: "Tab" }).click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await clickTerminalSurfaceInterior(page);

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  await terminalToolbar(page).getByRole("button", { name: "Tab" }).click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);
});

test("phone fullscreen keeps background controls inert until exit", async ({ page }) => {
  await openTaskTerminal(page);
  const expandProbe = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');

  const backProbe = page.locator(".task-detail .back");
  const dashboardProbe = page.locator('.bottom-nav [data-bottom-route="#/"]');
  const summaryProbe = page.locator(".meta-details summary");
  const dismissProbe = page.locator(".result-panel button.pill");

  await page.locator("[data-action='review']").click();
  await expect(page.locator(".result-panel")).toBeVisible({ timeout: 10_000 });

  await expandProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(expandProbe).toHaveAttribute("aria-pressed", "true");

  expect(
    await page.evaluate(() => {
      const header = document.querySelector(".task-detail .detail-header");
      const chrome = document.querySelector(".cockpit-chrome");
      const nav = document.querySelector(".bottom-nav");
      const meta = document.querySelector(".meta-details");
      const result = document.querySelector(".result-panel");
      return (
        header instanceof HTMLElement &&
        header.inert &&
        chrome instanceof HTMLElement &&
        chrome.inert &&
        nav instanceof HTMLElement &&
        nav.inert &&
        meta instanceof HTMLElement &&
        meta.inert &&
        result instanceof HTMLElement &&
        result.inert
      );
    }),
  ).toBe(true);

  await backProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".task-detail .back") === document.activeElement,
    ),
  ).toBe(false);

  await dismissProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".result-panel button.pill") === document.activeElement,
    ),
  ).toBe(false);

  await expandProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(expandProbe).toHaveAttribute("aria-pressed", "false");

  expect(
    await page.evaluate(() => {
      const header = document.querySelector(".task-detail .detail-header");
      const chrome = document.querySelector(".cockpit-chrome");
      const nav = document.querySelector(".bottom-nav");
      const meta = document.querySelector(".meta-details");
      const result = document.querySelector(".result-panel");
      return (
        header instanceof HTMLElement &&
        !header.inert &&
        chrome instanceof HTMLElement &&
        !chrome.inert &&
        nav instanceof HTMLElement &&
        !nav.inert &&
        meta instanceof HTMLElement &&
        !meta.inert &&
        result instanceof HTMLElement &&
        !result.inert
      );
    }),
  ).toBe(true);

  await backProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".task-detail .back") === document.activeElement,
    ),
  ).toBe(true);

  await dismissProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".result-panel button.pill") === document.activeElement,
    ),
  ).toBe(true);

  await summaryProbe.evaluate((el) => (el as HTMLElement).click());
  expect(
    await page.evaluate(() => document.querySelector(".meta-details")?.hasAttribute("open")),
  ).toBe(true);

  await dashboardProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(page.locator("[data-outlet='dashboard']")).toBeVisible({ timeout: 10_000 });
});

test("fullscreen exit blurs the terminal textarea without PTY input", async ({ page }) => {
  await openTaskTerminal(page);
  const expand = expandTerminalButton(page);

  await clickTerminalSurfaceInterior(page);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  const baseline = await inputFrameCount(page);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});

async function simulateKeyboardBand(
  page: import("@playwright/test").Page,
  band: { top?: number; height?: number } = {},
) {
  const top = band.top ?? 40;
  const height = band.height ?? 460;
  await page.evaluate(
    ({ top: appTop, height: appHeight }) => {
      document.documentElement.classList.add("keyboard-open");
      document.documentElement.style.setProperty("--app-height", `${appHeight}px`);
      document.documentElement.style.setProperty("--app-top", `${appTop}px`);
    },
    { top, height },
  );
}

async function clearKeyboardBand(page: import("@playwright/test").Page) {
  await page.evaluate(() => {
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
    document.documentElement.style.removeProperty("--app-top");
  });
}

async function visibleAppBand(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const top = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-top") || "0",
    );
    const height = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-height") || "0",
    );
    return { top, height, bottom: top + height };
  });
}

type BandFlushGeometry = {
  bandTop: number;
  bandBottom: number;
  bandHeight: number;
  pinnedTop: number;
  pinnedBottom: number;
  pinnedHeight: number;
  pinnedPosition: string;
  pinnedComputedTop: number;
  pinnedComputedHeight: number;
  pinnedComputedBottom: string;
  keysTop: number;
  keysBottom: number;
  keysHeight: number;
  keyboardOpen: boolean;
  expanded: boolean;
};

async function readBandFlushGeometry(
  page: import("@playwright/test").Page,
  pinnedSelector: string,
): Promise<BandFlushGeometry | null> {
  return page.evaluate((selector) => {
    const pinned = document.querySelector<HTMLElement>(selector);
    const keys = document.querySelector<HTMLElement>('[data-testid="terminal-bottom-controls"]');
    if (!pinned || !keys) return null;
    const top = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-top") || "0",
    );
    const height = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-height") || "0",
    );
    const pinnedBox = pinned.getBoundingClientRect();
    const keysBox = keys.getBoundingClientRect();
    const pinnedStyle = getComputedStyle(pinned);
    return {
      bandTop: top,
      bandBottom: top + height,
      bandHeight: height,
      pinnedTop: pinnedBox.top,
      pinnedBottom: pinnedBox.bottom,
      pinnedHeight: pinnedBox.height,
      pinnedPosition: pinnedStyle.position,
      pinnedComputedTop: Number.parseFloat(pinnedStyle.top) || 0,
      pinnedComputedHeight: Number.parseFloat(pinnedStyle.height) || 0,
      pinnedComputedBottom: pinnedStyle.bottom,
      keysTop: keysBox.top,
      keysBottom: keysBox.bottom,
      keysHeight: keysBox.height,
      keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
      expanded: document.documentElement.classList.contains("terminal-expanded"),
    };
  }, pinnedSelector);
}

function expectFlushToBand(
  geometry: BandFlushGeometry | null,
  options: { expanded: boolean; position?: string } = { expanded: false },
) {
  expect(geometry).not.toBeNull();
  const g = geometry!;
  expect(g.keyboardOpen).toBe(true);
  expect(g.expanded).toBe(options.expanded);
  expect(g.pinnedPosition).toBe(options.position ?? "fixed");
  // Height-based pin: resolved top/height track --app-* (flush above keyboard).
  expect(g.pinnedComputedTop).toBeCloseTo(g.bandTop, 0);
  expect(g.pinnedComputedHeight).toBeCloseTo(g.bandHeight, 0);
  expect(Math.abs(g.pinnedComputedHeight - g.bandHeight)).toBeLessThanOrEqual(1);
  expect(g.pinnedTop).toBeCloseTo(g.bandTop, 0);
  expect(g.pinnedBottom).toBeCloseTo(g.bandBottom, 0);
  expect(g.pinnedHeight).toBeCloseTo(g.bandHeight, 0);
  // Hotkeys flush to the band bottom (right above the keyboard).
  expect(g.keysBottom).toBeCloseTo(g.bandBottom, 0);
  expect(Math.abs(g.keysBottom - g.pinnedBottom)).toBeLessThanOrEqual(1);
  expect(g.keysTop).toBeGreaterThanOrEqual(g.bandTop - 1);
  expect(g.keysBottom).toBeLessThanOrEqual(g.bandBottom + 1);
  expect(g.keysHeight).toBeGreaterThan(0);
  expect(g.keysTop).toBeLessThan(g.keysBottom);
}

function boxesIntersect(
  box: { y: number; height: number },
  band: { top: number; bottom: number },
): boolean {
  const boxBottom = box.y + box.height;
  return box.y < band.bottom && boxBottom > band.top;
}

async function chromeDisplayState(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const cockpit = document.querySelector(".cockpit-chrome");
    const bottomNav = document.querySelector(".bottom-nav");
    const detailHeader = document.querySelector(".task-detail .detail-header");
    const interactPanel = document.querySelector(".task-detail .interact-panel");
    return {
      cockpit: cockpit ? getComputedStyle(cockpit).display : null,
      bottomNav: bottomNav ? getComputedStyle(bottomNav).display : null,
      detailHeader: detailHeader ? getComputedStyle(detailHeader).display : null,
      interactPanel: interactPanel ? getComputedStyle(interactPanel).display : null,
    };
  });
}

// Compact toolbar keys target 32px height; primary actions stay ≥ 44px.
const COMPACT_KEY_MIN_PX = 32;
const COMPACT_KEY_MAX_PX = 40;
const PRIMARY_TOUCH_MIN_PX = 44;

test("fullscreen band keeps expand tappable under keyboard-open offset band", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  const band = await visibleAppBand(page);
  const box = await expand.boundingBox();
  expect(box).not.toBeNull();
  expect(boxesIntersect(box!, band)).toBe(true);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");
});

test("fullscreen with keyboard-open pins panel bottom to the visual viewport band", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  expectFlushToBand(await readBandFlushGeometry(page, '[data-testid="task-terminal-panel"]'), {
    expanded: true,
  });
});

test("expand then keyboard-open still pins panel bottom to the visual viewport band", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  // Keyboard opens after fullscreen (tap-to-type path), not before expand.
  await simulateKeyboardBand(page);

  expectFlushToBand(await readBandFlushGeometry(page, '[data-testid="task-terminal-panel"]'), {
    expanded: true,
  });
});

test("inline keyboard-open pins task-detail to the visual viewport band", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });

  const chrome = await chromeDisplayState(page);
  expect(chrome.detailHeader).not.toBe("none");
  expect(chrome.interactPanel).not.toBe("none");
});

test("keyboard band CSS uses height pin and forbids 100lvh bottom math", async ({ page }) => {
  await openTaskTerminal(page);

  const contract = await page.evaluate(() => {
    const texts = Array.from(document.styleSheets).flatMap((sheet) => {
      try {
        return Array.from(sheet.cssRules).map((rule) => rule.cssText);
      } catch {
        return [] as string[];
      }
    });
    const joined = texts.join("\n");
    return {
      hasLvhBottom: /bottom:\s*max\(\s*0px,\s*calc\(\s*100lvh\s*-\s*var\(--app-top/.test(joined),
      taskDetailHeightPin:
        /keyboard-open:not\(\.terminal-expanded\)[\s\S]*?\.task-detail[\s\S]*?height:\s*var\(--app-height/.test(
          joined,
        ) ||
        /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail/.test(joined),
      appHeightVarInUse: /height:\s*var\(--app-height/.test(joined),
    };
  });

  expect(contract.hasLvhBottom).toBe(false);
  expect(contract.appHeightVarInUse).toBe(true);
});

test("exit fullscreen while keyboard-open pins inline task-detail to the band", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });
});

test("keyboard-open pin tracks live visual-viewport band CSS updates", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page, { top: 40, height: 460 });
  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });

  await simulateKeyboardBand(page, { top: 72, height: 390 });
  const after = await readBandFlushGeometry(page, ".task-detail");
  expectFlushToBand(after, { expanded: false });
  expect(after!.bandTop).toBe(72);
  expect(after!.bandHeight).toBe(390);
  expect(after!.pinnedTop).toBeCloseTo(72, 0);
  expect(after!.pinnedHeight).toBeCloseTo(390, 0);
});

test("keyboard close then reopen still pins inline task-detail flush to the band", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);
  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });

  await clearKeyboardBand(page);
  const cleared = await page.evaluate(() => ({
    keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
    detailPosition: getComputedStyle(document.querySelector(".task-detail")!).position,
  }));
  expect(cleared.keyboardOpen).toBe(false);
  expect(cleared.detailPosition).not.toBe("fixed");

  await simulateKeyboardBand(page, { top: 48, height: 420 });
  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });
});

test("keyboard-open hides cockpit chrome and bottom nav on task route", async ({ page }) => {
  await openTaskTerminal(page);

  const before = await chromeDisplayState(page);
  expect(before.cockpit).not.toBe("none");
  expect(before.bottomNav).not.toBe("none");

  await simulateKeyboardBand(page);
  const hidden = await chromeDisplayState(page);
  expect(hidden.cockpit).toBe("none");
  expect(hidden.bottomNav).toBe("none");
  expect(hidden.detailHeader).not.toBe("none");
  expect(hidden.interactPanel).not.toBe("none");

  await clearKeyboardBand(page);
  const restored = await chromeDisplayState(page);
  expect(restored.cockpit).not.toBe("none");
  expect(restored.bottomNav).not.toBe("none");
});

test("terminal-expanded hides cockpit chrome and bottom nav on task route", async ({ page }) => {
  await openTaskTerminal(page);

  const before = await chromeDisplayState(page);
  expect(before.cockpit).not.toBe("none");
  expect(before.bottomNav).not.toBe("none");

  await page.evaluate(() => {
    document.documentElement.classList.add("terminal-expanded");
  });
  const hidden = await chromeDisplayState(page);
  expect(hidden.cockpit).toBe("none");
  expect(hidden.bottomNav).toBe("none");
  expect(hidden.detailHeader).toBe("none");
  expect(hidden.interactPanel).toBe("none");

  await page.evaluate(() => {
    document.documentElement.classList.remove("terminal-expanded");
  });
  const restored = await chromeDisplayState(page);
  expect(restored.cockpit).not.toBe("none");
  expect(restored.bottomNav).not.toBe("none");
});

test("interaction wrap hides scrollbar chrome", async ({ page }) => {
  await openTaskTerminal(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 120)]);

  const wrap = terminalInteractionSurface(page);
  await expect(wrap).toHaveCSS("scrollbar-width", "none");

  const webkitHidden = await wrap.evaluate((el) => {
    const rules = Array.from(document.styleSheets).flatMap((sheet) => {
      try {
        return Array.from(sheet.cssRules);
      } catch {
        return [];
      }
    });
    const selectorMatches = rules.some(
      (rule) =>
        rule instanceof CSSStyleRule &&
        rule.selectorText.includes(".terminal-interaction-wrap") &&
        rule.selectorText.includes("::-webkit-scrollbar") &&
        rule.style.display === "none",
    );
    return selectorMatches;
  });
  expect(webkitHidden).toBe(true);
});

test("compact terminal keys are smaller than primary touch targets on phone", async ({ page }) => {
  await openTaskTerminal(page);

  const keySizes = await page.evaluate(() =>
    Array.from(document.querySelectorAll(".terminal-keys .terminal-key")).map((el) => {
      const rect = (el as HTMLElement).getBoundingClientRect();
      return { width: rect.width, height: rect.height };
    }),
  );
  expect(keySizes.length).toBeGreaterThan(0);
  for (const size of keySizes) {
    expect(size.height).toBeGreaterThanOrEqual(COMPACT_KEY_MIN_PX);
    expect(size.height).toBeLessThanOrEqual(COMPACT_KEY_MAX_PX);
    expect(size.height).toBeLessThan(PRIMARY_TOUCH_MIN_PX);
  }
});

test("terminal controls meet mobile touch target size on phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const measureVisibleTerminalButtons = () =>
    page.evaluate(() => {
      const panel = document.querySelector("[data-testid='task-terminal-panel']");
      if (!panel) throw new Error("terminal panel missing");
      const measured: Array<{ width: number; height: number; selector: string }> = [];
      const selectors = [
        ".terminal-expand-corner",
        ".terminal-keys .terminal-key",
        ".terminal-new-output",
        ".terminal-status-reconnect",
        ".terminal-paste-actions .terminal-key",
      ];
      for (const selector of selectors) {
        for (const el of panel.querySelectorAll(selector)) {
          const rect = (el as HTMLElement).getBoundingClientRect();
          measured.push({
            selector,
            width: rect.width,
            height: rect.height,
          });
        }
      }
      return measured;
    });

  const expectPrimaryTouchTargets = (
    sizes: Array<{ width: number; height: number; selector: string }>,
    requiredSelectors: string[],
  ) => {
    expect(sizes.length).toBeGreaterThan(0);
    for (const size of sizes) {
      if (size.selector === ".terminal-keys .terminal-key") {
        expect(size.height).toBeGreaterThanOrEqual(COMPACT_KEY_MIN_PX);
        expect(size.height).toBeLessThanOrEqual(COMPACT_KEY_MAX_PX);
        continue;
      }
      expect(size.width).toBeGreaterThanOrEqual(PRIMARY_TOUCH_MIN_PX);
      expect(size.height).toBeGreaterThanOrEqual(PRIMARY_TOUCH_MIN_PX);
    }
    for (const selector of requiredSelectors) {
      expect(sizes.some((size) => size.selector === selector)).toBe(true);
    }
  };

  let sizes = await measureVisibleTerminalButtons();
  expectPrimaryTouchTargets(sizes, [".terminal-expand-corner", ".terminal-keys .terminal-key"]);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await scrollInteractionSurfaceAway(page);
  await emitLatestTerminalOutput(page, ["more output\r\n"]);
  const newOutput = newOutputButton(page);
  await expect(newOutput).toBeVisible();
  sizes = await measureVisibleTerminalButtons();
  expectPrimaryTouchTargets(sizes, [".terminal-new-output"]);

  await failLatestTerminalSocket(page, "tmux session missing");
  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();
  sizes = await measureVisibleTerminalButtons();
  expectPrimaryTouchTargets(sizes, [".terminal-status-reconnect"]);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
  sizes = await measureVisibleTerminalButtons();
  expect(
    sizes.filter((size) => size.selector === ".terminal-paste-actions .terminal-key").length,
  ).toBe(2);
  expectPrimaryTouchTargets(sizes, [".terminal-paste-actions .terminal-key"]);
});

test("paste fallback preserves prior terminal focus when another control owns focus", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.getByRole("button", { name: "← Back" }).focus();

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();

  await page.getByRole("button", { name: "Cancel" }).click();
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await page.getByRole("button", { name: "← Back" }).focus();
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await page.getByRole("textbox", { name: "Paste text" }).fill("fallback-text");

  const baseline = await inputFrameCount(page);
  await page.getByRole("button", { name: "Send" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);
});

test("paste fallback restores terminal focus when terminal owned focus", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await clickTerminalSurfaceInterior(page);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();

  await page.getByRole("button", { name: "Cancel" }).click();
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);
});

test("keyboard activation does not reuse pointer focus ownership", async ({ page }) => {
  await openTaskTerminal(page);
  const toolbar = terminalToolbar(page);
  const tab = toolbar.getByRole("button", { name: "Tab" });
  const esc = toolbar.getByRole("button", { name: "Esc" });

  await clickTerminalSurfaceInterior(page);
  await tab.click();

  await page.getByRole("button", { name: "← Back" }).focus();

  await tab.focus();
  await page.keyboard.press("Enter");

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await esc.focus();
  await page.keyboard.press("Space");

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);
});

test("Paste preserves prior terminal focus when another control owns focus", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: "paste-me" });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);
  await page.getByRole("button", { name: "← Back" }).focus();

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);
});

test("Hide keyboard focus blur adds no PTY input", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await terminalToolbar(page).getByRole("button", { name: "Hide keyboard" }).click();

  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});

test("typing after manual reconnect sends exactly one input frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await failLatestTerminalSocket(page, "tmux session missing");

  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();

  await reconnect.click();
  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);
  await openLatestTerminalSocket(page);
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("!");

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe("!");
});

test("seeded reconnect restores live follow at the interaction surface bottom", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await expect(newOutputButton(page)).not.toBeVisible();

  await scrollInteractionSurfaceAway(page);

  await failLatestTerminalSocket(page, "tmux session missing");

  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();
  await reconnect.click();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);
  await openLatestTerminalSocket(page);
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 50), "seeded live tail\r\n"]);

  await expect(newOutputButton(page)).not.toBeVisible();
  await expect
    .poll(async () =>
      terminalInteractionSurface(page).evaluate(
        (el) =>
          el.scrollHeight <= el.clientHeight + 1 ||
          el.scrollTop + el.clientHeight >= el.scrollHeight - 1,
      ),
    )
    .toBe(true);
});

test("initial open eventually sends at least one valid positive-integer PTY size", async ({ page }) => {
  await openTaskTerminal(page);

  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);
  const frames = await terminalResizeFrames(page);
  for (const frame of frames) {
    expect(frame.cols).toBeGreaterThan(0);
    expect(frame.rows).toBeGreaterThan(0);
    expect(Number.isInteger(frame.cols)).toBe(true);
    expect(Number.isInteger(frame.rows)).toBe(true);
  }
});

async function readLogicalXtermGeometry(page: import("@playwright/test").Page) {
  return page.locator("[data-testid='task-terminal-panel'] .terminal-host .xterm").evaluate((xtermEl) => {
    const host = xtermEl.parentElement as HTMLElement | null;
    const screen = xtermEl.querySelector(".xterm-screen") as HTMLElement | null;
    if (!host || !screen) throw new Error("terminal host or xterm screen missing");
    const rendered = xtermEl.getBoundingClientRect();
    return {
      hostWidth: host.clientWidth,
      hostHeight: host.clientHeight,
      logicalWidth: xtermEl.offsetWidth,
      logicalHeight: xtermEl.offsetHeight,
      screenWidth: screen.offsetWidth,
      screenHeight: screen.offsetHeight,
      renderedWidth: rendered.width,
      renderedHeight: rendered.height,
    };
  });
}

test("logical xterm grid is at least 80 columns and scales to fill the phone host", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const geometry = await readLogicalXtermGeometry(page);

  expect(geometry.screenWidth).toBeGreaterThan(geometry.hostWidth);
  expect(geometry.screenHeight).toBeGreaterThan(geometry.hostHeight);
  expect(geometry.logicalWidth).toBeGreaterThan(geometry.hostWidth);
  expect(geometry.logicalHeight).toBeGreaterThan(geometry.hostHeight);

  expect(geometry.renderedWidth).toBeGreaterThanOrEqual(geometry.hostWidth - 2);
  expect(geometry.renderedWidth).toBeLessThanOrEqual(geometry.hostWidth + 2);
  expect(geometry.renderedHeight).toBeGreaterThanOrEqual(geometry.hostHeight - 2);
  expect(geometry.renderedHeight).toBeLessThanOrEqual(geometry.hostHeight + 2);

  const lastResize = (await terminalResizeFrames(page)).at(-1)!;
  expect(lastResize.cols).toBeGreaterThanOrEqual(80);
});

test("portrait-to-landscape eventually produces a fresh valid resize without adjacent duplicate sizes", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const beforeLast = (await terminalResizeFrames(page)).at(-1);
  const sliceStart = (await terminalResizeFrames(page)).length;

  await page.setViewportSize({ width: 844, height: 390 });
  await dispatchViewportEvents(page, ["orientationchange", "resize", "visualViewport.resize"]);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, beforeLast);
    })
    .toBe(true);

  const transitionFrames = (await terminalResizeFrames(page)).slice(sliceStart);
  expect(transitionFrames.length).toBeGreaterThan(0);
  expect(hasAdjacentDuplicateSizes(transitionFrames)).toBe(false);
});

test("repeated same-dimension viewport burst then meaningful change deduplicates resize outcomes", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);
  const countBeforeBurst = (await terminalResizeFrames(page)).length;

  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);
  const countAfterBurst = (await terminalResizeFrames(page)).length;
  expect(countAfterBurst).toBe(countBeforeBurst);

  // Shrink height substantially so the flex-filled inline terminal recomputes
  // a genuinely different logical grid (rows), not a dedupe-identical one.
  await page.setViewportSize({ width: 360, height: 640 });
  await dispatchViewportEvents(page, ["resize", "visualViewport.resize"]);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  const transitionFrames = (await terminalResizeFrames(page)).slice(countBeforeBurst);
  expect(hasAdjacentDuplicateSizes(transitionFrames)).toBe(false);
});

test("keyboard-open resize burst does not storm PTY resize; closing eventually settles without adjacent duplicates", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const countBeforeKeyboard = (await terminalResizeFrames(page)).length;

  await openKeyboardBandForResizeTests(page);
  await dispatchViewportEvents(page, VIEWPORT_EVENT_BURST);

  const countAfterKeyboardBurst = (await terminalResizeFrames(page)).length;
  const keyboardOpenFrames = (await terminalResizeFrames(page)).slice(
    countBeforeKeyboard,
    countAfterKeyboardBurst,
  );
  // Class-edge band settle may emit a few discreteIntent sizes; bursts must not
  // storm beyond that budget.
  expect(keyboardOpenFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(keyboardOpenFrames)).toBe(false);

  await page.evaluate(() => {
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
  });
  await page.setViewportSize({ width: 390, height: 800 });
  await dispatchViewportEvents(page, ["visualViewport.resize", "resize", "orientationchange"]);

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(countAfterKeyboardBurst);

  const afterCloseFrames = (await terminalResizeFrames(page)).slice(countAfterKeyboardBurst);
  expect(afterCloseFrames.length).toBeGreaterThan(0);
  expect(hasAdjacentDuplicateSizes(afterCloseFrames)).toBe(false);
});

test("keyboard-open expand enters fullscreen with a bounded discreteIntent settle while keyboard stays open", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);
  const countBeforeKeyboard = (await terminalResizeFrames(page)).length;

  await openKeyboardBandForResizeTests(page);

  const keyboardEdgeFrames = (await terminalResizeFrames(page)).slice(countBeforeKeyboard);
  expect(keyboardEdgeFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(keyboardEdgeFrames)).toBe(false);

  const countBeforeExpand = (await terminalResizeFrames(page)).length;
  const expand = expandTerminalButton(page);
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return frames.length > countBeforeExpand && !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  await waitForResizeFrameCountStable(page);

  const expandFrames = (await terminalResizeFrames(page)).slice(countBeforeExpand);
  expect(expandFrames.length).toBeGreaterThanOrEqual(1);
  expect(expandFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(expandFrames)).toBe(false);
  const expandFrame = expandFrames.at(-1)!;
  expect(sizesEqual(expandFrame, settledBefore!)).toBe(false);
  expect(expandFrame.cols).toBeGreaterThan(0);
  expect(expandFrame.rows).toBeGreaterThan(0);
  expect(Number.isInteger(expandFrame.cols)).toBe(true);
  expect(Number.isInteger(expandFrame.rows)).toBe(true);
  expect(
    await page.evaluate(() => document.documentElement.classList.contains("keyboard-open")),
  ).toBe(true);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("keyboard-open pinch-end produces a bounded fresh PTY resize while keyboard stays open", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);
  const countBeforeKeyboard = (await terminalResizeFrames(page)).length;

  await openKeyboardBandForResizeTests(page);

  const keyboardEdgeFrames = (await terminalResizeFrames(page)).slice(countBeforeKeyboard);
  expect(keyboardEdgeFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(keyboardEdgeFrames)).toBe(false);

  const countBeforePinch = (await terminalResizeFrames(page)).length;

  await syntheticOutwardPinchOnInteractionSurface(page);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return frames.length > countBeforePinch && !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  await waitForResizeFrameCountStable(page);

  const pinchFrames = (await terminalResizeFrames(page)).slice(countBeforePinch);
  expect(pinchFrames.length).toBeGreaterThanOrEqual(1);
  expect(pinchFrames.length).toBeLessThanOrEqual(BAND_SETTLE_RESIZE_BUDGET);
  expect(hasAdjacentDuplicateSizes(pinchFrames)).toBe(false);
  const pinchFrame = pinchFrames.at(-1)!;
  expect(sizesEqual(pinchFrame, settledBefore!)).toBe(false);
  expect(pinchFrame.cols).toBeGreaterThan(0);
  expect(pinchFrame.rows).toBeGreaterThan(0);
  expect(Number.isInteger(pinchFrame.cols)).toBe(true);
  expect(Number.isInteger(pinchFrame.rows)).toBe(true);
  expect(
    await page.evaluate(() => document.documentElement.classList.contains("keyboard-open")),
  ).toBe(true);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("scheduled terminal work does not survive disposal after immediate navigation away", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  await expandTerminalButton(page).click();
  await page.goto("/app.html#/");

  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => resolve());
        });
      }),
  );

  expect(pageErrors).toEqual([]);
  await expect(terminalSurface(page)).not.toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(0);
});

test("fullscreen enter and exit each produce a fresh valid resize and retain one active socket", async ({
  page,
}) => {
  const surface = await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const expand = expandTerminalButton(page);
  const countBeforeExpand = (await terminalResizeFrames(page)).length;

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(countBeforeExpand);

  const expandedLast = (await terminalResizeFrames(page)).at(-1)!;
  expect(expandedLast.cols).toBeGreaterThan(0);
  expect(expandedLast.rows).toBeGreaterThan(0);

  const countAfterExpand = (await terminalResizeFrames(page)).length;
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(countAfterExpand);

  const exitFrames = (await terminalResizeFrames(page)).slice(countAfterExpand);
  expect(hasAdjacentDuplicateSizes(exitFrames)).toBe(false);

  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("reopen with meaningful viewport change yields one surface and deduplicated resize outcomes", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const framesBeforeNav = (await terminalResizeFrames(page)).length;

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(terminalSurface(page)).not.toBeVisible();

  await gotoTaskRoute(page);
  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);

  await expect
    .poll(async () => (await terminalResizeFrames(page)).length)
    .toBeGreaterThan(framesBeforeNav);

  const settledBeforeChange = (await terminalResizeFrames(page)).at(-1);
  const sliceStart = (await terminalResizeFrames(page)).length;

  await page.setViewportSize({ width: 375, height: 812 });
  await dispatchViewportEvents(page, ["resize", "visualViewport.resize", "orientationchange"]);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBeforeChange);
    })
    .toBe(true);

  const changeFrames = (await terminalResizeFrames(page)).slice(sliceStart);
  expect(hasAdjacentDuplicateSizes(changeFrames)).toBe(false);

  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("desktop expanded mode keeps terminal bounded and task details summary reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const expand = expandTerminalButton(page);
  const maxInteractionHeight = Math.min(800 * 0.58, 560);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await expect
    .poll(async () =>
      terminalInteractionSurface(page).evaluate((el) => el.getBoundingClientRect().height),
    )
    .toBeLessThanOrEqual(maxInteractionHeight + 2);

  const summary = page.locator(".meta-details summary");
  await summary.scrollIntoViewIfNeeded();
  await expect(summary).toBeInViewport();
});

test("task route exposes a stable terminal interaction surface locator", async ({ page }) => {
  await openTaskTerminal(page);
  await expect(terminalInteractionSurface(page)).toBeVisible();
});

test("reading scrollback shows New output and restoring live output sends no PTY input", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await expect(newOutputButton(page)).not.toBeVisible();

  await scrollInteractionSurfaceAway(page);

  const baseline = await inputFrameCount(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(200, 40)]);

  const newOutput = newOutputButton(page);
  await expect(newOutput).toBeVisible();
  await newOutput.click();
  await expect(newOutput).not.toBeVisible();
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});

test("New output click does not refocus xterm or reopen keyboard, and direct surface click focuses without scrolling", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const isTermFocused = () =>
    page.evaluate(() => {
      const textarea = document.querySelector(
        ".terminal-host textarea.xterm-helper-textarea",
      );
      return textarea === document.activeElement;
    });
  const isKeyboardOpen = () =>
    page.evaluate(() => document.documentElement.classList.contains("keyboard-open"));

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await scrollInteractionSurfaceAway(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(200, 40)]);

  const newOutput = newOutputButton(page);
  await expect(newOutput).toBeVisible();

  expect(await isTermFocused()).toBe(false);
  expect(await isKeyboardOpen()).toBe(false);

  await newOutput.click();

  expect(await isTermFocused()).toBe(false);
  expect(await isKeyboardOpen()).toBe(false);
  await expect(newOutput).not.toBeVisible();

  const scrollBefore = await documentScrollPosition(page);
  await clickInteractionSurfaceCenter(page);
  const scrollAfter = await documentScrollPosition(page);

  expect(scrollAfter).toEqual(scrollBefore);
  await expect
    .poll(async () => isTermFocused())
    .toBe(true);
});

test("long press on the interaction surface sends no PTY input", async ({ page }) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, ["selectable terminal text\r\n"]);
  const baseline = await inputFrameCount(page);

  await longPressInteractionSurface(page);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});

test("long press on known output text selects word and shows Copy control", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  const baseline = await inputFrameCount(page);

  await longPressTerminalText(page, COPY_SELECTION_TEXT);

  await expect(terminalPanel(page).getByRole("button", { name: "Copy" })).toBeVisible({
    timeout: 10_000,
  });
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});

test("non-empty xterm selection shows Copy control in terminal panel", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  await expect(terminalPanel(page).getByRole("button", { name: "Copy" })).toBeVisible();
});

test("selection Copy sits beside the fullscreen expand control", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = terminalPanel(page).getByTestId("terminal-copy-overlay");
  const expand = terminalPanel(page).locator(".terminal-expand-corner");
  await expect(copy).toBeVisible();
  await expect(expand).toBeVisible();

  const geometry = await page.evaluate(() => {
    const copyEl = document.querySelector<HTMLElement>('[data-testid="terminal-copy-overlay"]');
    const expandEl = document.querySelector<HTMLElement>(".terminal-expand-corner");
    const wrap = document.querySelector<HTMLElement>(
      '[data-testid="terminal-interaction-surface"]',
    );
    const corner = document.querySelector<HTMLElement>(".terminal-corner-actions");
    const panel = document.querySelector<HTMLElement>('[data-testid="task-terminal-panel"]');
    if (!copyEl || !expandEl || !wrap || !corner || !panel) return null;
    const copyBox = copyEl.getBoundingClientRect();
    const expandBox = expandEl.getBoundingClientRect();
    const panelBox = panel.getBoundingClientRect();
    return {
      sharedParent: copyEl.parentElement === corner && expandEl.parentElement === corner,
      copyInsideScrollWrap: wrap.contains(copyEl),
      expandInsideScrollWrap: wrap.contains(expandEl),
      copyRight: copyBox.right,
      copyTop: copyBox.top,
      copyBottom: copyBox.bottom,
      copyWidth: copyBox.width,
      copyHeight: copyBox.height,
      expandLeft: expandBox.left,
      expandTop: expandBox.top,
      expandRight: expandBox.right,
      expandHeight: expandBox.height,
      panelTop: panelBox.top,
      panelRight: panelBox.right,
      gap: expandBox.left - copyBox.right,
    };
  });

  expect(geometry).not.toBeNull();
  expect(geometry!.sharedParent).toBe(true);
  expect(geometry!.copyInsideScrollWrap).toBe(false);
  expect(geometry!.expandInsideScrollWrap).toBe(false);
  expect(geometry!.copyRight).toBeLessThanOrEqual(geometry!.expandLeft + 1);
  expect(geometry!.gap).toBeGreaterThanOrEqual(0);
  expect(geometry!.gap).toBeLessThanOrEqual(12);
  expect(Math.abs(geometry!.copyTop - geometry!.expandTop)).toBeLessThanOrEqual(4);
  expect(geometry!.copyTop).toBeGreaterThanOrEqual(geometry!.panelTop);
  expect(geometry!.copyTop).toBeLessThanOrEqual(geometry!.panelTop + 16);
  expect(geometry!.expandRight).toBeLessThanOrEqual(geometry!.panelRight + 1);
  expect(geometry!.copyWidth).toBeGreaterThanOrEqual(44);
  expect(geometry!.copyHeight).toBeGreaterThanOrEqual(44);
  expect(geometry!.expandHeight).toBeGreaterThanOrEqual(44);
});

test("selection Copy stays pinned beside expand after scrolling the interaction wrap", async ({
  page,
}) => {
  await openTaskTerminalWithCopySpy(page);
  await emitLatestTerminalOutput(page, [
    `${COPY_SELECTION_TEXT}\r\n`,
    ...Array.from({ length: 80 }, (_, i) => `scroll-line-${i}\r\n`),
  ]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = terminalPanel(page).getByTestId("terminal-copy-overlay");
  await expect(copy).toBeVisible();

  const before = await copy.boundingBox();
  expect(before).not.toBeNull();

  await terminalInteractionSurface(page).evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });

  const after = await page.evaluate(() => {
    const copyEl = document.querySelector<HTMLElement>('[data-testid="terminal-copy-overlay"]');
    const expandEl = document.querySelector<HTMLElement>(".terminal-expand-corner");
    const wrap = document.querySelector<HTMLElement>(
      '[data-testid="terminal-interaction-surface"]',
    );
    if (!copyEl || !expandEl || !wrap) return null;
    const copyBox = copyEl.getBoundingClientRect();
    const expandBox = expandEl.getBoundingClientRect();
    return {
      scrollTop: wrap.scrollTop,
      copyTop: copyBox.top,
      copyRight: copyBox.right,
      expandLeft: expandBox.left,
      expandTop: expandBox.top,
      visible: getComputedStyle(copyEl).display !== "none" && copyBox.height > 0,
    };
  });

  expect(after).not.toBeNull();
  expect(after!.scrollTop).toBeGreaterThan(0);
  expect(after!.visible).toBe(true);
  expect(after!.copyTop).toBeCloseTo(before!.y, 0);
  expect(after!.copyRight).toBeLessThanOrEqual(after!.expandLeft + 1);
  expect(Math.abs(after!.copyTop - after!.expandTop)).toBeLessThanOrEqual(4);
});

test("Copy writes selected text to clipboard and shows Copied notice", async ({ page }) => {
  await openTaskTerminalWithCopySpy(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = terminalPanel(page).getByRole("button", { name: "Copy" });
  await expect(copy).toBeVisible();
  await copy.click();

  await expect.poll(() => clipboardWrites(page)).toContain(COPY_SELECTION_TEXT);
  await expect(terminalPanel(page).getByRole("status")).toContainText("Copied");
  await expect(copy).not.toBeVisible();
});

test("Copy opens read-only fallback when clipboard write fails", async ({ page }) => {
  await openTaskTerminalWithCopyFailure(page);

  await emitLatestTerminalOutput(page, [`${COPY_SELECTION_TEXT}\r\n`]);
  await programTerminalSelection(page, COPY_SELECTION_TEXT);

  const copy = terminalPanel(page).getByRole("button", { name: "Copy" });
  await expect(copy).toBeVisible();
  await copy.click();

  const fallback = page.getByRole("textbox", { name: "Copy text" });
  await expect(fallback).toBeVisible();
  await expect(fallback).toHaveAttribute("readonly", "");
  await expect(fallback).toHaveValue(COPY_SELECTION_TEXT);
});

test("synthetic scroll gesture on the interaction surface sends no PTY input and does not move the document", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 120)]);
  const scrollBefore = await documentScrollPosition(page);
  const baseline = await inputFrameCount(page);

  await syntheticScrollGestureOnInteractionSurface(page);

  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
  const scrollAfter = await documentScrollPosition(page);
  expect(scrollAfter).toEqual(scrollBefore);
});

test("fullscreen enter and exit keep one socket, one surface, and ordered PTY input", async ({
  page,
}) => {
  const surface = await openTaskTerminal(page);
  const expand = expandTerminalButton(page);
  const baseline = await inputFrameCount(page);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("1");

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("2");

  await expect
    .poll(async () => {
      const frames = await terminalInputFrames(page);
      return frames.slice(baseline).map((frame) => frame.data);
    })
    .toEqual(["1", "2"]);

  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});

test("outward pinch on the interaction surface changes PTY size and persists across reload", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBeforePinch = (await terminalResizeFrames(page)).at(-1);
  const resizeSliceStart = (await terminalResizeFrames(page)).length;
  const inputBaseline = await inputFrameCount(page);

  await syntheticOutwardPinchOnInteractionSurface(page);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBeforePinch);
    })
    .toBe(true);

  const settledAfterPinch = (await terminalResizeFrames(page)).at(-1)!;
  const pinchFrames = (await terminalResizeFrames(page)).slice(resizeSliceStart);
  expect(hasAdjacentDuplicateSizes(pinchFrames)).toBe(false);
  expect(!sizesEqual(settledAfterPinch, settledBeforePinch)).toBe(true);

  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("p");

  await expect.poll(async () => (await inputFrameCount(page)) - inputBaseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe("p");

  await page.reload();

  await expect(terminalSurface(page)).toBeVisible({ timeout: 10_000 });
  await expect(terminalInteractionSurface(page)).toBeVisible();
  await waitForTerminalSocket(page);

  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);
  const settledAfterReload = (await terminalResizeFrames(page)).at(-1)!;
  expect(!sizesEqual(settledAfterReload, settledBeforePinch)).toBe(true);

  const reloadInputBaseline = await inputFrameCount(page);
  await clickInteractionSurfaceCenter(page);
  await page.keyboard.type("q");

  await expect.poll(async () => (await inputFrameCount(page)) - reloadInputBaseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe("q");
});

test("supported Ctrl toolbar combinations send exact control codes and disarm sticky Ctrl", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const baseline = await inputFrameCount(page);
  const toolbar = terminalToolbar(page);
  const ctrl = toolbar.getByRole("button", { name: /Ctrl/ });

  await toolbar.getByRole("button", { name: "⌃C" }).click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "false");

  await ctrl.click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "true");
  await toolbar.getByRole("button", { name: "←" }).click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "false");

  await ctrl.click();
  await expect(ctrl).toHaveAttribute("aria-pressed", "true");
  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("c");
  await expect(ctrl).toHaveAttribute("aria-pressed", "false");

  await expect
    .poll(async () => {
      const frames = await terminalInputFrames(page);
      return frames.slice(baseline).map((frame) => frame.data);
    })
    .toEqual(["\x03", "\x1b[1;5D", "\x03"]);
});

test("pty output corpus during delayed socket open keeps surface stable without application errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(1);
  const socketsBeforeOpen = await terminalSocketSummaries(page);
  expect(socketsBeforeOpen[0]?.readyState).toBe(0);

  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await openLatestTerminalSocket(page);

  const status = page.getByTestId("terminal-status");
  await expect(status).toHaveAttribute("aria-hidden", "true");
  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});

test("rapid pty output during viewport transition eventually settles resize without application errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  const surface = await openTaskTerminal(page);
  await expect.poll(async () => (await terminalResizeFrames(page)).length).toBeGreaterThan(0);

  const settledBefore = (await terminalResizeFrames(page)).at(-1);

  await page.setViewportSize({ width: 844, height: 390 });
  await dispatchViewportEvents(page, ["orientationchange", "resize", "visualViewport.resize"]);
  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);
  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await expect
    .poll(async () => {
      const frames = await terminalResizeFrames(page);
      const last = frames.at(-1);
      return !!last && !sizesEqual(last, settledBefore);
    })
    .toBe(true);

  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});

test("Paste stays available after synthetic scroll gesture and fullscreen transitions", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });
  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 80)]);
  await syntheticScrollGestureOnInteractionSurface(page);

  const expand = expandTerminalButton(page);
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  const paste = terminalToolbar(page).getByRole("button", { name: "Paste" });
  await expect(paste).toBeVisible();

  const baseline = await inputFrameCount(page);
  await paste.click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
});
